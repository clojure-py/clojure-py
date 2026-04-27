//! Python-facing entry points exposed by the `clojure._core` module.
//!
//! These are thin marshaling wrappers — they package Python args into
//! `Value`s, dispatch through the existing `rt::*` helpers, and unwrap
//! the result. Any throwable Value returned by dispatch is re-raised
//! as a Python exception at this boundary.

use clojure_rt::{exception, rt, Value};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// `clojure._core.count(obj)` — call the `Counted/count` protocol on a
/// Python object via the `Counted for PyObject` impl.
#[pyfunction]
pub fn count(obj: &Bound<'_, PyAny>) -> PyResult<i64> {
    let v = Value::pyobject(obj.as_ptr() as *mut _);
    let result = rt::count(v);
    if result.is_exception() {
        let msg = exception::message(result)
            .unwrap_or_else(|| "<no message>".to_string());
        return Err(PyRuntimeError::new_err(msg));
    }
    result.as_int().ok_or_else(|| PyRuntimeError::new_err(
        "Counted/count returned non-integer Value (substrate bug)",
    ))
}
