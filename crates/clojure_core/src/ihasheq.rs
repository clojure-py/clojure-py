use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IHashEq", extend_via_metadata = false)]
pub trait IHashEq: Sized {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64>;
}
