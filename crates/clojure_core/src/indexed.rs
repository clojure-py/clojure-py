use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Indexed", extend_via_metadata = false)]
pub trait Indexed: Sized {
    fn nth(this: Py<Self>, py: Python<'_>, i: usize) -> PyResult<PyObject>;
    fn nth_or_default(this: Py<Self>, py: Python<'_>, i: usize, default: PyObject) -> PyResult<PyObject>;
}
