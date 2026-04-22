use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientCollection", extend_via_metadata = false)]
pub trait ITransientCollection: Sized {
    fn conj_bang(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
    fn persistent_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
