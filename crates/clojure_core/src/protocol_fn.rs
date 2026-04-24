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

/// Arity-specialized function-pointer table. One of these is stored per
/// `(ProtocolFn, type)` pair. `None` for an arity means "the impl doesn't
/// accept that many args" — dispatch then falls through to
/// `invoke_variadic`, then errors.
///
/// The function pointers take `&PyObject` for the receiver (target) so the
/// caller doesn't have to pre-clone; impls that need to hold the receiver
/// beyond the call do the clone_ref internally.
#[derive(Clone)]
pub struct InvokeFns {
    pub invoke0:  Option<fn(Python<'_>, &PyObject) -> PyResult<PyObject>>,
    pub invoke1:  Option<fn(Python<'_>, &PyObject, PyObject) -> PyResult<PyObject>>,
    pub invoke2:  Option<fn(Python<'_>, &PyObject, PyObject, PyObject) -> PyResult<PyObject>>,
    pub invoke3:  Option<fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>>,
    pub invoke4:  Option<fn(Python<'_>, &PyObject, PyObject, PyObject, PyObject, PyObject) -> PyResult<PyObject>>,
    pub invoke_variadic: Option<fn(Python<'_>, &PyObject, Vec<PyObject>) -> PyResult<PyObject>>,
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
            invoke0: None,
            invoke1: None,
            invoke2: None,
            invoke3: None,
            invoke4: None,
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
                let promoted = Arc::new(InvokeFns {
                    invoke0: parent_fns.invoke0,
                    invoke1: parent_fns.invoke1,
                    invoke2: parent_fns.invoke2,
                    invoke3: parent_fns.invoke3,
                    invoke4: parent_fns.invoke4,
                    invoke_variadic: parent_fns.invoke_variadic,
                    epoch: current_epoch,
                    promoted: true,
                });
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

    /// Rust-side entry point. Called by `rt::invoke_n_owned` when target
    /// is a ProtocolFn. Takes ownership of `args`.
    pub fn dispatch_owned(
        slf: Py<Self>,
        py: Python<'_>,
        target: PyObject,
        mut args: Vec<PyObject>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        let fns = match this.resolve(py, &target)? {
            Some(f) => f,
            None => return Err(this.raise_no_impl(py, &target)),
        };
        match args.len() {
            0 => match fns.invoke0 {
                Some(fp) => fp(py, &target),
                None => this.try_variadic(py, fns.as_ref(), target, args),
            },
            1 => match fns.invoke1 {
                Some(fp) => {
                    let a = args.pop().unwrap();
                    fp(py, &target, a)
                }
                None => this.try_variadic(py, fns.as_ref(), target, args),
            },
            2 => match fns.invoke2 {
                Some(fp) => {
                    let b = args.pop().unwrap();
                    let a = args.pop().unwrap();
                    fp(py, &target, a, b)
                }
                None => this.try_variadic(py, fns.as_ref(), target, args),
            },
            3 => match fns.invoke3 {
                Some(fp) => {
                    let c = args.pop().unwrap();
                    let b = args.pop().unwrap();
                    let a = args.pop().unwrap();
                    fp(py, &target, a, b, c)
                }
                None => this.try_variadic(py, fns.as_ref(), target, args),
            },
            4 => match fns.invoke4 {
                Some(fp) => {
                    let d = args.pop().unwrap();
                    let c = args.pop().unwrap();
                    let b = args.pop().unwrap();
                    let a = args.pop().unwrap();
                    fp(py, &target, a, b, c, d)
                }
                None => this.try_variadic(py, fns.as_ref(), target, args),
            },
            _ => this.try_variadic(py, fns.as_ref(), target, args),
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
    fn new_py(name: String, protocol_name: String, via_metadata: bool) -> Self {
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
