//! Runtime helpers — minimal set used by IFn impls.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};

type PyObject = Py<PyAny>;

/// Get an item from a container, returning `default` on miss.
/// Supports PyDict natively; falls back to `__getitem__` for dict-like objects.
pub fn get(py: Python<'_>, coll: PyObject, k: PyObject, default: PyObject) -> PyResult<PyObject> {
    let coll_b = coll.bind(py);
    let k_b = k.bind(py);
    if let Ok(d) = coll_b.downcast::<PyDict>() {
        if let Some(v) = d.get_item(k_b)? {
            return Ok(v.unbind());
        }
        return Ok(default);
    }
    match coll_b.get_item(k_b) {
        Ok(v) => Ok(v.unbind()),
        Err(_) => Ok(default),
    }
}
