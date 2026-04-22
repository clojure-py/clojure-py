use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Associative", extend_via_metadata = false)]
pub trait Associative: Sized {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
}
