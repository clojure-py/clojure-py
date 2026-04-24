//! `ProtocolFn` — per-function typed dispatch (Phase 1 scaffold).
//!
//! Design pivot from the current Protocol/MethodTable model: every protocol
//! method becomes a standalone ProtocolFn instance that owns its own
//! `PyType -> InvokeFns` dispatch table. No name-indexed shared map; no
//! PyTuple allocation on the hot path; no Arc::clone on MethodTable.
//!
//! Phase 1 scope: the pyclass exists, dispatches through its table, and
//! integrates with the VM's `Invoke` fast path via `rt::invoke_n_owned`.
//! No existing protocol is migrated yet — macros in Phase 2 will start
//! emitting ProtocolFn instances, then Phase 3 migrates protocols one
//! at a time.

use crate::exceptions::IllegalArgumentException;
use crate::protocol::CacheKey;
use dashmap::DashMap;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple, PyType};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

type PyObject = Py<PyAny>;

/// Global registry: (protocol_name, method_name) -> ProtocolFn instance.
/// Populated by the `#[protocol]` proc-macro at module init. `#[implements]`
/// looks up the right ProtocolFn via this map when registering typed impls.
static PROTOCOL_FN_REGISTRY: once_cell::sync::OnceCell<
    DashMap<(Arc<str>, Arc<str>), Py<ProtocolFn>>,
> = once_cell::sync::OnceCell::new();

fn registry() -> &'static DashMap<(Arc<str>, Arc<str>), Py<ProtocolFn>> {
    PROTOCOL_FN_REGISTRY.get_or_init(DashMap::new)
}

/// Global registry: protocol_name -> old-style Protocol instance. Mirrors
/// PROTOCOL_FN_REGISTRY but keyed by just the protocol name. Populated by
/// `#[protocol]` at init. Consulted by ProtocolFn's dispatch as a fallback
/// when the typed table misses — lets fallback-installed impls (e.g. the
/// `Counted::__len__` path for `str`) keep working after the primary
/// binding is flipped to ProtocolFn.
static PROTOCOL_REGISTRY: once_cell::sync::OnceCell<
    DashMap<Arc<str>, Py<crate::protocol::Protocol>>,
> = once_cell::sync::OnceCell::new();

fn protocol_registry() -> &'static DashMap<Arc<str>, Py<crate::protocol::Protocol>> {
    PROTOCOL_REGISTRY.get_or_init(DashMap::new)
}

/// Register a Protocol under its name so ProtocolFns declared by the same
/// `#[protocol]` invocation can find it on fallback.
pub fn register_old_protocol(name: &str, proto: Py<crate::protocol::Protocol>) {
    protocol_registry().insert(Arc::from(name), proto);
}

fn get_old_protocol(
    py: Python<'_>,
    name: &str,
) -> Option<Py<crate::protocol::Protocol>> {
    protocol_registry()
        .get(&Arc::from(name))
        .map(|e| e.value().clone_ref(py))
}

// -------- Cached-dispatch helpers for rt::* wrappers ----------
//
// Each `rt::*` helper (e.g. `rt::count`, `rt::first`) wraps a specific
// protocol method. Looking up the ProtocolFn in the registry on every
// call would dominate the fast path, so each helper stashes its resolved
// ProtocolFn in a `'static OnceCell` on first hit.
//
// The helpers below accept that OnceCell + the registry key, resolve it
// lazily, and dispatch through ProtocolFn::dispatch_owned.

fn resolve_cached<'a>(
    py: Python<'_>,
    cell: &'a once_cell::sync::OnceCell<Py<ProtocolFn>>,
    proto_name: &'static str,
    method_name: &'static str,
) -> &'a Py<ProtocolFn> {
    cell.get_or_init(|| {
        get_protocol_fn(py, proto_name, method_name).unwrap_or_else(|| {
            panic!(
                "ProtocolFn {}/{} not registered — check #[protocol] macro",
                proto_name, method_name
            )
        })
    })
}

/// Dispatch arity-1 (target-only method, like `count`) through a cached
/// ProtocolFn WITHOUT constructing a Vec<PyObject> for args — the typed
/// fn pointer is called directly.
#[inline]
pub fn dispatch_cached_1(
    py: Python<'_>,
    cell: &once_cell::sync::OnceCell<Py<ProtocolFn>>,
    proto_name: &'static str,
    method_name: &'static str,
    target: PyObject,
) -> PyResult<PyObject> {
    let pfn = resolve_cached(py, cell, proto_name, method_name);
    dispatch_via_pfn(py, pfn, target, Vec::new(), |fns, py, target| {
        fns.invoke0.map(|fp| fp(py, target))
    })
}

/// Dispatch arity-2 (target + 1 arg) through a cached ProtocolFn.
#[inline]
pub fn dispatch_cached_2(
    py: Python<'_>,
    cell: &once_cell::sync::OnceCell<Py<ProtocolFn>>,
    proto_name: &'static str,
    method_name: &'static str,
    target: PyObject,
    a: PyObject,
) -> PyResult<PyObject> {
    let pfn = resolve_cached(py, cell, proto_name, method_name);
    let this = pfn.bind(py).get();
    match this.resolve(py, &target)? {
        Some(fns) => match fns.invoke1 {
            Some(fp) => fp(py, &target, a),
            None => match dispatch_on_fns(py, fns.as_ref(), target, vec![a]) {
                Some(r) => r,
                None => Err(this.raise_no_impl(py, &py.None())),
            },
        },
        None => dispatch_fallback_miss(py, pfn, target, vec![a]),
    }
}

/// Dispatch arity-3 (target + 2 args) through a cached ProtocolFn.
#[inline]
pub fn dispatch_cached_3(
    py: Python<'_>,
    cell: &once_cell::sync::OnceCell<Py<ProtocolFn>>,
    proto_name: &'static str,
    method_name: &'static str,
    target: PyObject,
    a: PyObject,
    b: PyObject,
) -> PyResult<PyObject> {
    let pfn = resolve_cached(py, cell, proto_name, method_name);
    let this = pfn.bind(py).get();
    match this.resolve(py, &target)? {
        Some(fns) => match fns.invoke2 {
            Some(fp) => fp(py, &target, a, b),
            None => match dispatch_on_fns(py, fns.as_ref(), target, vec![a, b]) {
                Some(r) => r,
                None => Err(this.raise_no_impl(py, &py.None())),
            },
        },
        None => dispatch_fallback_miss(py, pfn, target, vec![a, b]),
    }
}

/// Helper for the arity-1 path that takes a closure to avoid moving target
/// into the None arm before we need it.
#[inline]
fn dispatch_via_pfn<F>(
    py: Python<'_>,
    pfn: &Py<ProtocolFn>,
    target: PyObject,
    args_fallback: Vec<PyObject>,
    fast: F,
) -> PyResult<PyObject>
where
    F: FnOnce(&InvokeFns, Python<'_>, &PyObject) -> Option<PyResult<PyObject>>,
{
    let this = pfn.bind(py).get();
    match this.resolve(py, &target)? {
        Some(fns) => match fast(fns.as_ref(), py, &target) {
            Some(r) => r,
            None => match dispatch_on_fns(py, fns.as_ref(), target, args_fallback) {
                Some(r) => r,
                None => Err(this.raise_no_impl(py, &py.None())),
            },
        },
        None => dispatch_fallback_miss(py, pfn, target, args_fallback),
    }
}

fn dispatch_fallback_miss(
    py: Python<'_>,
    pfn: &Py<ProtocolFn>,
    target: PyObject,
    args: Vec<PyObject>,
) -> PyResult<PyObject> {
    // Post-Phase-4: all dispatch is ProtocolFn-native. Miss path is:
    //   1. lazy legacy mirror: if the old Protocol's MethodCache has an
    //      impl for this (type, method) — typically from a Python-side
    //      `Protocol.extend_type` call — install it into this ProtocolFn's
    //      `generic` slot (marked promoted so future legacy re-extensions
    //      invalidate it via epoch) and retry. Direct extensions beat
    //      metadata.
    //   2. extend-via-metadata (if opt-in): look up __clj_meta__[method]
    //      and call with (target, *args) — does NOT populate the cache.
    //   3. protocol-level fallback closure (if set): gets one shot to
    //      populate the typed cache, then we re-resolve and retry.
    //   4. raise "no impl".
    let this = pfn.bind(py).get();

    let old_proto_opt = get_old_protocol(py, &this.protocol_name);

    // Step 1: lazy legacy mirror.
    if let Some(ref old_proto) = old_proto_opt {
        if let Some(result) = try_legacy_mirror(py, this, old_proto, &target, &args)? {
            return result;
        }
    }

    // Step 2: extend-via-metadata.
    if let Some(ref old_proto) = old_proto_opt {
        if old_proto.bind(py).get().via_metadata {
            if let Ok(meta) = target.bind(py).getattr("__clj_meta__") {
                if let Ok(meta_dict) = meta.cast::<pyo3::types::PyDict>() {
                    if let Some(impl_fn) = meta_dict.get_item(this.name.as_str())? {
                        let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
                        call_args.push(target);
                        call_args.extend(args);
                        let tup = pyo3::types::PyTuple::new(py, &call_args)?;
                        return Ok(impl_fn.call1(tup)?.unbind());
                    }
                }
            }
        }
    }

    // Step 3: protocol-level fallback closure.
    let old_proto = match old_proto_opt {
        Some(p) => p,
        None => return Err(this.raise_no_impl(py, &target)),
    };
    let proto_ref = old_proto.bind(py).get();
    let fb = match proto_ref.fallback.read().as_ref() {
        Some(f) => f.clone_ref(py),
        None => return Err(this.raise_no_impl(py, &target)),
    };
    let fb_args = (
        old_proto.clone_ref(py),
        this.name.clone(),
        target.clone_ref(py),
    );
    let _ = fb.bind(py).call1(fb_args)?;

    // Re-resolve — fallback should have populated the typed cache.
    let fns = match this.resolve(py, &target)? {
        Some(f) => f,
        None => {
            // Fallback may have populated only the legacy cache. Try one
            // more lazy legacy mirror before giving up.
            if let Some(result) = try_legacy_mirror(py, this, &old_proto, &target, &args)? {
                return result;
            }
            return Err(this.raise_no_impl(py, &target));
        }
    };
    let target_clone = target.clone_ref(py);
    match dispatch_on_fns(py, &*fns, target, args) {
        Some(r) => r,
        None => Err(this.raise_no_impl(py, &target_clone)),
    }
}

/// Look up an impl in the legacy cache for `target`'s type (exact + MRO).
/// If found, call it directly with (target, *args) — no caching, so that
/// concurrent `Protocol.extend_type` re-extensions are visible on the
/// next dispatch. The typed-fn-pointer fast path stays cached; only this
/// legacy-mirrored fallback runs an uncached lookup per call.
fn try_legacy_mirror(
    py: Python<'_>,
    this: &ProtocolFn,
    old_proto: &Py<crate::protocol::Protocol>,
    target: &PyObject,
    args: &[PyObject],
) -> PyResult<Option<PyResult<PyObject>>> {
    let Some(impl_py) = resolve_legacy_impl(py, old_proto, target, &this.name) else {
        return Ok(None);
    };
    let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
    call_args.push(target.clone_ref(py));
    for a in args {
        call_args.push(a.clone_ref(py));
    }
    let tup = match pyo3::types::PyTuple::new(py, &call_args) {
        Ok(t) => t,
        Err(e) => return Ok(Some(Err(e))),
    };
    let _ = this; // silence unused warning
    Ok(Some(impl_py.bind(py).call1(tup).map(|b| b.unbind())))
}

/// Walk the legacy MethodCache for `(type, method)` — exact type first,
/// then MRO. Returns the first matching impl (a `Py<PyAny>` callable).
fn resolve_legacy_impl(
    py: Python<'_>,
    old_proto: &Py<crate::protocol::Protocol>,
    target: &PyObject,
    method: &str,
) -> Option<PyObject> {
    let proto_ref = old_proto.bind(py).get();
    let ty = target.bind(py).get_type();
    let exact_key = crate::protocol::CacheKey::for_py_type(&ty);
    if let Some(impl_py) = proto_ref.lookup_legacy_impl(py, exact_key, method) {
        return Some(impl_py);
    }
    let mro = ty.getattr("__mro__").ok()?;
    let mro_tuple: pyo3::Bound<'_, PyTuple> = mro.cast_into().ok()?;
    for parent in mro_tuple.iter().skip(1) {
        let parent_ty: pyo3::Bound<'_, PyType> = parent.cast_into().ok()?;
        let pk = crate::protocol::CacheKey::for_py_type(&parent_ty);
        if let Some(impl_py) = proto_ref.lookup_legacy_impl(py, pk, method) {
            return Some(impl_py);
        }
    }
    None
}

/// Pure arity dispatch on a resolved InvokeFns — no cache lookup, no
/// fallback. Returns `Some(result)` if any slot matches (typed arity,
/// variadic, or generic Py<PyAny>); `None` only if nothing fits.
pub(crate) fn dispatch_on_fns(
    py: Python<'_>,
    fns: &InvokeFns,
    target: PyObject,
    mut args: Vec<PyObject>,
) -> Option<PyResult<PyObject>> {
    macro_rules! dispatch_n {
        ($fns_field:ident, $($arg:ident),*) => {
            match fns.$fns_field {
                Some(fp) => {
                    $(let $arg = args.pop().unwrap();)*
                    Some(dispatch_n!(@reverse_call fp, py, target, [], $($arg),*))
                }
                None => match fns.invoke_variadic {
                    Some(fp) => Some(fp(py, &target, args)),
                    None => dispatch_generic(py, fns, target, args),
                }
            }
        };
        (@reverse_call $fp:ident, $py:ident, $tgt:ident, [$($acc:ident),*], $head:ident $(, $rest:ident)*) => {
            dispatch_n!(@reverse_call $fp, $py, $tgt, [$head $(, $acc)*], $($rest),*)
        };
        (@reverse_call $fp:ident, $py:ident, $tgt:ident, [$($acc:ident),*],) => {
            $fp($py, &$tgt, $($acc),*)
        };
    }
    match args.len() {
        0  => match fns.invoke0 {
            Some(fp) => Some(fp(py, &target)),
            None => match fns.invoke_variadic {
                Some(fp) => Some(fp(py, &target, args)),
                None => dispatch_generic(py, fns, target, args),
            }
        },
        1  => dispatch_n!(invoke1,  a),
        2  => dispatch_n!(invoke2,  a, b),
        3  => dispatch_n!(invoke3,  a, b, c),
        4  => dispatch_n!(invoke4,  a, b, c, d),
        5  => dispatch_n!(invoke5,  a, b, c, d, e),
        6  => dispatch_n!(invoke6,  a, b, c, d, e, f),
        7  => dispatch_n!(invoke7,  a, b, c, d, e, f, g),
        8  => dispatch_n!(invoke8,  a, b, c, d, e, f, g, h),
        9  => dispatch_n!(invoke9,  a, b, c, d, e, f, g, h, i),
        10 => dispatch_n!(invoke10, a, b, c, d, e, f, g, h, i, j),
        11 => dispatch_n!(invoke11, a, b, c, d, e, f, g, h, i, j, k),
        12 => dispatch_n!(invoke12, a, b, c, d, e, f, g, h, i, j, k, l),
        13 => dispatch_n!(invoke13, a, b, c, d, e, f, g, h, i, j, k, l, mm),
        14 => dispatch_n!(invoke14, a, b, c, d, e, f, g, h, i, j, k, l, mm, n),
        15 => dispatch_n!(invoke15, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o),
        16 => dispatch_n!(invoke16, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p),
        17 => dispatch_n!(invoke17, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q),
        18 => dispatch_n!(invoke18, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r),
        19 => dispatch_n!(invoke19, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r, s),
        20 => dispatch_n!(invoke20, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r, s, t),
        _  => match fns.invoke_variadic {
            Some(fp) => Some(fp(py, &target, args)),
            None => dispatch_generic(py, fns, target, args),
        }
    }
}

/// Helper: if `fns.generic` holds a Py<PyAny> impl, call it with
/// (target, *args) via CPython tp_call. Otherwise `None`.
fn dispatch_generic(
    py: Python<'_>,
    fns: &InvokeFns,
    target: PyObject,
    args: Vec<PyObject>,
) -> Option<PyResult<PyObject>> {
    let Some(generic_fn) = &fns.generic else { return None; };
    let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
    call_args.push(target);
    call_args.extend(args);
    let tup = match pyo3::types::PyTuple::new(py, &call_args) {
        Ok(t) => t,
        Err(e) => return Some(Err(e)),
    };
    Some(generic_fn.bind(py).call1(tup).map(|b| b.unbind()))
}

/// Register a ProtocolFn under (protocol_name, method_name). Overwrites any
/// prior entry — module re-init shouldn't happen in a single process, but
/// idempotence keeps the API cheap rather than erroring.
pub fn register_protocol_fn(
    protocol_name: &str,
    method_name: &str,
    pfn: Py<ProtocolFn>,
) {
    registry().insert(
        (Arc::from(protocol_name), Arc::from(method_name)),
        pfn,
    );
}

/// Look up a ProtocolFn by (protocol_name, method_name). Returns None if the
/// declaration hasn't been registered yet — the expected steady-state is
/// that every `#[implements]` call finds its ProtocolFn here, because the
/// `#[protocol]` macro populated the registry earlier in module init.
pub fn get_protocol_fn(
    py: Python<'_>,
    protocol_name: &str,
    method_name: &str,
) -> Option<Py<ProtocolFn>> {
    registry()
        .get(&(Arc::from(protocol_name), Arc::from(method_name)))
        .map(|e| e.value().clone_ref(py))
}

/// Arity-specialized function-pointer table. One of these is stored per
/// `(ProtocolFn, type)` pair. `None` for an arity means "the impl doesn't
/// accept that many args" — dispatch then falls through to
/// `invoke_variadic`, then errors.
///
/// The function pointers take `&PyObject` for the receiver (target) so the
/// caller doesn't have to pre-clone; impls that need to hold the receiver
/// beyond the call do the clone_ref internally.
/// Type aliases for fn pointers of each arity, keeping the struct field
/// declarations readable. `A` is the target type (`&PyObject`); every
/// additional arg is `PyObject` (owned).
pub type InvokeFn0  = fn(Python<'_>, &PyObject) -> PyResult<PyObject>;
pub type InvokeFn1  = fn(Python<'_>, &PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn2  = fn(Python<'_>, &PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn3  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn4  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn5  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn6  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn7  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn8  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn9  = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn10 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn11 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn12 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn13 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn14 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn15 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn16 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn17 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn18 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn19 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFn20 = fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>;
pub type InvokeFnVariadic = fn(Python<'_>, &PyObject, Vec<PyObject>) -> PyResult<PyObject>;

pub struct InvokeFns {
    pub invoke0:  Option<InvokeFn0>,
    pub invoke1:  Option<InvokeFn1>,
    pub invoke2:  Option<InvokeFn2>,
    pub invoke3:  Option<InvokeFn3>,
    pub invoke4:  Option<InvokeFn4>,
    pub invoke5:  Option<InvokeFn5>,
    pub invoke6:  Option<InvokeFn6>,
    pub invoke7:  Option<InvokeFn7>,
    pub invoke8:  Option<InvokeFn8>,
    pub invoke9:  Option<InvokeFn9>,
    pub invoke10: Option<InvokeFn10>,
    pub invoke11: Option<InvokeFn11>,
    pub invoke12: Option<InvokeFn12>,
    pub invoke13: Option<InvokeFn13>,
    pub invoke14: Option<InvokeFn14>,
    pub invoke15: Option<InvokeFn15>,
    pub invoke16: Option<InvokeFn16>,
    pub invoke17: Option<InvokeFn17>,
    pub invoke18: Option<InvokeFn18>,
    pub invoke19: Option<InvokeFn19>,
    pub invoke20: Option<InvokeFn20>,
    pub invoke_variadic: Option<InvokeFnVariadic>,
    /// Catch-all for runtime-registered impls (e.g. `(extend-type T P ...)`
    /// from Clojure code) where the impl is a `Py<PyAny>` callable rather
    /// than a typed fn pointer. When set AND the matching arity slot is
    /// None, dispatch calls this via CPython `call1(target, *args)`.
    pub generic: Option<Py<pyo3::types::PyAny>>,
    /// Epoch at install time — if this entry was promoted by an MRO walk
    /// and the protocol's epoch has since advanced (re-extension of some
    /// type), the entry is treated as stale.
    pub epoch: u64,
    /// True iff this entry was installed by a promote (MRO match), not a
    /// direct extend. Direct extensions are always authoritative.
    pub promoted: bool,
}

impl InvokeFns {
    pub fn empty() -> Self {
        Self {
            invoke0: None, invoke1: None, invoke2: None, invoke3: None, invoke4: None,
            invoke5: None, invoke6: None, invoke7: None, invoke8: None, invoke9: None,
            invoke10: None, invoke11: None, invoke12: None, invoke13: None, invoke14: None,
            invoke15: None, invoke16: None, invoke17: None, invoke18: None, invoke19: None,
            invoke20: None,
            invoke_variadic: None,
            generic: None,
            epoch: 0,
            promoted: false,
        }
    }

    /// Clone-with-GIL: `Py<PyAny>` doesn't implement `Clone` directly,
    /// so fn-pointer fields are copied bitwise and the Py<PyAny> is
    /// cloned via `clone_ref`.
    pub fn clone_with_gil(&self, py: Python<'_>) -> Self {
        Self {
            invoke0: self.invoke0, invoke1: self.invoke1, invoke2: self.invoke2,
            invoke3: self.invoke3, invoke4: self.invoke4, invoke5: self.invoke5,
            invoke6: self.invoke6, invoke7: self.invoke7, invoke8: self.invoke8,
            invoke9: self.invoke9, invoke10: self.invoke10, invoke11: self.invoke11,
            invoke12: self.invoke12, invoke13: self.invoke13, invoke14: self.invoke14,
            invoke15: self.invoke15, invoke16: self.invoke16, invoke17: self.invoke17,
            invoke18: self.invoke18, invoke19: self.invoke19, invoke20: self.invoke20,
            invoke_variadic: self.invoke_variadic,
            generic: self.generic.as_ref().map(|g| g.clone_ref(py)),
            epoch: self.epoch,
            promoted: self.promoted,
        }
    }
}

/// A protocol method — a callable with a per-type dispatch table.
///
/// Semantically replaces `ProtocolMethod` but the impl storage is typed
/// and per-fn rather than a name-keyed hashmap shared by the whole
/// protocol.
#[pyclass(module = "clojure._core", name = "ProtocolFn", frozen)]
pub struct ProtocolFn {
    /// Method name — used only in error messages.
    pub name: String,
    /// Declaring protocol's name — used only in error messages.
    pub protocol_name: String,
    /// True iff the declaring protocol opted into extend-via-metadata.
    /// Phase 1 stores this but does not yet consult it; Phase 3 wires
    /// the metadata consult.
    pub via_metadata: bool,
    /// Dispatch table: Python type pointer -> impl fns.
    pub cache: DashMap<CacheKey, Arc<InvokeFns>>,
    /// Monotonic counter, bumped on every `extend_type_*`. Promoted MRO
    /// entries older than the current epoch are treated as stale.
    pub epoch: AtomicU64,
}

impl ProtocolFn {
    /// Look up the impl for `target`. Mirrors the classic three-step
    /// dispatch: exact type, MRO walk (with promotion), metadata fallback.
    /// Returns None when no impl is found.
    pub(crate) fn resolve(&self, py: Python<'_>, target: &PyObject) -> PyResult<Option<Arc<InvokeFns>>> {
        let ty = target.bind(py).get_type();
        let exact_key = CacheKey::for_py_type(&ty);
        let current_epoch = self.epoch.load(Ordering::Acquire);

        // Step 1: exact-type hit.
        if let Some(entry) = self.cache.get(&exact_key) {
            let fns = Arc::clone(entry.value());
            drop(entry);
            if !fns.promoted || fns.epoch == current_epoch {
                return Ok(Some(fns));
            }
            // Stale promoted entry — fall through to re-walk.
        }

        // Step 2: MRO walk + promotion. Skip index 0 (= exact_ty).
        let mro = ty.getattr("__mro__")?;
        let mro_tuple: pyo3::Bound<'_, PyTuple> = mro.cast_into()?;
        for parent in mro_tuple.iter().skip(1) {
            let parent_ty: pyo3::Bound<'_, PyType> = parent.cast_into()?;
            let pk = CacheKey::for_py_type(&parent_ty);
            if let Some(entry) = self.cache.get(&pk) {
                let parent_fns = Arc::clone(entry.value());
                drop(entry);
                // Promote: install a copy at exact_key stamped with current epoch.
                let mut promoted_fns = parent_fns.clone_with_gil(py);
                promoted_fns.epoch = current_epoch;
                promoted_fns.promoted = true;
                let promoted = Arc::new(promoted_fns);
                self.cache.insert(exact_key, Arc::clone(&promoted));
                return Ok(Some(promoted));
            }
        }

        // Step 3: metadata fallback (Phase 3).
        if self.via_metadata {
            // Deferred until Phase 3; keeps the flag reachable.
        }

        Ok(None)
    }

    fn raise_no_impl(&self, py: Python<'_>, target: &PyObject) -> PyErr {
        let ty_repr = target
            .bind(py)
            .get_type()
            .qualname()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "?".into());
        IllegalArgumentException::new_err(format!(
            "No implementation of method: {} of protocol: {} found for class: {}",
            self.name, self.protocol_name, ty_repr
        ))
    }

    /// Install a typed impl in the dispatch table. Overwrites any prior
    /// entry for this type — direct extensions are authoritative.
    /// Bumps the epoch so any previously-promoted MRO entries become stale.
    pub fn extend_with_native(
        &self,
        ty: pyo3::Bound<'_, pyo3::types::PyType>,
        mut fns: InvokeFns,
    ) {
        let epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        fns.epoch = epoch;
        fns.promoted = false;
        let key = CacheKey::for_py_type(&ty);
        self.cache.insert(key, Arc::new(fns));
    }

    /// Rust-side entry point. Called by `rt::invoke_n_owned` when target
    /// is a ProtocolFn. Takes ownership of `args`.
    ///
    /// On typed-table miss, consults the declaring protocol's fallback
    /// closure (if any). The fallback's job is to populate the typed
    /// cache for the encountered type; after it runs, we retry once. If
    /// the fallback is absent or didn't populate a matching entry, we
    /// raise "no impl".
    pub fn dispatch_owned(
        slf: Py<Self>,
        py: Python<'_>,
        target: PyObject,
        mut args: Vec<PyObject>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        let fns = match this.resolve(py, &target)? {
            Some(f) => f,
            None => return dispatch_fallback_miss(py, &slf, target, args),
        };
        // Arity-specialized dispatch. Each match arm: if the corresponding
        // typed fn pointer is present, drain the right number of args from
        // the Vec and call directly. Otherwise fall through to the variadic
        // path, which packs args into a single Vec and calls the variadic
        // impl if one exists.
        //
        // Helper macro: each arm pops N args in reverse (so the reverse
        // order below produces (a, b, c, …) in call order).
        macro_rules! dispatch_n {
            ($fns_field:ident, $($arg:ident),*) => {
                match fns.$fns_field {
                    Some(fp) => {
                        $(let $arg = args.pop().unwrap();)*
                        dispatch_n!(@reverse_call fp, py, target, [], $($arg),*)
                    }
                    None => this.try_variadic(py, fns.as_ref(), target, args),
                }
            };
            // Reverse the popped-args list so the call expression is in
            // declaration (left-to-right) order. Accumulator pattern.
            (@reverse_call $fp:ident, $py:ident, $tgt:ident, [$($acc:ident),*], $head:ident $(, $rest:ident)*) => {
                dispatch_n!(@reverse_call $fp, $py, $tgt, [$head $(, $acc)*], $($rest),*)
            };
            (@reverse_call $fp:ident, $py:ident, $tgt:ident, [$($acc:ident),*],) => {
                $fp($py, &$tgt, $($acc),*)
            };
        }
        match args.len() {
            0  => match fns.invoke0 {
                Some(fp) => fp(py, &target),
                None => this.try_variadic(py, fns.as_ref(), target, args),
            },
            1  => dispatch_n!(invoke1,  a),
            2  => dispatch_n!(invoke2,  a, b),
            3  => dispatch_n!(invoke3,  a, b, c),
            4  => dispatch_n!(invoke4,  a, b, c, d),
            5  => dispatch_n!(invoke5,  a, b, c, d, e),
            6  => dispatch_n!(invoke6,  a, b, c, d, e, f),
            7  => dispatch_n!(invoke7,  a, b, c, d, e, f, g),
            8  => dispatch_n!(invoke8,  a, b, c, d, e, f, g, h),
            9  => dispatch_n!(invoke9,  a, b, c, d, e, f, g, h, i),
            10 => dispatch_n!(invoke10, a, b, c, d, e, f, g, h, i, j),
            11 => dispatch_n!(invoke11, a, b, c, d, e, f, g, h, i, j, k),
            12 => dispatch_n!(invoke12, a, b, c, d, e, f, g, h, i, j, k, l),
            13 => dispatch_n!(invoke13, a, b, c, d, e, f, g, h, i, j, k, l, mm),
            14 => dispatch_n!(invoke14, a, b, c, d, e, f, g, h, i, j, k, l, mm, n),
            15 => dispatch_n!(invoke15, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o),
            16 => dispatch_n!(invoke16, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p),
            17 => dispatch_n!(invoke17, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q),
            18 => dispatch_n!(invoke18, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r),
            19 => dispatch_n!(invoke19, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r, s),
            20 => dispatch_n!(invoke20, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r, s, t),
            _  => this.try_variadic(py, fns.as_ref(), target, args),
        }
    }

    fn try_variadic(
        &self,
        py: Python<'_>,
        fns: &InvokeFns,
        target: PyObject,
        args: Vec<PyObject>,
    ) -> PyResult<PyObject> {
        if let Some(fp) = fns.invoke_variadic {
            return fp(py, &target, args);
        }
        if let Some(r) = dispatch_generic(py, fns, target, args) {
            return r;
        }
        Err(IllegalArgumentException::new_err(format!(
            "Protocol method {} of protocol {}: no impl",
            self.name, self.protocol_name,
        )))
    }
}

#[pymethods]
impl ProtocolFn {
    /// Construct an empty ProtocolFn. Phase-2 macros and Phase-1 tests
    /// both use this; in a fully-migrated world the macro emits these at
    /// protocol-declaration time.
    #[new]
    pub fn new_py(name: String, protocol_name: String, via_metadata: bool) -> Self {
        Self {
            name,
            protocol_name,
            via_metadata,
            cache: DashMap::new(),
            epoch: AtomicU64::new(0),
        }
    }

    fn __repr__(&self) -> String {
        format!("#<ProtocolFn {}/{}>", self.protocol_name, self.name)
    }

    /// Python-visible `__call__`. Target is always `args[0]`, remaining
    /// args are extras. Mirrors how Clojure code calls a protocol method:
    /// `(first coll)` => `first.__call__(coll)`, i.e. target=coll args=[].
    #[pyo3(signature = (*args))]
    fn __call__(
        slf: Py<Self>,
        py: Python<'_>,
        args: pyo3::Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        if args.is_empty() {
            let this = slf.bind(py).get();
            return Err(IllegalArgumentException::new_err(format!(
                "Protocol method {} requires at least one arg (the target)",
                this.name
            )));
        }
        let target: PyObject = args.get_item(0)?.unbind();
        let mut rest: Vec<PyObject> = Vec::with_capacity(args.len().saturating_sub(1));
        for i in 1..args.len() {
            rest.push(args.get_item(i)?.unbind());
        }
        ProtocolFn::dispatch_owned(slf, py, target, rest)
    }
}

pub(crate) fn register(
    _py: Python<'_>,
    m: &pyo3::Bound<'_, pyo3::types::PyModule>,
) -> PyResult<()> {
    m.add_class::<ProtocolFn>()?;
    Ok(())
}
