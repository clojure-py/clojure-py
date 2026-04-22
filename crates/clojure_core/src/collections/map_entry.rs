//! MapEntry — key/value pair used in map iteration. Equivalent to Clojure's
//! clojure.lang.MapEntry (Java-level), which is a tuple of (key, value) that
//! destructures like a 2-vector.

use pyo3::prelude::*;
use pyo3::types::PyAny;
use parking_lot::RwLock;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "MapEntry", frozen)]
pub struct MapEntry {
    pub key: PyObject,
    pub val: PyObject,
    pub meta: RwLock<Option<PyObject>>,
}

#[pymethods]
impl MapEntry {
    #[new]
    pub fn new(key: PyObject, val: PyObject) -> Self {
        Self {
            key,
            val,
            meta: RwLock::new(None),
        }
    }

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
        let Ok(other_me) = other_b.downcast::<MapEntry>() else {
            return Ok(false);
        };
        let a = slf.bind(py).get();
        let b = other_me.get();
        let keys_eq = crate::rt::equiv(py, a.key.clone_ref(py), b.key.clone_ref(py))?;
        if !keys_eq { return Ok(false); }
        crate::rt::equiv(py, a.val.clone_ref(py), b.val.clone_ref(py))
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        let a = slf.bind(py).get();
        let hk = crate::rt::hash_eq(py, a.key.clone_ref(py))?;
        let hv = crate::rt::hash_eq(py, a.val.clone_ref(py))?;
        Ok(hk.wrapping_mul(31).wrapping_add(hv))
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
