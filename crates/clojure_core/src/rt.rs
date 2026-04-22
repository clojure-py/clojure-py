//! Runtime helpers — thin wrappers over protocol dispatch.
//!
//! Design rule: rt::* functions must route through protocols, NOT special-case
//! Python types in their bodies. Python-native behavior belongs in the
//! protocol's built-in fallback (installed at module init), not here.

use once_cell::sync::{Lazy, OnceCell};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

// --- Cached references to the protocols we route through. ---

static ILOOKUP_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static IFN_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();

static VAL_AT_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("val_at"));
static INVOKE_VARIADIC_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("invoke_variadic"));

/// Cached arity keys `invoke0`..`invoke20`.
static INVOKE_KEYS: Lazy<Vec<Arc<str>>> = Lazy::new(|| {
    (0..=20usize).map(|n| Arc::from(format!("invoke{n}").as_str())).collect()
});

pub(crate) fn init(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ilookup = m.getattr("ILookup")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = ILOOKUP_PROTO.set(ilookup);
    let ifn = m.getattr("IFn")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IFN_PROTO.set(ifn);
    let _ = py;
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

/// Invoke `target` with `args`, dispatched through IFn using the arity-specific
/// method key (`invoke{N}` for N ≤ 20, `invoke_variadic` otherwise).
///
/// This is the canonical way for Rust code to call a Clojure/Python callable:
/// it goes through the IFn protocol's method cache, so our own IFn-implementing
/// types hit the fast path, and arbitrary Python callables go through the
/// built-in IFn fallback registered at module init.
pub fn invoke_n(py: Python<'_>, target: PyObject, args: &[PyObject]) -> PyResult<PyObject> {
    let proto = IFN_PROTO
        .get()
        .expect("rt::invoke_n called before rt::init — check pymodule init order");
    let method_key: &Arc<str> = if args.len() <= 20 {
        &INVOKE_KEYS[args.len()]
    } else {
        &INVOKE_VARIADIC_KEY
    };
    let args_vec: Vec<PyObject> = args.iter().map(|o| o.clone_ref(py)).collect();
    let args_tup = PyTuple::new(py, &args_vec)?;
    crate::dispatch::dispatch(py, proto, method_key, target, args_tup)
}
