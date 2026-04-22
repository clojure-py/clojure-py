use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientAssociative", extend_via_metadata = false)]
pub trait ITransientAssociative: Sized {
    fn assoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
}
