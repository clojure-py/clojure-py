//! IDeref — the `@x` / `(deref x)` protocol. One method: `deref`.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IDeref", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IDeref: Sized {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
