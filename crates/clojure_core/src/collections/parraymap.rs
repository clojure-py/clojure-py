//! PersistentArrayMap — flat-array small map (<=8 entries) with linear scan.
//!
//! Port of `clojure/lang/PersistentArrayMap.java`. Holds entries in a shared
//! `Arc<[(k,v)]>`; every op that would push past the threshold promotes to a
//! `PersistentHashMap`. Linear scans use `rt::equiv` so keys compare by
//! Clojure equality semantics.
//!
//! TransientArrayMap is the editable counterpart; `assoc_bang` past the
//! threshold promotes itself to a `TransientHashMap` and returns it — so
//! callers must re-bind (`t = assoc_bang(t, k, v)`).

use crate::associative::Associative;
use crate::collections::phashmap::{PersistentHashMap, TransientHashMap};
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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Max entries in an array map before promotion to PersistentHashMap.
pub const HASHMAP_THRESHOLD: usize = 8;

// ============================================================================
// PersistentArrayMap
// ============================================================================

#[pyclass(module = "clojure._core", name = "PersistentArrayMap", frozen)]
pub struct PersistentArrayMap {
    pub entries: Arc<[(PyObject, PyObject)]>,
    pub meta: RwLock<Option<PyObject>>,
}

impl PersistentArrayMap {
    pub fn new_empty() -> Self {
        Self {
            entries: Arc::from(Vec::<(PyObject, PyObject)>::new().into_boxed_slice()),
            meta: RwLock::new(None),
        }
    }

    fn clone_meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }

    fn clone_entries(&self, py: Python<'_>) -> Vec<(PyObject, PyObject)> {
        self.entries
            .iter()
            .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
            .collect()
    }

    /// Linear scan; returns the index matching `key` via `rt::equiv`, or None.
    fn index_of(&self, py: Python<'_>, key: &PyObject) -> PyResult<Option<usize>> {
        for (i, (k, _)) in self.entries.iter().enumerate() {
            if crate::rt::equiv(py, k.clone_ref(py), key.clone_ref(py))? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    pub fn val_at_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        match self.index_of(py, &key)? {
            Some(i) => Ok(self.entries[i].1.clone_ref(py)),
            None => Ok(py.None()),
        }
    }

    pub fn val_at_default_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        match self.index_of(py, &key)? {
            Some(i) => Ok(self.entries[i].1.clone_ref(py)),
            None => Ok(default),
        }
    }

    pub fn contains_key_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        Ok(self.index_of(py, &key)?.is_some())
    }

    /// Return a fresh PersistentArrayMap with the same meta but new entries.
    fn with_entries(&self, py: Python<'_>, entries: Vec<(PyObject, PyObject)>) -> Self {
        Self {
            entries: Arc::from(entries.into_boxed_slice()),
            meta: RwLock::new(self.clone_meta(py)),
        }
    }

    /// Build a PersistentHashMap from these entries plus (k, v).
    fn promote_with_pair(
        &self,
        py: Python<'_>,
        k: PyObject,
        v: PyObject,
    ) -> PyResult<Py<PersistentHashMap>> {
        // Transiently build over all existing entries, then add the new pair.
        let empty = Py::new(py, PersistentHashMap::new_empty())?;
        let t = TransientHashMap::from_persistent(py, empty.bind(py).get());
        let t_py = Py::new(py, t)?;
        for (ek, ev) in self.entries.iter() {
            let _ = TransientHashMap::assoc_bang(
                t_py.clone_ref(py),
                py,
                ek.clone_ref(py),
                ev.clone_ref(py),
            )?;
        }
        let _ = TransientHashMap::assoc_bang(t_py.clone_ref(py), py, k, v)?;
        // Call the inherent (pymethods) persistent_bang that returns
        // `Py<PersistentHashMap>` specifically.
        let result_any: PyObject =
            <TransientHashMap as ITransientCollection>::persistent_bang(t_py, py)?;
        Ok(result_any.bind(py).downcast::<PersistentHashMap>()?.clone().unbind())
    }

    /// `assoc` — may return PersistentArrayMap or a promoted PersistentHashMap.
    pub fn assoc_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        val: PyObject,
    ) -> PyResult<PyObject> {
        if let Some(i) = self.index_of(py, &key)? {
            // Replace in-place in a cloned array.
            let mut entries = self.clone_entries(py);
            entries[i] = (key, val);
            let new = self.with_entries(py, entries);
            return Ok(Py::new(py, new)?.into_any());
        }
        if self.entries.len() < HASHMAP_THRESHOLD {
            let mut entries = self.clone_entries(py);
            entries.push((key, val));
            let new = self.with_entries(py, entries);
            return Ok(Py::new(py, new)?.into_any());
        }
        // Promote.
        let phm = self.promote_with_pair(py, key, val)?;
        Ok(phm.into_any())
    }

    pub fn without_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<Self> {
        match self.index_of(py, &key)? {
            None => Ok(self.with_entries(py, self.clone_entries(py))),
            Some(i) => {
                let mut entries = self.clone_entries(py);
                entries.remove(i);
                Ok(self.with_entries(py, entries))
            }
        }
    }

    pub fn collect_entries(&self, py: Python<'_>) -> Vec<(PyObject, PyObject)> {
        self.clone_entries(py)
    }
}

#[pymethods]
impl PersistentArrayMap {
    fn __len__(&self) -> usize {
        self.entries.len()
    }

    fn __bool__(&self) -> bool {
        !self.entries.is_empty()
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentArrayMapKeyIter>> {
        let s = slf.bind(py).get();
        let keys: Vec<PyObject> = s.entries.iter().map(|(k, _)| k.clone_ref(py)).collect();
        Py::new(py, PersistentArrayMapKeyIter { keys, pos: 0 })
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
        let mut parts: Vec<String> = Vec::with_capacity(self.entries.len());
        for (k, v) in self.entries.iter() {
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
    fn val_at_default(
        &self,
        py: Python<'_>,
        key: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        self.val_at_default_internal(py, key, default)
    }

    #[pyo3(signature = (key, val, /))]
    fn assoc(&self, py: Python<'_>, key: PyObject, val: PyObject) -> PyResult<PyObject> {
        self.assoc_internal(py, key, val)
    }

    #[pyo3(signature = (key, /))]
    fn without(&self, py: Python<'_>, key: PyObject) -> PyResult<Py<PersistentArrayMap>> {
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

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<PersistentArrayMap>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(
            py,
            Self {
                entries: Arc::clone(&self.entries),
                meta: RwLock::new(m),
            },
        )
    }

    /// `(m k)` / `(m k default)` — map-as-IFn: behaves like lookup.
    #[pyo3(signature = (key, default=None))]
    fn __call__(
        &self,
        py: Python<'_>,
        key: PyObject,
        default: Option<PyObject>,
    ) -> PyResult<PyObject> {
        match default {
            Some(d) => self.val_at_default_internal(py, key, d),
            None => self.val_at_internal(py, key),
        }
    }
}

#[pyclass(module = "clojure._core", name = "PersistentArrayMapKeyIter")]
pub struct PersistentArrayMapKeyIter {
    keys: Vec<PyObject>,
    pos: usize,
}

#[pymethods]
impl PersistentArrayMapKeyIter {
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

// --- Python-facing constructor: (array-map k1 v1 k2 v2 ...). ---

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn array_map(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
    if args.len() % 2 != 0 {
        return Err(crate::exceptions::IllegalArgumentException::new_err(
            "array-map requires an even number of arguments",
        ));
    }
    // If the user passes > 2*HASHMAP_THRESHOLD args, defer to hash_map.
    // Otherwise, build up via assoc_internal (which promotes as needed).
    let mut m = Py::new(py, PersistentArrayMap::new_empty())?.into_any();
    let mut i = 0usize;
    while i < args.len() {
        let k = args.get_item(i)?.unbind();
        let v = args.get_item(i + 1)?.unbind();
        // Dispatch on current type — after promotion m becomes PersistentHashMap.
        let m_bind = m.bind(py);
        if let Ok(am) = m_bind.downcast::<PersistentArrayMap>() {
            m = am.get().assoc_internal(py, k, v)?;
        } else if let Ok(hm) = m_bind.downcast::<PersistentHashMap>() {
            let new = hm.get().assoc_internal(py, k, v)?;
            m = Py::new(py, new)?.into_any();
        } else {
            return Err(crate::exceptions::IllegalStateException::new_err(
                "array-map builder: unexpected intermediate type",
            ));
        }
        i += 2;
    }
    Ok(m)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentArrayMap>()?;
    m.add_class::<PersistentArrayMapKeyIter>()?;
    m.add_class::<TransientArrayMap>()?;
    m.add_function(wrap_pyfunction!(array_map, m)?)?;
    Ok(())
}

// --- Protocol impls ---

#[implements(Counted)]
impl Counted for PersistentArrayMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().entries.len())
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentArrayMap {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let other_b = other.bind(py);
        // Array-vs-array fast-ish path.
        if let Ok(other_m) = other_b.downcast::<PersistentArrayMap>() {
            let a = this.bind(py).get();
            let b = other_m.get();
            if a.entries.len() != b.entries.len() {
                return Ok(false);
            }
            for (k, v) in a.entries.iter() {
                match b.index_of(py, k)? {
                    None => return Ok(false),
                    Some(j) => {
                        if !crate::rt::equiv(
                            py,
                            v.clone_ref(py),
                            b.entries[j].1.clone_ref(py),
                        )? {
                            return Ok(false);
                        }
                    }
                }
            }
            return Ok(true);
        }
        // Compare with a PersistentHashMap.
        if let Ok(other_m) = other_b.downcast::<PersistentHashMap>() {
            let a = this.bind(py).get();
            let b = other_m.get();
            if (a.entries.len() as u32) != b.count {
                return Ok(false);
            }
            for (k, v) in a.entries.iter() {
                if !b.contains_key_internal(py, k.clone_ref(py))? {
                    return Ok(false);
                }
                let bv = b.val_at_internal(py, k.clone_ref(py))?;
                if !crate::rt::equiv(py, v.clone_ref(py), bv)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentArrayMap {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        let s = this.bind(py).get();
        // Commutative fold: XOR of (hash(k) XOR hash(v)), summed.
        let mut h: i64 = 0;
        for (k, v) in s.entries.iter() {
            let kh = crate::rt::hash_eq(py, k.clone_ref(py))?;
            let vh = crate::rt::hash_eq(py, v.clone_ref(py))?;
            h = h.wrapping_add(kh ^ vh);
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentArrayMap {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta
            .read()
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(
            py,
            PersistentArrayMap {
                entries: Arc::clone(&s.entries),
                meta: RwLock::new(m),
            },
        )?
        .into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentArrayMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().entries.len())
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let x_b = x.bind(py);
        if let Ok(me) = x_b.downcast::<crate::collections::map_entry::MapEntry>() {
            let k = me.get().key.clone_ref(py);
            let v = me.get().val.clone_ref(py);
            return s.assoc_internal(py, k, v);
        }
        let k = x_b.get_item(0)?.unbind();
        let v = x_b.get_item(1)?.unbind();
        s.assoc_internal(py, k, v)
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(Py::new(py, PersistentArrayMap::new_empty())?.into_any())
    }
}

#[implements(IPersistentMap)]
impl IPersistentMap for PersistentArrayMap {
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().assoc_internal(py, k, v)
    }
    fn without(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let new = this.bind(py).get().without_internal(py, k)?;
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
impl Associative for PersistentArrayMap {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_key_internal(py, k)
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        <PersistentArrayMap as IPersistentMap>::entry_at(this, py, k)
    }
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        <PersistentArrayMap as IPersistentMap>::assoc(this, py, k, v)
    }
}

#[implements(ILookup)]
impl ILookup for PersistentArrayMap {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, k, not_found)
    }
}

#[implements(IFn)]
impl IFn for PersistentArrayMap {
    fn invoke0(_this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err(
            "Wrong number of args (0) passed to: PersistentArrayMap",
        ))
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_internal(py, a0)
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, a0, a1)
    }
    fn invoke3(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (3) passed to: PersistentArrayMap"))
    }
    fn invoke4(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (4) passed to: PersistentArrayMap"))
    }
    fn invoke5(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (5) passed to: PersistentArrayMap"))
    }
    fn invoke6(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (6) passed to: PersistentArrayMap"))
    }
    fn invoke7(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (7) passed to: PersistentArrayMap"))
    }
    fn invoke8(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (8) passed to: PersistentArrayMap"))
    }
    fn invoke9(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (9) passed to: PersistentArrayMap"))
    }
    fn invoke10(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (10) passed to: PersistentArrayMap"))
    }
    fn invoke11(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (11) passed to: PersistentArrayMap"))
    }
    fn invoke12(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (12) passed to: PersistentArrayMap"))
    }
    fn invoke13(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (13) passed to: PersistentArrayMap"))
    }
    fn invoke14(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (14) passed to: PersistentArrayMap"))
    }
    fn invoke15(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (15) passed to: PersistentArrayMap"))
    }
    fn invoke16(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (16) passed to: PersistentArrayMap"))
    }
    fn invoke17(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (17) passed to: PersistentArrayMap"))
    }
    fn invoke18(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (18) passed to: PersistentArrayMap"))
    }
    fn invoke19(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (19) passed to: PersistentArrayMap"))
    }
    fn invoke20(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject, _a19: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (20) passed to: PersistentArrayMap"))
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        match args.len() {
            1 => Self::invoke1(this, py, args.get_item(0)?.unbind()),
            2 => Self::invoke2(this, py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind()),
            n => Err(crate::exceptions::ArityException::new_err(format!(
                "Wrong number of args ({n}) passed to: PersistentArrayMap"
            ))),
        }
    }
}

// ============================================================================
// TransientArrayMap
// ============================================================================
//
// Mutable-in-place variant. State is a Vec<(k,v)>. `assoc_bang` past
// HASHMAP_THRESHOLD returns a different pyclass (TransientHashMap) — the
// caller re-binds the name.

fn current_thread_id() -> usize {
    use std::hash::{Hash, Hasher};
    let tid = std::thread::current().id();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tid.hash(&mut h);
    h.finish() as usize
}

#[pyclass(module = "clojure._core", name = "TransientArrayMap", frozen)]
pub struct TransientArrayMap {
    state: parking_lot::Mutex<Vec<(PyObject, PyObject)>>,
    alive: AtomicBool,
    owner_thread: AtomicUsize,
}

impl TransientArrayMap {
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

    fn from_persistent(py: Python<'_>, m: &PersistentArrayMap) -> Self {
        let entries: Vec<(PyObject, PyObject)> = m
            .entries
            .iter()
            .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
            .collect();
        Self {
            state: parking_lot::Mutex::new(entries),
            alive: AtomicBool::new(true),
            owner_thread: AtomicUsize::new(current_thread_id()),
        }
    }

    fn find_index(
        py: Python<'_>,
        entries: &[(PyObject, PyObject)],
        key: &PyObject,
    ) -> PyResult<Option<usize>> {
        for (i, (k, _)) in entries.iter().enumerate() {
            if crate::rt::equiv(py, k.clone_ref(py), key.clone_ref(py))? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    /// Promote `self` → `TransientHashMap`, marking self dead. Returns the
    /// new transient (owns the entries).
    fn promote(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<TransientHashMap>> {
        let this = slf.bind(py).get();
        this.check_alive_and_owner()?;
        let entries = {
            let mut st = this.state.lock();
            std::mem::take(&mut *st)
        };
        // Mark dead so further uses of the old handle error out.
        this.alive.store(false, Ordering::Release);

        let empty = Py::new(py, PersistentHashMap::new_empty())?;
        let thm = TransientHashMap::from_persistent(py, empty.bind(py).get());
        let thm_py = Py::new(py, thm)?;
        for (k, v) in entries {
            TransientHashMap::assoc_bang(thm_py.clone_ref(py), py, k, v)?;
        }
        Ok(thm_py)
    }
}

#[pymethods]
impl TransientArrayMap {
    fn __len__(&self) -> PyResult<usize> {
        self.check_alive_and_owner()?;
        Ok(self.state.lock().len())
    }

    /// Returns `Py<TransientArrayMap>` when still small, or a promoted
    /// `Py<TransientHashMap>` once we cross `HASHMAP_THRESHOLD`.
    fn assoc_bang(
        slf: Py<Self>,
        py: Python<'_>,
        k: PyObject,
        v: PyObject,
    ) -> PyResult<PyObject> {
        {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let mut st = this.state.lock();
            if let Some(i) = Self::find_index(py, &st, &k)? {
                st[i] = (k, v);
                drop(st);
                return Ok(slf.into_any());
            }
            if st.len() < HASHMAP_THRESHOLD {
                st.push((k, v));
                drop(st);
                return Ok(slf.into_any());
            }
            // At threshold and appending a new key — need to promote.
            drop(st);
        }
        let thm_py = Self::promote(slf, py)?;
        TransientHashMap::assoc_bang(thm_py.clone_ref(py), py, k, v)?;
        Ok(thm_py.into_any())
    }

    fn dissoc_bang(slf: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<Py<Self>> {
        {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let mut st = this.state.lock();
            if let Some(i) = Self::find_index(py, &st, &k)? {
                st.remove(i);
            }
            drop(st);
        }
        Ok(slf)
    }

    fn conj_bang(slf: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
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

    fn persistent_bang(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentArrayMap>> {
        let this = slf.bind(py).get();
        this.check_alive_and_owner()?;
        let entries = {
            let mut st = this.state.lock();
            std::mem::take(&mut *st)
        };
        this.alive.store(false, Ordering::Release);
        let pam = PersistentArrayMap {
            entries: Arc::from(entries.into_boxed_slice()),
            meta: RwLock::new(None),
        };
        Py::new(py, pam)
    }
}

// --- Protocol impls ---

#[implements(IEditableCollection)]
impl IEditableCollection for PersistentArrayMap {
    fn as_transient(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let t = TransientArrayMap::from_persistent(py, s);
        Ok(Py::new(py, t)?.into_any())
    }
}

#[implements(ITransientCollection)]
impl ITransientCollection for TransientArrayMap {
    fn conj_bang(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        TransientArrayMap::conj_bang(this, py, x)
    }
    fn persistent_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let r = TransientArrayMap::persistent_bang(this, py)?;
        Ok(r.into_any())
    }
}

#[implements(ITransientAssociative)]
impl ITransientAssociative for TransientArrayMap {
    fn assoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        TransientArrayMap::assoc_bang(this, py, k, v)
    }
}

#[implements(ITransientMap)]
impl ITransientMap for TransientArrayMap {
    fn dissoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let r = TransientArrayMap::dissoc_bang(this, py, k)?;
        Ok(r.into_any())
    }
}
