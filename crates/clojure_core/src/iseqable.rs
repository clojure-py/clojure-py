use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ISeqable", extend_via_metadata = false)]
pub trait ISeqable: Sized {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
