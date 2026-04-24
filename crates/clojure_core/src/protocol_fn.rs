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
/// fn pointer is called directly. Falls through to the old-style Protocol
/// dispatch on resolve miss, matching `dispatch_owned`'s semantics.
#[inline]
pub fn dispatch_cached_1(
    py: Python<'_>,
    cell: &once_cell::sync::OnceCell<Py<ProtocolFn>>,
    proto_name: &'static str,
    method_name: &'static str,
    target: PyObject,
) -> PyResult<PyObject> {
    let pfn = resolve_cached(py, cell, proto_name, method_name);
    let this = pfn.bind(py).get();
    match this.resolve(py, &target)? {
        Some(fns) => match fns.invoke0 {
            Some(fp) => fp(py, &target),
            None => {
                // Unusual but possible: type implements only invoke_variadic.
                dispatch_fallback(py, this, fns.as_ref(), target, Vec::new())
            }
        },
        None => dispatch_fallback_miss(py, this, target, Vec::new()),
    }
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
            None => dispatch_fallback(py, this, fns.as_ref(), target, vec![a]),
        },
        None => dispatch_fallback_miss(py, this, target, vec![a]),
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
            None => dispatch_fallback(py, this, fns.as_ref(), target, vec![a, b]),
        },
        None => dispatch_fallback_miss(py, this, target, vec![a, b]),
    }
}

// Helpers used by the dispatch_cached_N path when the typed slot is absent:
// try variadic, else fall through to old Protocol dispatch.
fn dispatch_fallback(
    py: Python<'_>,
    this: &ProtocolFn,
    fns: &InvokeFns,
    target: PyObject,
    args: Vec<PyObject>,
) -> PyResult<PyObject> {
    if let Some(fp) = fns.invoke_variadic {
        return fp(py, &target, args);
    }
    Err(crate::exceptions::IllegalArgumentException::new_err(format!(
        "Protocol method {} of protocol {}: no impl for arity {}",
        this.name, this.protocol_name, args.len()
    )))
}

fn dispatch_fallback_miss(
    py: Python<'_>,
    this: &ProtocolFn,
    target: PyObject,
    args: Vec<PyObject>,
) -> PyResult<PyObject> {
    // Fall through to the old-style Protocol dispatch so fallback-installed
    // impls (e.g. Counted's __len__ path for str) keep working.
    if let Some(old_proto) = get_old_protocol(py, &this.protocol_name) {
        let method_key: Arc<str> = Arc::from(this.name.as_str());
        let rest_tup = pyo3::types::PyTuple::new(py, &args)?;
        return crate::dispatch::dispatch(py, &old_proto, &method_key, target, rest_tup);
    }
    Err(this.raise_no_impl(py, &target))
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

#[derive(Clone)]
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
            epoch: 0,
            promoted: false,
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
    fn resolve(&self, py: Python<'_>, target: &PyObject) -> PyResult<Option<Arc<InvokeFns>>> {
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
                let mut promoted_fns = (*parent_fns).clone();
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
    /// On typed-table miss, falls through to the old-style Protocol (looked
    /// up by name in the global registry) so fallback-installed impls and
    /// any protocol not yet migrated to the typed path still dispatch
    /// correctly. This fallback is what makes Phase 3's binding-flip safe
    /// even for protocols with dynamic fallbacks like `Counted::__len__`.
    pub fn dispatch_owned(
        slf: Py<Self>,
        py: Python<'_>,
        target: PyObject,
        mut args: Vec<PyObject>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        let fns = match this.resolve(py, &target)? {
            Some(f) => f,
            None => {
                // Fall through to the old Protocol, if one was registered
                // under the same protocol name. This covers fallback-driven
                // impls (Counted's __len__, etc.) and anything else still
                // living on the old path.
                if let Some(old_proto) = get_old_protocol(py, &this.protocol_name) {
                    let method_key: Arc<str> = Arc::from(this.name.as_str());
                    let mut tup_items: Vec<PyObject> =
                        Vec::with_capacity(args.len() + 1);
                    tup_items.push(target.clone_ref(py));
                    tup_items.append(&mut args);
                    // The old dispatch takes args as a PyTuple (excluding
                    // target, which is passed separately).
                    let rest = &tup_items[1..];
                    let rest_tup = pyo3::types::PyTuple::new(py, rest)?;
                    return crate::dispatch::dispatch(
                        py,
                        &old_proto,
                        &method_key,
                        target,
                        rest_tup,
                    );
                }
                return Err(this.raise_no_impl(py, &target));
            }
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
        match fns.invoke_variadic {
            Some(fp) => fp(py, &target, args),
            None => Err(IllegalArgumentException::new_err(format!(
                "Protocol method {} of protocol {}: no impl for arity {}",
                self.name,
                self.protocol_name,
                args.len()
            ))),
        }
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
