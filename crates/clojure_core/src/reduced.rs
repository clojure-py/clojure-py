//! Reduced — the short-circuit wrapper used by `reduce` / `reduce-kv` /
//! transducers. When a reduction function returns `(reduced x)`, every
//! reducer along the call chain tests for it and bails out with `x`.

use crate::ideref::IDeref;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Reduced", frozen)]
pub struct Reduced {
    pub val: PyObject,
}

impl Reduced {
    pub fn new(val: PyObject) -> Self {
        Self { val }
    }
}

#[pymethods]
impl Reduced {
    #[new]
    pub fn py_new(val: PyObject) -> Self {
        Self::new(val)
    }

    fn deref(&self, py: Python<'_>) -> PyObject {
        self.val.clone_ref(py)
    }

    #[getter]
    fn val(&self, py: Python<'_>) -> PyObject {
        self.val.clone_ref(py)
    }
}

/// True if `x` is a `Reduced`.
pub fn is_reduced(py: Python<'_>, x: &PyObject) -> bool {
    x.bind(py).cast::<Reduced>().is_ok()
}

/// If `x` is a `Reduced`, unwrap it; else return `x` as-is.
pub fn unreduced(py: Python<'_>, x: PyObject) -> PyObject {
    let b = x.bind(py);
    if let Ok(r) = b.cast::<Reduced>() {
        return r.get().val.clone_ref(py);
    }
    x
}

#[implements(IDeref)]
impl IDeref for Reduced {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().val.clone_ref(py))
    }
}

#[pyfunction]
#[pyo3(name = "reduced")]
pub fn py_reduced(val: PyObject) -> Reduced {
    Reduced::new(val)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Reduced>()?;
    m.add_function(wrap_pyfunction!(py_reduced, m)?)?;
    Ok(())
}
