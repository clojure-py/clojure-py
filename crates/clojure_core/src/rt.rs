//! Runtime helpers — thin wrappers over protocol dispatch.
//!
//! Design rule: rt::* functions must route through protocols, NOT special-case
//! Python types in their bodies. Python-native behavior belongs in the
//! protocol's built-in fallback (installed at module init), not here.

use once_cell::sync::OnceCell;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

// --- Cached references to the protocols we route through. ---

static ILOOKUP_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static VAL_AT_KEY: once_cell::sync::Lazy<Arc<str>> =
    once_cell::sync::Lazy::new(|| Arc::from("val_at"));

pub(crate) fn init(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ilookup = m
        .getattr("ILookup")?
        .downcast::<crate::Protocol>()?
        .clone()
        .unbind();
    let _ = ILOOKUP_PROTO.set(ilookup);
    let _ = py; // keep Python handle parameter for symmetry with other init fns
    Ok(())
}

// --- Helpers. ---

/// `(get coll k default)` — dispatches through ILookup.
pub fn get(py: Python<'_>, coll: PyObject, k: PyObject, default: PyObject) -> PyResult<PyObject> {
    let proto = ILOOKUP_PROTO
        .get()
        .expect("rt::get called before rt::init — check pymodule init order");
    let args = PyTuple::new(py, &[k, default])?;
    crate::dispatch::dispatch(py, proto, &VAL_AT_KEY, coll, args)
}
