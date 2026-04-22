use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ISeq", extend_via_metadata = false)]
pub trait ISeq: Sized {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
}
