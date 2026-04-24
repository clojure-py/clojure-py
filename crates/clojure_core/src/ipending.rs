//! `IPending` — the `realized?` protocol. Implemented by Future, Promise,
//! Delay, and any other deferred-value type.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPending", extend_via_metadata = false)]
pub trait IPending: Sized {
    fn is_realized(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
