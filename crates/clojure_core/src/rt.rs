//! Runtime helpers — thin wrappers over protocol dispatch.
//!
//! Design rule: rt::* functions must route through protocols, NOT special-case
//! Python types in their bodies. Python-native behavior belongs in the
//! protocol's built-in fallback (installed at module init), not here.

use once_cell::sync::{Lazy, OnceCell};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule, PyTuple};
use std::sync::atomic::{AtomicU64, Ordering};
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
static ASSOC_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static IMETA_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static SEQUENTIAL_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static COMPARABLE_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();

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
static CONJ_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("conj"));
static ASSOC_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("assoc"));
static META_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("meta"));
static WITH_META_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("with_meta"));
static COMPARE_TO_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("compare_to"));

/// Cached arity keys `invoke0`..`invoke20`.
static INVOKE_KEYS: Lazy<Vec<Arc<str>>> = Lazy::new(|| {
    (0..=20usize).map(|n| Arc::from(format!("invoke{n}").as_str())).collect()
});

pub(crate) fn init(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ilookup = m.getattr("ILookup")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = ILOOKUP_PROTO.set(ilookup);
    let ifn = m.getattr("IFn")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = IFN_PROTO.set(ifn);

    let iequiv = m.getattr("IEquiv")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = IEQUIV_PROTO.set(iequiv);

    let ihasheq = m.getattr("IHashEq")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = IHASHEQ_PROTO.set(ihasheq);

    let iseq = m.getattr("ISeq")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = ISEQ_PROTO.set(iseq);

    let iseqable = m.getattr("ISeqable")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = ISEQABLE_PROTO.set(iseqable);

    let counted = m.getattr("Counted")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = COUNTED_PROTO.set(counted);

    let ipc = m.getattr("IPersistentCollection")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = IPC_PROTO.set(ipc);

    let assoc = m.getattr("Associative")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = ASSOC_PROTO.set(assoc);

    let imeta = m.getattr("IMeta")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = IMETA_PROTO.set(imeta);

    let comparable = m.getattr("Comparable")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = COMPARABLE_PROTO.set(comparable);

    let sequential = m.getattr("Sequential")?.cast::<crate::Protocol>()?.clone().unbind();
    let _ = SEQUENTIAL_PROTO.set(sequential);

    let _ = py;
    Ok(())
}

// --- Helpers. ---

/// `(get coll k default)` — dispatches through ILookup's ProtocolFn.
/// `nil` coll returns the default, matching vanilla `(get nil k)` → nil.
pub fn get(py: Python<'_>, coll: PyObject, k: PyObject, default: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        return Ok(default);
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_3(py, &PFN, "ILookup", "val_at", coll, k, default)
}

/// Invoke `target` with `args`, dispatched through IFn using the arity-specific
/// method key (`invoke{N}` for N ≤ 20, `invoke_variadic` otherwise).
///
/// This is the canonical way for Rust code to call a Clojure/Python callable:
/// it goes through the IFn protocol's method cache, so our own IFn-implementing
/// types hit the fast path, and arbitrary Python callables go through the
/// built-in IFn fallback registered at module init.
pub fn invoke_n(py: Python<'_>, target: PyObject, args: &[PyObject]) -> PyResult<PyObject> {
    // Fast path: a ProtocolFn dispatches through its typed per-type table
    // directly — skips IFn protocol cache + double PyTuple allocation.
    if let Ok(pf) = target.bind(py).downcast::<crate::protocol_fn::ProtocolFn>() {
        let pf_py: Py<crate::protocol_fn::ProtocolFn> = pf.clone().unbind();
        if args.is_empty() {
            return Err(crate::exceptions::IllegalArgumentException::new_err(format!(
                "Protocol method {} requires at least one arg (the target)",
                pf_py.bind(py).get().name
            )));
        }
        let target_arg = args[0].clone_ref(py);
        let rest: Vec<PyObject> = args[1..].iter().map(|o| o.clone_ref(py)).collect();
        return crate::protocol_fn::ProtocolFn::dispatch_owned(pf_py, py, target_arg, rest);
    }

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

/// Like `invoke_n`, but takes ownership of `args` so no per-arg `clone_ref`
/// is needed inside the function. Callers that already own a fresh `Vec`
/// should prefer this (e.g. the bytecode VM when draining the value stack).
pub fn invoke_n_owned(
    py: Python<'_>,
    target: PyObject,
    mut args: Vec<PyObject>,
) -> PyResult<PyObject> {
    // Fast path: target is a ProtocolFn — dispatch through its typed
    // table directly, skipping the IFn protocol cache + two PyTuple
    // allocations + the name-keyed method lookup.
    if let Ok(pf) = target.bind(py).downcast::<crate::protocol_fn::ProtocolFn>() {
        let pf_py: Py<crate::protocol_fn::ProtocolFn> = pf.clone().unbind();
        if args.is_empty() {
            return Err(crate::exceptions::IllegalArgumentException::new_err(format!(
                "Protocol method {} requires at least one arg (the target)",
                pf_py.bind(py).get().name
            )));
        }
        let target_arg = args.remove(0);
        return crate::protocol_fn::ProtocolFn::dispatch_owned(pf_py, py, target_arg, args);
    }

    let proto = IFN_PROTO
        .get()
        .expect("rt::invoke_n_owned called before rt::init — check pymodule init order");
    let method_key: &Arc<str> = if args.len() <= 20 {
        &INVOKE_KEYS[args.len()]
    } else {
        &INVOKE_VARIADIC_KEY
    };
    // `PyTuple::new` copies its inputs into the tuple's internal storage
    // (incrementing refcount per slot), so we don't need to pre-clone:
    // the Vec's owned refs are released on drop as expected.
    let args_tup = PyTuple::new(py, &args)?;
    crate::dispatch::dispatch(py, proto, method_key, target, args_tup)
}

/// True iff `x`'s type (or an MRO ancestor) is extended to `Sequential`.
/// Used by IEquiv impls of sequential collections to decide whether the
/// other side of an `=` comparison should be walked pairwise.
pub fn is_sequential(py: Python<'_>, x: &PyObject) -> bool {
    let Some(proto) = SEQUENTIAL_PROTO.get() else { return false; };
    let proto_ref = proto.bind(py).get();
    let b = x.bind(py);
    let ty = b.get_type();
    let exact = crate::protocol::CacheKey::for_py_type(&ty);
    if proto_ref.cache.lookup(exact).is_some() {
        return true;
    }
    let Ok(mro) = ty.getattr("__mro__") else { return false; };
    let Ok(mro_tuple): Result<Bound<'_, PyTuple>, _> = mro.cast_into() else { return false; };
    for parent in mro_tuple.iter().skip(1) {
        let Ok(pt): Result<Bound<'_, pyo3::types::PyType>, _> = parent.cast_into() else { continue; };
        let pk = crate::protocol::CacheKey::for_py_type(&pt);
        if proto_ref.cache.lookup(pk).is_some() {
            return true;
        }
    }
    false
}

/// Element-wise equality by walking both sides via `seq`. Both inputs are
/// assumed sequential (caller checks); walks terminate at the first
/// length-or-element mismatch. Used by every sequential collection's IEquiv
/// impl to honor Clojure's cross-type sequential-equality rule
/// (e.g. `(= [1 2 3] '(1 2 3))` → true).
pub fn sequential_equiv(py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<bool> {
    let mut a_seq = seq(py, a)?;
    let mut b_seq = seq(py, b)?;
    loop {
        let a_nil = a_seq.is_none(py);
        let b_nil = b_seq.is_none(py);
        if a_nil && b_nil { return Ok(true); }
        if a_nil || b_nil { return Ok(false); }
        let ha = first(py, a_seq.clone_ref(py))?;
        let hb = first(py, b_seq.clone_ref(py))?;
        if !equiv(py, ha, hb)? { return Ok(false); }
        a_seq = next_(py, a_seq)?;
        b_seq = next_(py, b_seq)?;
    }
}

/// `(= a b)` — dispatches through IEquiv's ProtocolFn.
pub fn equiv(py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<bool> {
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    let result = crate::protocol_fn::dispatch_cached_2(py, &PFN, "IEquiv", "equiv", a, b)?;
    result.bind(py).extract::<bool>()
}

/// `(hash x)` — Clojure-style hash, dispatches through IHashEq's ProtocolFn.
pub fn hash_eq(py: Python<'_>, x: PyObject) -> PyResult<i64> {
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    let result = crate::protocol_fn::dispatch_cached_1(py, &PFN, "IHashEq", "hash_eq", x)?;
    result.bind(py).extract::<i64>()
}

/// `(seq coll)` — returns ISeq or nil; nil-safe.
pub fn seq(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        return Ok(py.None());
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_1(py, &PFN, "ISeqable", "seq", coll)
}

/// `(first coll)` — returns first element or nil.
pub fn first(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) {
        return Ok(py.None());
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_1(py, &PFN, "ISeq", "first", s)
}

/// `(next coll)` — returns ISeq of rest, or nil when empty.
pub fn next_(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) {
        return Ok(py.None());
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_1(py, &PFN, "ISeq", "next", s)
}

/// `(rest coll)` — returns ISeq of rest, or empty-seq when empty.
pub fn rest(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) {
        return Ok(py.None());
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_1(py, &PFN, "ISeq", "more", s)
}

/// `(count coll)` — nil-safe; dispatches through Counted's ProtocolFn.
pub fn count(py: Python<'_>, coll: PyObject) -> PyResult<usize> {
    if coll.is_none(py) {
        return Ok(0);
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    let result = crate::protocol_fn::dispatch_cached_1(py, &PFN, "Counted", "count", coll)?;
    result.bind(py).extract::<usize>()
}

/// `(empty coll)` — dispatches through IPersistentCollection's ProtocolFn.
pub fn empty(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        return Ok(py.None());
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_1(py, &PFN, "IPersistentCollection", "empty", coll)
}

/// `(conj coll x)` — dispatches through IPersistentCollection's ProtocolFn.
/// Nil-safe: `(conj nil x)` returns `(x)`.
pub fn conj(py: Python<'_>, coll: PyObject, x: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        let tup = PyTuple::new(py, &[x])?;
        return crate::collections::plist::list_(py, tup);
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_2(py, &PFN, "IPersistentCollection", "conj", coll, x)
}

/// `(assoc coll k v)` — dispatches through Associative's ProtocolFn.
/// `nil` coll yields a fresh 1-entry map, matching vanilla
/// `(assoc nil :a 1)` → `{:a 1}`.
pub fn assoc(py: Python<'_>, coll: PyObject, k: PyObject, v: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) {
        let mut m = crate::collections::phashmap::PersistentHashMap::new_empty();
        m = m.assoc_internal(py, k, v)?;
        return Ok(Py::new(py, m)?.into_any());
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_3(py, &PFN, "Associative", "assoc", coll, k, v)
}

/// `(meta x)` — dispatches through IMeta; nil-safe and falls back to nil if
/// the target has no IMeta impl (unlike strict dispatch).
pub fn meta(py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
    if x.is_none(py) {
        return Ok(py.None());
    }
    // Clojure namespaces don't implement IMeta — their meta lives on the
    // `__clj_ns_meta__` dunder attached at create_ns time.
    let b = x.bind(py);
    if crate::namespace::is_clojure_namespace(py, b).unwrap_or(false) {
        return match b.getattr("__clj_ns_meta__") {
            Ok(m) => Ok(m.unbind()),
            Err(_) => Ok(py.None()),
        };
    }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    match crate::protocol_fn::dispatch_cached_1(py, &PFN, "IMeta", "meta", x) {
        Ok(v) => Ok(v),
        Err(e) if e.is_instance_of::<crate::exceptions::IllegalArgumentException>(py) => {
            Ok(py.None())
        }
        Err(e) => Err(e),
    }
}

/// `(with-meta x m)` — dispatches through IMeta's ProtocolFn.
pub fn with_meta(py: Python<'_>, x: PyObject, m: PyObject) -> PyResult<PyObject> {
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_2(py, &PFN, "IMeta", "with_meta", x, m)
}

/// `(identical? a b)` — Python `is` semantics.
pub fn identical(py: Python<'_>, a: PyObject, b: PyObject) -> bool {
    a.bind(py).is(b.bind(py))
}

/// `(compare a b)` — dispatches through Comparable; nil sorts first.
pub fn compare(py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<i64> {
    // Nil handling lives here so user types don't need to special-case it.
    if a.is_none(py) && b.is_none(py) { return Ok(0); }
    if a.is_none(py) { return Ok(-1); }
    if b.is_none(py) { return Ok(1); }
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
    let result = crate::protocol_fn::dispatch_cached_2(py, &PFN, "Comparable", "compare_to", a, b)?;
    result.bind(py).extract::<i64>()
}

/// Monotonic ID counter — backs `gensym` and any other caller needing a
/// fresh integer per process.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// `(cons x coll)` — build a Cons cell. The tail is stored **unrealized**;
/// `seq`/`next`/`rest` on the Cons will call `rt::seq` on the tail lazily.
/// Forcing the tail here would realize LazySeqs eagerly at cons-construction
/// time and break `(cons x (lazy-seq ...))` recursion.
pub fn cons(py: Python<'_>, x: PyObject, coll: PyObject) -> PyResult<PyObject> {
    let c = crate::seqs::cons::Cons::new(x, coll);
    Ok(Py::new(py, c)?.into_any())
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
