//! PersistentHashMap — 32-way HAMT + separate nil-key slot.
//!
//! Port of clojure/lang/PersistentHashMap.java. This phase (8A) lands the
//! core struct and its pymethods; protocol trait impls follow in Phase 8B
//! and the TransientHashMap variant in 8C.

use crate::associative::Associative;
use crate::coll_reduce::CollReduce;
use crate::collections::phashmap_node::{fold_hash_i64, MNode};
use crate::counted::Counted;
use crate::ikvreduce::IKVReduce;
use crate::exceptions::IllegalStateException;
use crate::ieditable_collection::IEditableCollection;
use crate::iequiv::IEquiv;
use crate::ifn::IFn;
use crate::ihasheq::IHashEq;
use crate::ilookup::ILookup;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_map::IPersistentMap;
use crate::iseqable::ISeqable;
use crate::itransient_associative::ITransientAssociative;
use crate::itransient_collection::ITransientCollection;
use crate::itransient_map::ITransientMap;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "PersistentHashMap", frozen)]
pub struct PersistentHashMap {
    pub count: u32,
    /// `None` means no non-nil entries.
    pub root: Option<Arc<MNode>>,
    pub has_null: bool,
    pub null_value: Option<PyObject>,
    /// 0 = uncomputed; else folded hash + 1.
    pub hash_cache: AtomicI64,
    pub meta: Option<PyObject>,
}

impl PersistentHashMap {
    pub fn new_empty() -> Self {
        Self {
            count: 0,
            root: None,
            has_null: false,
            null_value: None,
            hash_cache: AtomicI64::new(0),
            meta: None,
        }
    }

    fn clone_meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.as_ref().map(|o| o.clone_ref(py))
    }

    fn clone_null(&self, py: Python<'_>) -> Option<PyObject> {
        self.null_value.as_ref().map(|o| o.clone_ref(py))
    }

    pub fn val_at_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        if key.is_none(py) {
            if self.has_null {
                return Ok(self.clone_null(py).unwrap_or_else(|| py.None()));
            }
            return Ok(py.None());
        }
        let Some(root) = &self.root else {
            return Ok(py.None());
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        Ok(root.find(py, 0, h, key)?.unwrap_or_else(|| py.None()))
    }

    pub fn val_at_default_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        if key.is_none(py) {
            return Ok(if self.has_null {
                self.clone_null(py).unwrap_or(default)
            } else {
                default
            });
        }
        let Some(root) = &self.root else {
            return Ok(default);
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        root.find_or_default(py, 0, h, key, default)
    }

    pub fn contains_key_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        if key.is_none(py) {
            return Ok(self.has_null);
        }
        let Some(root) = &self.root else { return Ok(false); };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        Ok(root.find(py, 0, h, key)?.is_some())
    }

    pub fn assoc_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        val: PyObject,
    ) -> PyResult<Self> {
        if key.is_none(py) {
            let new_count = if self.has_null { self.count } else { self.count + 1 };
            return Ok(Self {
                count: new_count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: true,
                null_value: Some(val),
                hash_cache: AtomicI64::new(0),
                meta: self.clone_meta(py),
            });
        }
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let (new_root, added) = match &self.root {
            Some(r) => r.assoc(py, 0, h, key, val)?,
            None => MNode::create_leaf(py, 0, h, key, val)?,
        };
        Ok(Self {
            count: if added { self.count + 1 } else { self.count },
            root: Some(new_root),
            has_null: self.has_null,
            null_value: self.clone_null(py),
            hash_cache: AtomicI64::new(0),
            meta: self.clone_meta(py),
        })
    }

    pub fn without_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<Self> {
        if key.is_none(py) {
            if !self.has_null {
                // no-op: return clone
                return Ok(Self {
                    count: self.count,
                    root: self.root.as_ref().map(Arc::clone),
                    has_null: false,
                    null_value: None,
                    hash_cache: AtomicI64::new(0),
                    meta: self.clone_meta(py),
                });
            }
            return Ok(Self {
                count: self.count - 1,
                root: self.root.as_ref().map(Arc::clone),
                has_null: false,
                null_value: None,
                hash_cache: AtomicI64::new(0),
                meta: self.clone_meta(py),
            });
        }
        let Some(root) = &self.root else {
            return Ok(Self {
                count: self.count,
                root: None,
                has_null: self.has_null,
                null_value: self.clone_null(py),
                hash_cache: AtomicI64::new(0),
                meta: self.clone_meta(py),
            });
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let (new_root_opt, removed) = root.without(py, 0, h, key)?;
        if !removed {
            return Ok(Self {
                count: self.count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: self.has_null,
                null_value: self.clone_null(py),
                hash_cache: AtomicI64::new(0),
                meta: self.clone_meta(py),
            });
        }
        Ok(Self {
            count: self.count - 1,
            root: new_root_opt,
            has_null: self.has_null,
            null_value: self.clone_null(py),
            hash_cache: AtomicI64::new(0),
            meta: self.clone_meta(py),
        })
    }

    /// Collect every (key, value) pair, including the nil-key slot if present.
    pub fn collect_entries(&self, py: Python<'_>) -> Vec<(PyObject, PyObject)> {
        let mut out: Vec<(PyObject, PyObject)> = Vec::with_capacity(self.count as usize);
        if self.has_null {
            let v = self.clone_null(py).unwrap_or_else(|| py.None());
            out.push((py.None(), v));
        }
        if let Some(r) = &self.root {
            r.collect_entries(py, &mut out);
        }
        out
    }
}

#[pymethods]
impl PersistentHashMap {
    fn __len__(&self) -> usize {
        self.count as usize
    }

    fn __bool__(&self) -> bool {
        self.count > 0
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentHashMapKeyIter>> {
        let s = slf.bind(py).get();
        let entries = s.collect_entries(py);
        let keys: Vec<PyObject> = entries.into_iter().map(|(k, _)| k).collect();
        Py::new(py, PersistentHashMapKeyIter { keys, pos: 0 })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __getitem__(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        if !self.contains_key_internal(py, key.clone_ref(py))? {
            return Err(pyo3::exceptions::PyKeyError::new_err(
                key.bind(py).repr()?.extract::<String>()?,
            ));
        }
        self.val_at_internal(py, key)
    }

    fn __contains__(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.contains_key_internal(py, key)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let entries = self.collect_entries(py);
        let mut parts: Vec<String> = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let ks = k.bind(py).repr()?.extract::<String>()?;
            let vs = v.bind(py).repr()?.extract::<String>()?;
            parts.push(format!("{ks} {vs}"));
        }
        Ok(format!("{{{}}}", parts.join(", ")))
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.__repr__(py)
    }

    #[pyo3(signature = (key, /))]
    fn val_at(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        self.val_at_internal(py, key)
    }

    #[pyo3(signature = (key, default, /))]
    fn val_at_default(&self, py: Python<'_>, key: PyObject, default: PyObject) -> PyResult<PyObject> {
        self.val_at_default_internal(py, key, default)
    }

    #[pyo3(signature = (key, val, /))]
    fn assoc(&self, py: Python<'_>, key: PyObject, val: PyObject) -> PyResult<Py<PersistentHashMap>> {
        let new = self.assoc_internal(py, key, val)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (key, /))]
    fn without(&self, py: Python<'_>, key: PyObject) -> PyResult<Py<PersistentHashMap>> {
        let new = self.without_internal(py, key)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (key, /))]
    fn contains_key(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.contains_key_internal(py, key)
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }

    /// `(m k)` / `(m k default)` — map-as-IFn: behaves like lookup.
    #[pyo3(signature = (key, default=None))]
    fn __call__(&self, py: Python<'_>, key: PyObject, default: Option<PyObject>) -> PyResult<PyObject> {
        match default {
            Some(d) => self.val_at_default_internal(py, key, d),
            None => self.val_at_internal(py, key),
        }
    }
}

#[pyclass(module = "clojure._core", name = "PersistentHashMapKeyIter")]
pub struct PersistentHashMapKeyIter {
    keys: Vec<PyObject>,
    pos: usize,
}

#[pymethods]
impl PersistentHashMapKeyIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        if self.pos >= self.keys.len() {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        let item = self.keys[self.pos].clone_ref(py);
        self.pos += 1;
        Ok(item)
    }
}

// --- Python-facing constructor: (hash-map k1 v1 k2 v2 ...). ---

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn hash_map(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<Py<PersistentHashMap>> {
    if args.len() % 2 != 0 {
        return Err(crate::exceptions::IllegalArgumentException::new_err(
            "hash-map requires an even number of arguments",
        ));
    }
    let mut m = PersistentHashMap::new_empty();
    let mut i = 0usize;
    while i < args.len() {
        let k = args.get_item(i)?.unbind();
        let v = args.get_item(i + 1)?.unbind();
        m = m.assoc_internal(py, k, v)?;
        i += 2;
    }
    Py::new(py, m)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentHashMap>()?;
    m.add_class::<PersistentHashMapKeyIter>()?;
    m.add_class::<TransientHashMap>()?;
    m.add_function(wrap_pyfunction!(hash_map, m)?)?;
    Ok(())
}

// --- Protocol impls (Phase 8B). ---

#[implements(Counted)]
impl Counted for PersistentHashMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentHashMap {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::ipersistent_map::cross_map_equiv(py, this.into_any(), other)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentHashMap {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // Vanilla `APersistentMap.hasheq` = `Murmur3.hashUnordered`. Iteration
        // yields MapEntry instances, whose hash matches a length-2 vector
        // hash — so two maps with the same {k v} entries produce the same
        // collection hash regardless of insertion order.
        Ok(crate::murmur3::hash_unordered_seq(py, this.into_any())? as i64)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentHashMap {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, PersistentHashMap {
            count: s.count,
            root: s.root.as_ref().map(std::sync::Arc::clone),
            has_null: s.has_null,
            null_value: s.null_value.as_ref().map(|o| o.clone_ref(py)),
            hash_cache: std::sync::atomic::AtomicI64::new(0),
            meta: m,
        })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentHashMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        // conj on a map takes a MapEntry, a 2-tuple-like [k v], or another
        // map (whose entries are merged).
        if x.is_none(py) {
            return Ok(this.clone_ref(py).into_any());
        }
        let x_b = x.bind(py);
        if let Ok(me) = x_b.cast::<crate::collections::map_entry::MapEntry>() {
            let s = this.bind(py).get();
            let k = me.get().key.clone_ref(py);
            let v = me.get().val.clone_ref(py);
            let new = s.assoc_internal(py, k, v)?;
            return Ok(Py::new(py, new)?.into_any());
        }
        if x_b.cast::<PersistentHashMap>().is_ok()
            || x_b.cast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
        {
            let mut acc: PyObject = this.clone_ref(py).into_any();
            let mut cur = crate::rt::seq(py, x.clone_ref(py))?;
            while !cur.is_none(py) {
                let entry = crate::rt::first(py, cur.clone_ref(py))?;
                acc = crate::rt::conj(py, acc, entry)?;
                cur = crate::rt::next_(py, cur)?;
            }
            return Ok(acc);
        }
        let k = x_b.get_item(0)?.unbind();
        let v = x_b.get_item(1)?.unbind();
        let s = this.bind(py).get();
        let new = s.assoc_internal(py, k, v)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(Py::new(py, PersistentHashMap::new_empty())?.into_any())
    }
}

#[implements(IPersistentMap)]
impl IPersistentMap for PersistentHashMap {
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.assoc_internal(py, k, v)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn without(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.without_internal(py, k)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_key_internal(py, k)
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if !s.contains_key_internal(py, k.clone_ref(py))? {
            return Ok(py.None());
        }
        let v = s.val_at_internal(py, k.clone_ref(py))?;
        let me = crate::collections::map_entry::MapEntry::new(k, v);
        Ok(Py::new(py, me)?.into_any())
    }
}

#[implements(Associative)]
impl Associative for PersistentHashMap {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_key_internal(py, k)
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        <PersistentHashMap as IPersistentMap>::entry_at(this, py, k)
    }
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        <PersistentHashMap as IPersistentMap>::assoc(this, py, k, v)
    }
}

#[implements(ISeqable)]
impl ISeqable for PersistentHashMap {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.count == 0 {
            return Ok(py.None());
        }
        let entries = s.collect_entries(py);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, v) in entries.into_iter().rev() {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            let cons = crate::seqs::cons::Cons::new(me_py, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
}

#[implements(ILookup)]
impl ILookup for PersistentHashMap {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, k, not_found)
    }
}

#[implements(CollReduce)]
impl CollReduce for PersistentHashMap {
    fn coll_reduce1(this: Py<Self>, py: Python<'_>, f: PyObject) -> PyResult<PyObject> {
        let s: &PersistentHashMap = this.bind(py).get();
        let entries = s.collect_entries(py);
        if entries.is_empty() {
            return crate::rt::invoke_n(py, f, &[]);
        }
        let mut it = entries.into_iter();
        let (k0, v0) = it.next().unwrap();
        let me0 = crate::collections::map_entry::MapEntry::new(k0, v0);
        let mut acc: PyObject = Py::new(py, me0)?.into_any();
        for (k, v) in it {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, me_py])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
    fn coll_reduce2(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let s: &PersistentHashMap = this.bind(py).get();
        let mut acc = init;
        for (k, v) in s.collect_entries(py) {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, me_py])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
}

#[implements(IKVReduce)]
impl IKVReduce for PersistentHashMap {
    fn kv_reduce(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let s: &PersistentHashMap = this.bind(py).get();
        let mut acc = init;
        for (k, v) in s.collect_entries(py) {
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, k, v])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
}

#[implements(IFn)]
impl IFn for PersistentHashMap {
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_internal(py, a0)
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, a0, a1)
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        match args.len() {
            1 => Self::invoke1(this, py, args.get_item(0)?.unbind()),
            2 => Self::invoke2(this, py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind()),
            n => Err(crate::exceptions::ArityException::new_err(format!(
                "Wrong number of args ({n}) passed to: PersistentHashMap"
            ))),
        }
    }
}

// ============================================================================
// TransientHashMap (Phase 8C)
// ============================================================================
//
// Mutable-in-place variant of PersistentHashMap. Each transient carries an
// `edit` token (an Arc<AtomicUsize>) shared with every node it has taken
// ownership of. Operations mutate nodes whose edit matches (fast path) and
// clone otherwise.
//
// Safety:
//   - `alive: AtomicBool` guards against use-after-`persistent!`.
//   - Thread ownership is NOT enforced (matches Clojure JVM post-CLJ-1613).
//     Callers handing a transient to another thread are responsible for
//     their own synchronization (typically via `future`'s `@deref`).

#[pyclass(module = "clojure._core", name = "TransientHashMap", frozen)]
pub struct TransientHashMap {
    state: parking_lot::Mutex<TransientHashMapState>,
    alive: AtomicBool,
    edit: Arc<AtomicUsize>,
}

struct TransientHashMapState {
    count: u32,
    root: Option<Arc<MNode>>,
    has_null: bool,
    null_value: Option<PyObject>,
}

impl TransientHashMap {
    fn check_alive_and_owner(&self) -> PyResult<()> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(IllegalStateException::new_err(
                "Transient used after persistent!",
            ));
        }
        Ok(())
    }

    pub(crate) fn from_persistent(py: Python<'_>, m: &PersistentHashMap) -> Self {
        let edit = Arc::new(AtomicUsize::new(1));
        let editable_root = m.root.as_ref().map(|r| r.ensure_editable(&edit));
        Self {
            state: parking_lot::Mutex::new(TransientHashMapState {
                count: m.count,
                root: editable_root,
                has_null: m.has_null,
                null_value: m.null_value.as_ref().map(|o| o.clone_ref(py)),
            }),
            alive: AtomicBool::new(true),
            edit,
        }
    }

    /// Lookup helper for transient: mirrors PersistentHashMap's
    /// `val_at_default_internal` but reads from the transient's live state.
    pub(crate) fn val_at_default_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        self.check_alive_and_owner()?;
        let st = self.state.lock();
        if key.is_none(py) {
            if st.has_null {
                return Ok(st.null_value.as_ref().map(|o| o.clone_ref(py)).unwrap_or(default));
            }
            return Ok(default);
        }
        let Some(root) = st.root.as_ref() else { return Ok(default); };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let root = Arc::clone(root);
        drop(st);
        root.find_or_default(py, 0, h, key, default)
    }

    pub(crate) fn contains_key_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.check_alive_and_owner()?;
        let st = self.state.lock();
        if key.is_none(py) {
            return Ok(st.has_null);
        }
        let Some(root) = st.root.as_ref() else { return Ok(false); };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let root = Arc::clone(root);
        drop(st);
        Ok(root.find(py, 0, h, key)?.is_some())
    }
}

#[pymethods]
impl TransientHashMap {
    fn __len__(&self) -> PyResult<usize> {
        self.check_alive_and_owner()?;
        Ok(self.state.lock().count as usize)
    }

    /// `key in transient-map` — routed through ILookup semantics.
    fn __contains__(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.contains_key_internal(py, key)
    }

    /// `transient-map[key]` — used by `(get t-map k)` fallback and by
    /// `(contains? t-map k)` via ILookup-driven default.
    fn __getitem__(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        let sentinel: PyObject = pyo3::types::PyTuple::empty(py).unbind().into_any();
        let v = self.val_at_default_internal(py, key, sentinel.clone_ref(py))?;
        if crate::rt::identical(py, v.clone_ref(py), sentinel) {
            return Err(pyo3::exceptions::PyKeyError::new_err(()));
        }
        Ok(v)
    }

    fn assoc_bang(slf: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<Py<Self>> {
        {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let mut st = this.state.lock();
            if k.is_none(py) {
                let had = st.has_null;
                st.has_null = true;
                st.null_value = Some(v);
                if !had {
                    st.count += 1;
                }
                drop(st);
                return Ok(slf);
            }
            let h = fold_hash_i64(crate::rt::hash_eq(py, k.clone_ref(py))?);
            let (new_root, added) = match st.root.as_ref() {
                Some(r) => r.assoc_editable(py, &this.edit, 0, h, k, v)?,
                None => {
                    let empty = Arc::new(MNode::Bitmap(
                        crate::collections::phashmap_node::BitmapIndexedNode {
                            inner: parking_lot::Mutex::new(
                                crate::collections::phashmap_node::BitmapIndexedInner {
                                    bitmap: 0,
                                    array: Vec::new(),
                                },
                            ),
                            edit: Some(Arc::clone(&this.edit)),
                        },
                    ));
                    empty.assoc_editable(py, &this.edit, 0, h, k, v)?
                }
            };
            st.root = Some(new_root);
            if added {
                st.count += 1;
            }
            drop(st);
        }
        Ok(slf)
    }

    fn dissoc_bang(slf: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<Py<Self>> {
        {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let mut st = this.state.lock();
            if k.is_none(py) {
                if st.has_null {
                    st.has_null = false;
                    st.null_value = None;
                    st.count -= 1;
                }
                drop(st);
                return Ok(slf);
            }
            let Some(root) = st.root.as_ref().cloned() else {
                drop(st);
                return Ok(slf);
            };
            let h = fold_hash_i64(crate::rt::hash_eq(py, k.clone_ref(py))?);
            let had = root.contains_key(py, 0, h, k.clone_ref(py))?;
            let new_root = root.without_editable(py, &this.edit, 0, h, k)?;
            st.root = new_root;
            if had {
                st.count -= 1;
            }
            drop(st);
        }
        Ok(slf)
    }

    fn conj_bang(slf: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<Py<Self>> {
        // conj on a map transient: x is MapEntry or [k v].
        let (k, v) = {
            let x_b = x.bind(py);
            if let Ok(me) = x_b.cast::<crate::collections::map_entry::MapEntry>() {
                (me.get().key.clone_ref(py), me.get().val.clone_ref(py))
            } else {
                (x_b.get_item(0)?.unbind(), x_b.get_item(1)?.unbind())
            }
        };
        Self::assoc_bang(slf, py, k, v)
    }

    fn persistent_bang(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentHashMap>> {
        let this = slf.bind(py).get();
        this.check_alive_and_owner()?;
        let st = this.state.lock();
        let phm = PersistentHashMap {
            count: st.count,
            root: st.root.as_ref().map(Arc::clone),
            has_null: st.has_null,
            null_value: st.null_value.as_ref().map(|o| o.clone_ref(py)),
            hash_cache: std::sync::atomic::AtomicI64::new(0),
            meta: None,
        };
        drop(st);
        this.alive.store(false, Ordering::Release);
        Py::new(py, phm)
    }
}

// --- Protocol impls ---

#[implements(IEditableCollection)]
impl IEditableCollection for PersistentHashMap {
    fn as_transient(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let t = TransientHashMap::from_persistent(py, s);
        Ok(Py::new(py, t)?.into_any())
    }
}

#[implements(ITransientCollection)]
impl ITransientCollection for TransientHashMap {
    fn conj_bang(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let r = TransientHashMap::conj_bang(this, py, x)?;
        Ok(r.into_any())
    }
    fn persistent_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let r = TransientHashMap::persistent_bang(this, py)?;
        Ok(r.into_any())
    }
}

#[implements(ITransientAssociative)]
impl ITransientAssociative for TransientHashMap {
    fn assoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        let r = TransientHashMap::assoc_bang(this, py, k, v)?;
        Ok(r.into_any())
    }
}

#[implements(ITransientMap)]
impl ITransientMap for TransientHashMap {
    fn dissoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let r = TransientHashMap::dissoc_bang(this, py, k)?;
        Ok(r.into_any())
    }
}

/// Transients are read-through: `get` / `find` / `contains?` on a transient
/// should behave like the same ops on the equivalent persistent map. We
/// implement ILookup + Associative here so protocol dispatch finds them.
#[implements(ILookup)]
impl ILookup for TransientHashMap {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, k, not_found)
    }
}

#[implements(Associative)]
impl Associative for TransientHashMap {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_key_internal(py, k)
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if !s.contains_key_internal(py, k.clone_ref(py))? {
            return Ok(py.None());
        }
        let sentinel: PyObject = pyo3::types::PyTuple::empty(py).unbind().into_any();
        let v = s.val_at_default_internal(py, k.clone_ref(py), sentinel)?;
        let me = crate::collections::map_entry::MapEntry::new(k, v);
        Ok(Py::new(py, me)?.into_any())
    }
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        // `assoc` on a transient is `assoc!` — in-place.
        let r = TransientHashMap::assoc_bang(this, py, k, v)?;
        Ok(r.into_any())
    }
}
