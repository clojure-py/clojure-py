//! `PersistentTreeSet` — sorted set, thin wrapper over `PersistentTreeMap`
//! where each element `x` is stored as the map entry `(x, x)`.
//!
//! Same design as `PersistentHashSet` wrapping `PersistentHashMap`; swaps
//! the inner map impl so the outer gets sorted iteration for free.

use crate::coll_reduce::CollReduce;
use crate::collections::ptreemap::{self, PersistentTreeMap};
use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::ifn::IFn;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_set::IPersistentSet;
use crate::iseqable::ISeqable;
use crate::reversible::Reversible;
use crate::sorted::Sorted;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "PersistentTreeSet", frozen)]
pub struct PersistentTreeSet {
    pub impl_map: Py<PersistentTreeMap>,
    pub meta: Option<PyObject>,
}

impl PersistentTreeSet {
    pub fn new_empty(py: Python<'_>) -> PyResult<Self> {
        let m = Py::new(py, PersistentTreeMap::new_empty())?;
        Ok(Self { impl_map: m, meta: None })
    }

    pub fn new_with_comparator(py: Python<'_>, comparator: Option<PyObject>) -> PyResult<Self> {
        let m = Py::new(py, PersistentTreeMap::new_with_comparator(comparator))?;
        Ok(Self { impl_map: m, meta: None })
    }

    fn with_map(&self, py: Python<'_>, map: Py<PersistentTreeMap>) -> Self {
        Self { impl_map: map, meta: self.meta.as_ref().map(|o| o.clone_ref(py)) }
    }

    pub fn contains_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        self.impl_map.bind(py).get().contains_key_internal(py, k)
    }

    pub fn get_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let m = self.impl_map.bind(py).get();
        if !m.contains_key_internal(py, k.clone_ref(py))? {
            return Ok(py.None());
        }
        m.val_at_internal(py, k)
    }

    pub fn conj_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<Self> {
        if self.impl_map.bind(py).get().contains_key_internal(py, k.clone_ref(py))? {
            return Ok(self.with_map(py, self.impl_map.clone_ref(py)));
        }
        let new_map = self.impl_map.bind(py).get().assoc_internal(py, k.clone_ref(py), k)?;
        let new_map_py = Py::new(py, new_map)?;
        Ok(self.with_map(py, new_map_py))
    }

    pub fn disjoin_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<Self> {
        let new_map = self.impl_map.bind(py).get().without_internal(py, k)?;
        let new_map_py = Py::new(py, new_map)?;
        Ok(self.with_map(py, new_map_py))
    }

    pub fn count(&self, py: Python<'_>) -> usize {
        self.impl_map.bind(py).get().count as usize
    }
}

#[pymethods]
impl PersistentTreeSet {
    fn __len__(&self, py: Python<'_>) -> usize {
        self.count(py)
    }

    fn __bool__(&self, py: Python<'_>) -> bool {
        self.count(py) > 0
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<crate::seqs::cons::ConsIter>> {
        let s = <PersistentTreeSet as ISeqable>::seq(slf, py)?;
        Py::new(py, crate::seqs::cons::ConsIter { current: s })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __contains__(&self, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        self.contains_internal(py, k)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let m = self.impl_map.bind(py).get();
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, true, &mut entries);
        let mut parts = Vec::with_capacity(entries.len());
        for (k, _) in entries {
            parts.push(k.bind(py).repr()?.extract::<String>()?);
        }
        Ok(format!("#{{{}}}", parts.join(" ")))
    }

    #[pyo3(signature = (k, /))]
    fn conj(&self, py: Python<'_>, k: PyObject) -> PyResult<Py<PersistentTreeSet>> {
        let new = self.conj_internal(py, k)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (k, /))]
    fn disjoin(&self, py: Python<'_>, k: PyObject) -> PyResult<Py<PersistentTreeSet>> {
        let new = self.disjoin_internal(py, k)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (k, /))]
    fn contains(&self, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        self.contains_internal(py, k)
    }

    #[pyo3(signature = (k, /))]
    fn get(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        self.get_internal(py, k)
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    #[pyo3(signature = (k, /))]
    fn __call__(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        self.get_internal(py, k)
    }
}

#[implements(Counted)]
impl Counted for PersistentTreeSet {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count(py))
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentTreeSet {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let a = this.bind(py).get();
        let other_b = other.bind(py);
        if let Ok(ots) = other_b.cast::<PersistentTreeSet>() {
            let b = ots.get();
            if a.count(py) != b.count(py) {
                return Ok(false);
            }
            // Walk our elements; ensure each is in other.
            let m = a.impl_map.bind(py).get();
            let mut entries = Vec::new();
            ptreemap::collect_entries(py, &m.root, true, &mut entries);
            for (k, _) in entries {
                if !b.contains_internal(py, k)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        if let Ok(ohs) = other_b.cast::<crate::collections::phashset::PersistentHashSet>() {
            let b = ohs.get();
            if a.count(py) != b.count(py) {
                return Ok(false);
            }
            let m = a.impl_map.bind(py).get();
            let mut entries = Vec::new();
            ptreemap::collect_entries(py, &m.root, true, &mut entries);
            for (k, _) in entries {
                if !b.contains_internal(py, k)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentTreeSet {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // XOR-fold over member hashes (insertion-order-independent).
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, true, &mut entries);
        let mut h: i64 = 0;
        for (k, _) in entries {
            let eh = crate::rt::hash_eq(py, k)?;
            h = h.wrapping_add(eh);
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentTreeSet {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        let new = Self { impl_map: s.impl_map.clone_ref(py), meta: m };
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentTreeSet {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count(py))
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.conj_internal(py, x)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn empty(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        // Preserve comparator.
        let inner_comp = s.impl_map.bind(py).get().comparator.as_ref().map(|c| c.clone_ref(py));
        let new = Self::new_with_comparator(py, inner_comp)?;
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(IPersistentSet)]
impl IPersistentSet for PersistentTreeSet {
    fn disjoin(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.disjoin_internal(py, k)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn contains(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_internal(py, k)
    }
    fn get(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().get_internal(py, k)
    }
}

#[implements(ISeqable)]
impl ISeqable for PersistentTreeSet {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        if m.count == 0 {
            return Ok(py.None());
        }
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, true, &mut entries);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, _) in entries.into_iter().rev() {
            let cons = crate::seqs::cons::Cons::new(k, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
}

#[implements(CollReduce)]
impl CollReduce for PersistentTreeSet {
    fn coll_reduce1(this: Py<Self>, py: Python<'_>, f: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, true, &mut entries);
        if entries.is_empty() {
            return crate::rt::invoke_n(py, f, &[]);
        }
        let mut it = entries.into_iter();
        let (k0, _) = it.next().unwrap();
        let mut acc = k0;
        for (k, _) in it {
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, k])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
    fn coll_reduce2(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, true, &mut entries);
        let mut acc = init;
        for (k, _) in entries {
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, k])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
}

#[implements(IFn)]
impl IFn for PersistentTreeSet {
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().get_internal(py, a0)
    }
}

#[implements(Reversible)]
impl Reversible for PersistentTreeSet {
    fn rseq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        if m.count == 0 {
            return Ok(py.None());
        }
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, false, &mut entries);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, _) in entries.into_iter().rev() {
            let cons = crate::seqs::cons::Cons::new(k, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
}

#[implements(Sorted)]
impl Sorted for PersistentTreeSet {
    fn sorted_seq(this: Py<Self>, py: Python<'_>, ascending: PyObject) -> PyResult<PyObject> {
        let asc = ascending.bind(py).is_truthy()?;
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        if m.count == 0 {
            return Ok(py.None());
        }
        let mut entries = Vec::new();
        ptreemap::collect_entries(py, &m.root, asc, &mut entries);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, _) in entries.into_iter().rev() {
            let cons = crate::seqs::cons::Cons::new(k, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
    fn sorted_seq_from(
        this: Py<Self>,
        py: Python<'_>,
        key: PyObject,
        ascending: PyObject,
    ) -> PyResult<PyObject> {
        let asc = ascending.bind(py).is_truthy()?;
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        if m.count == 0 {
            return Ok(py.None());
        }
        let entries = ptreemap::collect_entries_from(py, m.comparator.as_ref(), &m.root, &key, asc)?;
        if entries.is_empty() {
            return Ok(py.None());
        }
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, _) in entries.into_iter().rev() {
            let cons = crate::seqs::cons::Cons::new(k, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
    fn entry_key(_this: Py<Self>, _py: Python<'_>, entry: PyObject) -> PyResult<PyObject> {
        // For a set, entry == key.
        Ok(entry)
    }
    fn comparator_of(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = s.impl_map.bind(py).get();
        Ok(m.comparator.as_ref().map(|c| c.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
}

// --- Constructors ----------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn sorted_set(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<Py<PersistentTreeSet>> {
    let mut s = PersistentTreeSet::new_empty(py)?;
    for i in 0..args.len() {
        let v = args.get_item(i)?.unbind();
        s = s.conj_internal(py, v)?;
    }
    Py::new(py, s)
}

#[pyfunction]
#[pyo3(signature = (comparator, *args))]
pub fn sorted_set_by(
    py: Python<'_>,
    comparator: PyObject,
    args: Bound<'_, PyTuple>,
) -> PyResult<Py<PersistentTreeSet>> {
    let mut s = PersistentTreeSet::new_with_comparator(py, Some(comparator))?;
    for i in 0..args.len() {
        let v = args.get_item(i)?.unbind();
        s = s.conj_internal(py, v)?;
    }
    Py::new(py, s)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentTreeSet>()?;
    m.add_function(wrap_pyfunction!(sorted_set, m)?)?;
    m.add_function(wrap_pyfunction!(sorted_set_by, m)?)?;
    Ok(())
}
