use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IMeta", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IMeta: Sized {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject>;
}
