//! Shim for the historical `dispatch::dispatch` entry point.
//!
//! Pre-Phase-4, this module held the generic "look up an impl in a shared
//! name-keyed `MethodTable`, walk the MRO, honor the fallback, call the
//! PyCFunction" pipeline. Phase 4.2 moved dispatch to per-method
//! `ProtocolFn` objects with typed function-pointer tables — the old
//! machinery is gone.
//!
//! A small number of call sites (mostly `clojure.lang.RT/*` intern_fn
//! wrappers in `eval::rt_ns`, plus a few collection internals) still
//! reference `dispatch::dispatch` by name. Rather than migrate each site
//! individually, this shim looks up the matching `ProtocolFn` by
//! `(protocol_name, method_key)` and dispatches through it. Call sites
//! may be migrated to `protocol_fn::dispatch_cached_N` opportunistically
//! for lower per-call overhead.
//!
//! **Not a long-term API.** Will be deleted once the remaining callers
//! are ported.

use crate::exceptions::IllegalArgumentException;
use crate::protocol::Protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Dispatch a protocol method via the ProtocolFn system. Kept as a shim
/// for legacy call sites that pass a `&Py<Protocol>` + method-name Arc.
///
/// Internally: look up the ProtocolFn by (protocol name, method_key),
/// then forward to `ProtocolFn::dispatch_owned` with target as the
/// receiver and the args tuple unpacked into a Vec.
pub fn dispatch(
    py: Python<'_>,
    protocol_py: &Py<Protocol>,
    method_key: &Arc<str>,
    target: PyObject,
    args: pyo3::Bound<'_, PyTuple>,
) -> PyResult<PyObject> {
    let protocol = protocol_py.bind(py).get();
    // Pull the short name off the protocol's Symbol — everything after the
    // "/" if present, matching how #[protocol] keyed the global registry.
    let name_sym = protocol.name.bind(py).get();
    let proto_name: &str = &name_sym.name;

    let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, proto_name, method_key.as_ref()) else {
        return Err(IllegalArgumentException::new_err(format!(
            "No ProtocolFn for {}/{} — #[protocol] must register every method",
            proto_name, method_key
        )));
    };

    // Convert Bound<PyTuple> to Vec<PyObject> for dispatch_owned.
    let rest: Vec<PyObject> = (0..args.len())
        .map(|i| args.get_item(i).map(|b| b.unbind()))
        .collect::<PyResult<_>>()?;

    crate::protocol_fn::ProtocolFn::dispatch_owned(pfn, py, target, rest)
}
