//! Bridge: Python `PyErr` ⇒ throwable `Value`.
//!
//! Every PyObject-protocol impl that catches a `PyErr` routes through
//! `pyerr_to_value`, so the message format is uniform across protocols
//! and stable for future Clojure-level `try/catch`.

use clojure_rt::exception;
use clojure_rt::Value;
use pyo3::types::{PyAnyMethods, PyTypeMethods};
use pyo3::{PyErr, Python};

/// Convert a Python exception into a `Foreign` throwable Value. The
/// message includes the Python exception's type name and string form
/// (`<TypeName>: <args>`), giving downstream `try/catch` enough to
/// pattern-match on without re-importing the original `PyErr`.
pub fn pyerr_to_value(py: Python<'_>, err: PyErr) -> Value {
    let type_name = err
        .get_type(py)
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    let detail = err.value(py).str()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "<unrepresentable>".to_string());
    exception::make_foreign(format!("{type_name}: {detail}"))
}
