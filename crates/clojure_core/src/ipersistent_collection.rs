use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentCollection", extend_via_metadata = false)]
pub trait IPersistentCollection: Sized {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
    fn empty(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    // Equality lives on IEquiv, not duplicated here.
}
