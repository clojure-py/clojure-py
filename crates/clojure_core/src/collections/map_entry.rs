//! MapEntry — key/value pair used in map iteration. Equivalent to Clojure's
//! clojure.lang.MapEntry (Java-level), which is a tuple of (key, value) that
//! destructures like a 2-vector.

use pyo3::prelude::*;
use pyo3::types::PyAny;
use clojure_core_macros::implements;

use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::indexed::Indexed;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "MapEntry", frozen)]
pub struct MapEntry {
    pub key: PyObject,
    pub val: PyObject,
    pub meta: Option<PyObject>,
}

impl MapEntry {
    pub fn new(key: PyObject, val: PyObject) -> Self {
        Self {
            key,
            val,
            meta: None,
        }
    }
}

#[pymethods]
impl MapEntry {
    #[getter]
    fn key(&self, py: Python<'_>) -> PyObject {
        self.key.clone_ref(py)
    }

    #[getter]
    fn val(&self, py: Python<'_>) -> PyObject {
        self.val.clone_ref(py)
    }

    fn __len__(&self) -> usize { 2 }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<MapEntryIter>> {
        Py::new(py, MapEntryIter { entry: slf, pos: 0 })
    }

    fn __getitem__(&self, py: Python<'_>, i: isize) -> PyResult<PyObject> {
        match i {
            0 => Ok(self.key.clone_ref(py)),
            1 => Ok(self.val.clone_ref(py)),
            _ => Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "MapEntry index {i} out of range"
            ))),
        }
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let other_b = other.bind(py);
        let Ok(other_me) = other_b.cast::<MapEntry>() else {
            return Ok(false);
        };
        let a = slf.bind(py).get();
        let b = other_me.get();
        let keys_eq = crate::rt::equiv(py, a.key.clone_ref(py), b.key.clone_ref(py))?;
        if !keys_eq { return Ok(false); }
        crate::rt::equiv(py, a.val.clone_ref(py), b.val.clone_ref(py))
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // Vanilla MapEntry extends APersistentVector, so its hasheq matches
        // a length-2 vector hash: `mixCollHash(31*(31*1 + hk) + hv, 2)`.
        // Required for `(hash {k v}) == (hash (hash-map k v))` regardless of
        // map type — entry hashes must agree with `[k v]`'s vector hash.
        let a = slf.bind(py).get();
        let hk = crate::rt::hash_eq(py, a.key.clone_ref(py))? as i32;
        let hv = crate::rt::hash_eq(py, a.val.clone_ref(py))? as i32;
        let acc = 1i32.wrapping_mul(31).wrapping_add(hk).wrapping_mul(31).wrapping_add(hv);
        Ok(crate::murmur3::mix_coll_hash(acc, 2) as i64)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let k = self.key.bind(py).str()?.extract::<String>()?;
        let v = self.val.bind(py).str()?.extract::<String>()?;
        Ok(format!("[{k} {v}]"))
    }
}

#[pyclass(module = "clojure._core", name = "MapEntryIter")]
pub struct MapEntryIter {
    entry: Py<MapEntry>,
    pos: u32,
}

#[pymethods]
impl MapEntryIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let e = self.entry.bind(py).get();
        let item = match self.pos {
            0 => e.key.clone_ref(py),
            1 => e.val.clone_ref(py),
            _ => return Err(pyo3::exceptions::PyStopIteration::new_err(())),
        };
        self.pos += 1;
        Ok(item)
    }
}

#[implements(Indexed)]
impl Indexed for MapEntry {
    fn nth(this: Py<Self>, py: Python<'_>, i: PyObject) -> PyResult<PyObject> {
        let idx: i64 = i.bind(py).extract().map_err(|_| {
            crate::exceptions::IllegalArgumentException::new_err("index must be integer")
        })?;
        let e = this.bind(py).get();
        match idx {
            0 => Ok(e.key.clone_ref(py)),
            1 => Ok(e.val.clone_ref(py)),
            _ => Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "MapEntry index {idx} out of range"
            ))),
        }
    }
    fn nth_or_default(this: Py<Self>, py: Python<'_>, i: PyObject, default: PyObject) -> PyResult<PyObject> {
        let Ok(idx) = i.bind(py).extract::<i64>() else { return Ok(default); };
        let e = this.bind(py).get();
        match idx {
            0 => Ok(e.key.clone_ref(py)),
            1 => Ok(e.val.clone_ref(py)),
            _ => Ok(default),
        }
    }
}

#[implements(Counted)]
impl Counted for MapEntry {
    fn count(_this: Py<Self>, _py: Python<'_>) -> PyResult<usize> {
        Ok(2)
    }
}

#[implements(Sequential)]
impl Sequential for MapEntry {}

/// MapEntry acts as a 2-element sequential for destructuring and equality
/// with 2-element vectors. `(seq (->MapEntry :a 1))` returns a cons of the
/// two values so `(= [:a 1] (find m :a))` flows through sequential_equiv.
#[implements(ISeqable)]
impl ISeqable for MapEntry {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let empty: PyObject = crate::collections::plist::empty_list(py).into_any();
        let val_cons = crate::seqs::cons::Cons::new(s.val.clone_ref(py), empty);
        let val_cons_py: PyObject = Py::new(py, val_cons)?.into_any();
        let pair = crate::seqs::cons::Cons::new(s.key.clone_ref(py), val_cons_py);
        Ok(Py::new(py, pair)?.into_any())
    }
}

/// Cross-type sequential equality: `(= [:a 1] (->MapEntry :a 1))` is true.
#[implements(IEquiv)]
impl IEquiv for MapEntry {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        // Same-type fast path.
        let other_b = other.bind(py);
        if let Ok(other_me) = other_b.cast::<MapEntry>() {
            let a = this.bind(py).get();
            let b = other_me.get();
            if !crate::rt::equiv(py, a.key.clone_ref(py), b.key.clone_ref(py))? {
                return Ok(false);
            }
            return crate::rt::equiv(py, a.val.clone_ref(py), b.val.clone_ref(py));
        }
        // Any other Sequential with the same two elements.
        if !crate::rt::is_sequential(py, &other) {
            return Ok(false);
        }
        crate::rt::sequential_equiv(py, this.into_any(), other)
    }
}

#[pyfunction]
#[pyo3(name = "map_entry")]
pub fn py_map_entry(key: PyObject, val: PyObject) -> MapEntry {
    MapEntry::new(key, val)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MapEntry>()?;
    m.add_class::<MapEntryIter>()?;
    m.add_function(wrap_pyfunction!(py_map_entry, m)?)?;
    Ok(())
}
