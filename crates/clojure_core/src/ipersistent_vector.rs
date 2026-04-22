use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentVector", extend_via_metadata = false)]
pub trait IPersistentVector: Sized {
    fn length(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
    fn assoc_n(this: Py<Self>, py: Python<'_>, i: PyObject, x: PyObject) -> PyResult<PyObject>;
}
