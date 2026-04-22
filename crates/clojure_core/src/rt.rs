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
static IEQUIV_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static IHASHEQ_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static ISEQ_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static ISEQABLE_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static COUNTED_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static IPC_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();

static VAL_AT_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("val_at"));
static INVOKE_VARIADIC_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("invoke_variadic"));
static EQUIV_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("equiv"));
static HASH_EQ_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("hash_eq"));
static SEQ_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("seq"));
static FIRST_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("first"));
static NEXT_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("next"));
static MORE_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("more"));
static COUNT_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("count"));
static EMPTY_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("empty"));

/// Cached arity keys `invoke0`..`invoke20`.
static INVOKE_KEYS: Lazy<Vec<Arc<str>>> = Lazy::new(|| {
    (0..=20usize).map(|n| Arc::from(format!("invoke{n}").as_str())).collect()
});

pub(crate) fn init(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ilookup = m.getattr("ILookup")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = ILOOKUP_PROTO.set(ilookup);
    let ifn = m.getattr("IFn")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IFN_PROTO.set(ifn);

    let iequiv = m.getattr("IEquiv")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IEQUIV_PROTO.set(iequiv);

    let ihasheq = m.getattr("IHashEq")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IHASHEQ_PROTO.set(ihasheq);

    let iseq = m.getattr("ISeq")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = ISEQ_PROTO.set(iseq);

    let iseqable = m.getattr("ISeqable")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = ISEQABLE_PROTO.set(iseqable);

    let counted = m.getattr("Counted")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = COUNTED_PROTO.set(counted);

    let ipc = m.getattr("IPersistentCollection")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IPC_PROTO.set(ipc);

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

/// `(= a b)` — dispatches through IEquiv.
pub fn equiv(py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<bool> {
    let proto = IEQUIV_PROTO.get().expect("rt::equiv called before rt::init");
    let args = PyTuple::new(py, &[b])?;
    let result: Py<PyAny> = crate::dispatch::dispatch(py, proto, &EQUIV_KEY, a, args)?;
    result.bind(py).extract::<bool>()
}

/// `(hash x)` — Clojure-style hash, dispatches through IHashEq.
pub fn hash_eq(py: Python<'_>, x: PyObject) -> PyResult<i64> {
    let proto = IHASHEQ_PROTO.get().expect("rt::hash_eq called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    let result: Py<PyAny> = crate::dispatch::dispatch(py, proto, &HASH_EQ_KEY, x, args)?;
    result.bind(py).extract::<i64>()
}

/// `(seq coll)` — returns ISeq or nil; nil-safe.
pub fn seq(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        return Ok(py.None());
    }
    let proto = ISEQABLE_PROTO.get().expect("rt::seq called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &SEQ_KEY, coll, args)
}

/// `(first coll)` — returns first element or nil.
pub fn first(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) {
        return Ok(py.None());
    }
    let proto = ISEQ_PROTO.get().expect("rt::first called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &FIRST_KEY, s, args)
}

/// `(next coll)` — returns ISeq of rest, or nil when empty.
pub fn next_(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) {
        return Ok(py.None());
    }
    let proto = ISEQ_PROTO.get().expect("rt::next called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &NEXT_KEY, s, args)
}

/// `(rest coll)` — returns ISeq of rest, or empty-seq when empty.
pub fn rest(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) {
        // When plist lands we'll return an EmptyList here. For now, nil.
        return Ok(py.None());
    }
    let proto = ISEQ_PROTO.get().expect("rt::rest called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &MORE_KEY, s, args)
}

/// `(count coll)` — nil-safe; dispatches through Counted.
pub fn count(py: Python<'_>, coll: PyObject) -> PyResult<usize> {
    if coll.is_none(py) {
        return Ok(0);
    }
    let proto = COUNTED_PROTO.get().expect("rt::count called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    let result: Py<PyAny> = crate::dispatch::dispatch(py, proto, &COUNT_KEY, coll, args)?;
    result.bind(py).extract::<usize>()
}

/// `(empty coll)` — dispatches through IPersistentCollection.
pub fn empty(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        return Ok(py.None());
    }
    let proto = IPC_PROTO.get().expect("rt::empty called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &EMPTY_KEY, coll, args)
}

// --- Python-exposed wrappers for helpers that aren't already ProtocolMethods. ---
//
// Most of `rt::*` (seq, first, next_, count, empty, equiv, hash_eq, conj, peek, pop)
// is already accessible from Python via the ProtocolMethod bound at `clojure._core.<name>`.
// `rest` is different: there is no IRest protocol (ISeq exposes `more`, not `rest`),
// and `rt::rest` is the nil-safe Clojure-level semantic. Expose it as a pyfunction.

#[pyfunction]
#[pyo3(name = "rest")]
pub fn py_rest(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    rest(py, coll)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_rest, m)?)?;
    Ok(())
}
