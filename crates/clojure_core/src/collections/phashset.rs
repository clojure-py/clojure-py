//! PersistentHashSet — thin wrapper over PersistentHashMap where each key maps
//! to itself. Port of `clojure/lang/PersistentHashSet.java`.
//!
//! Almost every op delegates to the inner map. The set stores its single
//! canonical element in both "key" and "value" slots so that `get` can return
//! the stored value (useful for interning-style sets where keys compare
//! equal but aren't identical).

use crate::collections::phashmap::{PersistentHashMap, TransientHashMap};
use crate::counted::Counted;
use crate::ieditable_collection::IEditableCollection;
use crate::iequiv::IEquiv;
use crate::ifn::IFn;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_set::IPersistentSet;
use crate::itransient_associative::ITransientAssociative;
use crate::itransient_collection::ITransientCollection;
use crate::itransient_map::ITransientMap;
use crate::itransient_set::ITransientSet;
use clojure_core_macros::implements;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

// ============================================================================
// PersistentHashSet
// ============================================================================

#[pyclass(module = "clojure._core", name = "PersistentHashSet", frozen)]
pub struct PersistentHashSet {
    pub impl_map: Py<PersistentHashMap>,
    pub meta: RwLock<Option<PyObject>>,
}

impl PersistentHashSet {
    pub fn new_empty(py: Python<'_>) -> PyResult<Self> {
        let m = Py::new(py, PersistentHashMap::new_empty())?;
        Ok(Self {
            impl_map: m,
            meta: RwLock::new(None),
        })
    }

    fn clone_meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }

    fn with_map(&self, py: Python<'_>, map: Py<PersistentHashMap>) -> Self {
        Self {
            impl_map: map,
            meta: RwLock::new(self.clone_meta(py)),
        }
    }

    pub fn contains_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        self.impl_map.bind(py).get().contains_key_internal(py, k)
    }

    pub fn get_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let m = self.impl_map.bind(py).get();
        if !m.contains_key_internal(py, k.clone_ref(py))? {
            return Ok(py.None());
        }
        // Return the stored key (== k); inner map stores k -> k.
        m.val_at_internal(py, k)
    }

    pub fn conj_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<Self> {
        // Duplicate → no-op: return same impl_map.
        if self
            .impl_map
            .bind(py)
            .get()
            .contains_key_internal(py, k.clone_ref(py))?
        {
            return Ok(self.with_map(py, self.impl_map.clone_ref(py)));
        }
        let new_map = self
            .impl_map
            .bind(py)
            .get()
            .assoc_internal(py, k.clone_ref(py), k)?;
        let new_map_py = Py::new(py, new_map)?;
        Ok(self.with_map(py, new_map_py))
    }

    pub fn disjoin_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<Self> {
        let new_map = self.impl_map.bind(py).get().without_internal(py, k)?;
        let new_map_py = Py::new(py, new_map)?;
        Ok(self.with_map(py, new_map_py))
    }

    /// Count — delegated to impl_map.
    pub fn count(&self, py: Python<'_>) -> usize {
        self.impl_map.bind(py).get().count as usize
    }
}

#[pymethods]
impl PersistentHashSet {
    fn __len__(&self, py: Python<'_>) -> usize {
        self.count(py)
    }

    fn __bool__(&self, py: Python<'_>) -> bool {
        self.count(py) > 0
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentHashSetIter>> {
        let s = slf.bind(py).get();
        let entries = s.impl_map.bind(py).get().collect_entries(py);
        // Yield values (keys == values).
        let items: Vec<PyObject> = entries.into_iter().map(|(k, _)| k).collect();
        Py::new(py, PersistentHashSetIter { items, pos: 0 })
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
        let entries = self.impl_map.bind(py).get().collect_entries(py);
        let mut parts: Vec<String> = Vec::with_capacity(entries.len());
        for (k, _) in entries {
            parts.push(k.bind(py).repr()?.extract::<String>()?);
        }
        Ok(format!("#{{{}}}", parts.join(" ")))
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.__repr__(py)
    }

    #[pyo3(signature = (k, /))]
    fn conj(&self, py: Python<'_>, k: PyObject) -> PyResult<Py<PersistentHashSet>> {
        let new = self.conj_internal(py, k)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (k, /))]
    fn disjoin(&self, py: Python<'_>, k: PyObject) -> PyResult<Py<PersistentHashSet>> {
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
        self.meta
            .read()
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<PersistentHashSet>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(
            py,
            Self {
                impl_map: self.impl_map.clone_ref(py),
                meta: RwLock::new(m),
            },
        )
    }

    /// `(s k)` — set-as-IFn: returns k if present, else nil.
    #[pyo3(signature = (k, /))]
    fn __call__(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        self.get_internal(py, k)
    }
}

#[pyclass(module = "clojure._core", name = "PersistentHashSetIter")]
pub struct PersistentHashSetIter {
    items: Vec<PyObject>,
    pos: usize,
}

#[pymethods]
impl PersistentHashSetIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        if self.pos >= self.items.len() {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        let item = self.items[self.pos].clone_ref(py);
        self.pos += 1;
        Ok(item)
    }
}

// --- Python-facing constructor: (hash-set v1 v2 ...). ---

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn hash_set(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<Py<PersistentHashSet>> {
    let mut s = PersistentHashSet::new_empty(py)?;
    for i in 0..args.len() {
        let v = args.get_item(i)?.unbind();
        s = s.conj_internal(py, v)?;
    }
    Py::new(py, s)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentHashSet>()?;
    m.add_class::<PersistentHashSetIter>()?;
    m.add_class::<TransientHashSet>()?;
    m.add_function(wrap_pyfunction!(hash_set, m)?)?;
    Ok(())
}

// --- Protocol impls for PersistentHashSet ---

#[implements(Counted)]
impl Counted for PersistentHashSet {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count(py))
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentHashSet {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let other_b = other.bind(py);
        let Ok(other_s) = other_b.downcast::<PersistentHashSet>() else {
            return Ok(false);
        };
        let a = this.bind(py).get();
        let b = other_s.get();
        if a.count(py) != b.count(py) {
            return Ok(false);
        }
        // Every element of a must be contained in b.
        let iter = this.bind(py).try_iter()?;
        for item in iter {
            let x = item?.unbind();
            if !b.contains_internal(py, x)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentHashSet {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // Commutative fold: sum of hash_eq over elements.
        let mut h: i64 = 0;
        let iter = this.bind(py).try_iter()?;
        for item in iter {
            let x = item?.unbind();
            h = h.wrapping_add(crate::rt::hash_eq(py, x)?);
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentHashSet {
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
            PersistentHashSet {
                impl_map: s.impl_map.clone_ref(py),
                meta: RwLock::new(m),
            },
        )?
        .into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentHashSet {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count(py))
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.conj_internal(py, x)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(Py::new(py, PersistentHashSet::new_empty(py)?)?.into_any())
    }
}

#[implements(IPersistentSet)]
impl IPersistentSet for PersistentHashSet {
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

#[implements(IFn)]
impl IFn for PersistentHashSet {
    fn invoke0(_this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err(
            "Wrong number of args (0) passed to: PersistentHashSet",
        ))
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().get_internal(py, a0)
    }
    fn invoke2(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err(
            "Wrong number of args (2) passed to: PersistentHashSet",
        ))
    }
    fn invoke3(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (3) passed to: PersistentHashSet"))
    }
    fn invoke4(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (4) passed to: PersistentHashSet"))
    }
    fn invoke5(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (5) passed to: PersistentHashSet"))
    }
    fn invoke6(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (6) passed to: PersistentHashSet"))
    }
    fn invoke7(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (7) passed to: PersistentHashSet"))
    }
    fn invoke8(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (8) passed to: PersistentHashSet"))
    }
    fn invoke9(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (9) passed to: PersistentHashSet"))
    }
    fn invoke10(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (10) passed to: PersistentHashSet"))
    }
    fn invoke11(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (11) passed to: PersistentHashSet"))
    }
    fn invoke12(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (12) passed to: PersistentHashSet"))
    }
    fn invoke13(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (13) passed to: PersistentHashSet"))
    }
    fn invoke14(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (14) passed to: PersistentHashSet"))
    }
    fn invoke15(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (15) passed to: PersistentHashSet"))
    }
    fn invoke16(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (16) passed to: PersistentHashSet"))
    }
    fn invoke17(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (17) passed to: PersistentHashSet"))
    }
    fn invoke18(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (18) passed to: PersistentHashSet"))
    }
    fn invoke19(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (19) passed to: PersistentHashSet"))
    }
    fn invoke20(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject, _a19: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (20) passed to: PersistentHashSet"))
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        match args.len() {
            1 => Self::invoke1(this, py, args.get_item(0)?.unbind()),
            n => Err(crate::exceptions::ArityException::new_err(format!(
                "Wrong number of args ({n}) passed to: PersistentHashSet"
            ))),
        }
    }
}

// ============================================================================
// TransientHashSet
// ============================================================================
//
// Thin wrapper over TransientHashMap. `conj_bang(x)` stores (x, x) in the
// inner transient map; `disj_bang(x)` dissocs x. The inner transient enforces
// alive/owner-thread checks, so those naturally propagate through here.

#[pyclass(module = "clojure._core", name = "TransientHashSet", frozen)]
pub struct TransientHashSet {
    pub impl_transient: Py<TransientHashMap>,
}

impl TransientHashSet {
    pub fn from_persistent(py: Python<'_>, s: &PersistentHashSet) -> PyResult<Self> {
        let m = s.impl_map.bind(py).get();
        let t = TransientHashMap::from_persistent(py, m);
        Ok(Self {
            impl_transient: Py::new(py, t)?,
        })
    }
}

#[pymethods]
impl TransientHashSet {
    fn __len__(slf: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        // Route through the inner transient's Counted/len path; since Counted
        // isn't implemented for TransientHashMap, we pull the count via the
        // protocol machinery by calling its pymethod via Python.
        let t = slf.bind(py).get().impl_transient.clone_ref(py);
        let len_any = t.bind(py).call_method0("__len__")?;
        len_any.extract::<usize>()
    }

    fn conj_bang(slf: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<Py<Self>> {
        let t = slf.bind(py).get().impl_transient.clone_ref(py);
        <TransientHashMap as ITransientAssociative>::assoc_bang(t, py, x.clone_ref(py), x)?;
        Ok(slf)
    }

    fn disj_bang(slf: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<Py<Self>> {
        let t = slf.bind(py).get().impl_transient.clone_ref(py);
        <TransientHashMap as ITransientMap>::dissoc_bang(t, py, x)?;
        Ok(slf)
    }

    fn persistent_bang(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentHashSet>> {
        let t = slf.bind(py).get().impl_transient.clone_ref(py);
        let new_map_any: PyObject =
            <TransientHashMap as ITransientCollection>::persistent_bang(t, py)?;
        let new_map: Py<PersistentHashMap> = new_map_any
            .bind(py)
            .downcast::<PersistentHashMap>()?
            .clone()
            .unbind();
        let new_set = PersistentHashSet {
            impl_map: new_map,
            meta: RwLock::new(None),
        };
        Py::new(py, new_set)
    }
}

// --- Protocol impls for TransientHashSet / IEditableCollection hook ---

#[implements(IEditableCollection)]
impl IEditableCollection for PersistentHashSet {
    fn as_transient(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let t = TransientHashSet::from_persistent(py, s)?;
        Ok(Py::new(py, t)?.into_any())
    }
}

#[implements(ITransientCollection)]
impl ITransientCollection for TransientHashSet {
    fn conj_bang(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let r = TransientHashSet::conj_bang(this, py, x)?;
        Ok(r.into_any())
    }
    fn persistent_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let r = TransientHashSet::persistent_bang(this, py)?;
        Ok(r.into_any())
    }
}

#[implements(ITransientSet)]
impl ITransientSet for TransientHashSet {
    fn disj_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let r = TransientHashSet::disj_bang(this, py, k)?;
        Ok(r.into_any())
    }
    fn contains_bang(_this: Py<Self>, _py: Python<'_>, _k: PyObject) -> PyResult<bool> {
        Err(crate::exceptions::IllegalStateException::new_err(
            "contains? on a TransientHashSet not yet implemented — call persistent! first",
        ))
    }
    fn get_bang(_this: Py<Self>, _py: Python<'_>, _k: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::IllegalStateException::new_err(
            "get on a TransientHashSet not yet implemented — call persistent! first",
        ))
    }
}
