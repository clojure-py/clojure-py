use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientMap", extend_via_metadata = false)]
pub trait ITransientMap: Sized {
    fn dissoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
