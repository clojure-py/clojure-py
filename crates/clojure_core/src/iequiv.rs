use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IEquiv", extend_via_metadata = false)]
pub trait IEquiv: Sized {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}
