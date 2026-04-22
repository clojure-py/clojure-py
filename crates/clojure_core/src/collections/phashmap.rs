//! PersistentHashMap — 32-way HAMT + separate nil-key slot.
//!
//! Port of clojure/lang/PersistentHashMap.java. This phase (8A) lands the
//! core struct and its pymethods; protocol trait impls follow in Phase 8B
//! and the TransientHashMap variant in 8C.

use crate::associative::Associative;
use crate::collections::phashmap_node::{fold_hash_i64, MNode};
use crate::counted::Counted;
use crate::exceptions::IllegalStateException;
use crate::ieditable_collection::IEditableCollection;
use crate::iequiv::IEquiv;
use crate::ifn::IFn;
use crate::ihasheq::IHashEq;
use crate::ilookup::ILookup;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_map::IPersistentMap;
use crate::itransient_associative::ITransientAssociative;
use crate::itransient_collection::ITransientCollection;
use crate::itransient_map::ITransientMap;
use clojure_core_macros::implements;
use parking_lot::RwLock;
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
    pub null_value: RwLock<Option<PyObject>>,
    /// 0 = uncomputed; else folded hash + 1.
    pub hash_cache: AtomicI64,
    pub meta: RwLock<Option<PyObject>>,
}

impl PersistentHashMap {
    pub fn new_empty() -> Self {
        Self {
            count: 0,
            root: None,
            has_null: false,
            null_value: RwLock::new(None),
            hash_cache: AtomicI64::new(0),
            meta: RwLock::new(None),
        }
    }

    fn clone_meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }

    fn clone_null(&self, py: Python<'_>) -> Option<PyObject> {
        self.null_value.read().as_ref().map(|o| o.clone_ref(py))
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
                null_value: RwLock::new(Some(val)),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
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
            null_value: RwLock::new(self.clone_null(py)),
            hash_cache: AtomicI64::new(0),
            meta: RwLock::new(self.clone_meta(py)),
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
                    null_value: RwLock::new(None),
                    hash_cache: AtomicI64::new(0),
                    meta: RwLock::new(self.clone_meta(py)),
                });
            }
            return Ok(Self {
                count: self.count - 1,
                root: self.root.as_ref().map(Arc::clone),
                has_null: false,
                null_value: RwLock::new(None),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        }
        let Some(root) = &self.root else {
            return Ok(Self {
                count: self.count,
                root: None,
                has_null: self.has_null,
                null_value: RwLock::new(self.clone_null(py)),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let (new_root_opt, removed) = root.without(py, 0, h, key)?;
        if !removed {
            return Ok(Self {
                count: self.count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: self.has_null,
                null_value: RwLock::new(self.clone_null(py)),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        }
        Ok(Self {
            count: self.count - 1,
            root: new_root_opt,
            has_null: self.has_null,
            null_value: RwLock::new(self.clone_null(py)),
            hash_cache: AtomicI64::new(0),
            meta: RwLock::new(self.clone_meta(py)),
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
            .read()
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<PersistentHashMap>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(
            py,
            Self {
                count: self.count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: self.has_null,
                null_value: RwLock::new(self.clone_null(py)),
                hash_cache: AtomicI64::new(self.hash_cache.load(Ordering::Relaxed)),
                meta: RwLock::new(m),
            },
        )
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
        let other_b = other.bind(py);
        let Ok(other_m) = other_b.downcast::<PersistentHashMap>() else {
            return Ok(false);
        };
        let a = this.bind(py).get();
        let b = other_m.get();
        if a.count != b.count { return Ok(false); }
        // For every key in a, look up in b and check equiv.
        // Since we don't have a key-iterator exposed to Rust yet (it's behind __iter__),
        // we iterate via the internal node structure. Simpler: use __iter__ via Python.
        let iter = this.bind(py).try_iter()?;
        for item in iter {
            let k = item?.unbind();
            let av = a.val_at_default_internal(py, k.clone_ref(py), py.None())?;
            let bv = b.val_at_default_internal(py, k, py.None())?;
            if !crate::rt::equiv(py, av, bv)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentHashMap {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        let s = this.bind(py).get();
        // Commutative fold over (hash_eq(k) XOR hash_eq(v)) so insertion order doesn't matter.
        let mut h: i64 = 0;
        let iter = this.bind(py).try_iter()?;
        for item in iter {
            let k = item?.unbind();
            let v = s.val_at_default_internal(py, k.clone_ref(py), py.None())?;
            let kh = crate::rt::hash_eq(py, k)?;
            let vh = crate::rt::hash_eq(py, v)?;
            h = h.wrapping_add(kh ^ vh);
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentHashMap {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, PersistentHashMap {
            count: s.count,
            root: s.root.as_ref().map(std::sync::Arc::clone),
            has_null: s.has_null,
            null_value: parking_lot::RwLock::new(s.null_value.read().as_ref().map(|o| o.clone_ref(py))),
            hash_cache: std::sync::atomic::AtomicI64::new(0),
            meta: parking_lot::RwLock::new(m),
        })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentHashMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        // conj on a map takes a MapEntry or a 2-tuple-like [k v] and assocs.
        let x_b = x.bind(py);
        // Try MapEntry first.
        if let Ok(me) = x_b.downcast::<crate::collections::map_entry::MapEntry>() {
            let s = this.bind(py).get();
            let k = me.get().key.clone_ref(py);
            let v = me.get().val.clone_ref(py);
            let new = s.assoc_internal(py, k, v)?;
            return Ok(Py::new(py, new)?.into_any());
        }
        // Fallback: assume x is sequential with 2 elements (key, val).
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

#[implements(ILookup)]
impl ILookup for PersistentHashMap {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, k, not_found)
    }
}

#[implements(IFn)]
impl IFn for PersistentHashMap {
    fn invoke0(_this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (0) passed to: PersistentHashMap"))
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_internal(py, a0)
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, a0, a1)
    }
    fn invoke3(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (3) passed to: PersistentHashMap"))
    }
    fn invoke4(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (4) passed to: PersistentHashMap"))
    }
    fn invoke5(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (5) passed to: PersistentHashMap"))
    }
    fn invoke6(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (6) passed to: PersistentHashMap"))
    }
    fn invoke7(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (7) passed to: PersistentHashMap"))
    }
    fn invoke8(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (8) passed to: PersistentHashMap"))
    }
    fn invoke9(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (9) passed to: PersistentHashMap"))
    }
    fn invoke10(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (10) passed to: PersistentHashMap"))
    }
    fn invoke11(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (11) passed to: PersistentHashMap"))
    }
    fn invoke12(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (12) passed to: PersistentHashMap"))
    }
    fn invoke13(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (13) passed to: PersistentHashMap"))
    }
    fn invoke14(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (14) passed to: PersistentHashMap"))
    }
    fn invoke15(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (15) passed to: PersistentHashMap"))
    }
    fn invoke16(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (16) passed to: PersistentHashMap"))
    }
    fn invoke17(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (17) passed to: PersistentHashMap"))
    }
    fn invoke18(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (18) passed to: PersistentHashMap"))
    }
    fn invoke19(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (19) passed to: PersistentHashMap"))
    }
    fn invoke20(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject, _a19: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (20) passed to: PersistentHashMap"))
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
//   - `owner_thread` pins the creating thread. Cross-thread use raises
//     IllegalStateException.

/// Hash-based owner-thread identity. Reuses the approach from TransientVector.
fn current_thread_id() -> usize {
    use std::hash::{Hash, Hasher};
    let tid = std::thread::current().id();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tid.hash(&mut h);
    h.finish() as usize
}

#[pyclass(module = "clojure._core", name = "TransientHashMap", frozen)]
pub struct TransientHashMap {
    state: parking_lot::Mutex<TransientHashMapState>,
    alive: AtomicBool,
    owner_thread: AtomicUsize,
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
        let owner = self.owner_thread.load(Ordering::Acquire);
        if owner != current_thread_id() {
            return Err(IllegalStateException::new_err(
                "Transient used by non-owner thread",
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
                null_value: m.null_value.read().as_ref().map(|o| o.clone_ref(py)),
            }),
            alive: AtomicBool::new(true),
            owner_thread: AtomicUsize::new(current_thread_id()),
            edit,
        }
    }
}

#[pymethods]
impl TransientHashMap {
    fn __len__(&self) -> PyResult<usize> {
        self.check_alive_and_owner()?;
        Ok(self.state.lock().count as usize)
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
            if let Ok(me) = x_b.downcast::<crate::collections::map_entry::MapEntry>() {
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
            null_value: parking_lot::RwLock::new(
                st.null_value.as_ref().map(|o| o.clone_ref(py)),
            ),
            hash_cache: std::sync::atomic::AtomicI64::new(0),
            meta: parking_lot::RwLock::new(None),
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
