use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Counted", extend_via_metadata = false)]
pub trait Counted: Sized {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
}
