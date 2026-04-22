//! Protocols used only by the test suite to exercise machinery not otherwise
//! covered by IFn. Not part of the public-facing API.

use clojure_core_macros::protocol;
use pyo3::prelude::*;

type PyObject = Py<pyo3::types::PyAny>;

#[protocol(name = "clojure.core.test/Greeter", extend_via_metadata = true)]
pub trait Greeter {
    fn greet(&self, py: Python<'_>) -> PyResult<PyObject>;
}
