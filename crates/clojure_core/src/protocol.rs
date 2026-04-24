use crate::symbol::Symbol;
use dashmap::DashMap;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyType};
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

type PyObject = Py<PyAny>;

/// Key type for the method cache. Holds an erased CPython `PyType*` (as usize).
/// (Rust-side `TypeId` keys will arrive with the `#[implements]` macro in Phase 4.)
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub struct CacheKey(pub usize);

impl CacheKey {
    pub fn for_py_type(ty: &Bound<'_, PyType>) -> Self {
        Self(ty.as_ptr() as usize)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Origin {
    InlineAttr,
    Extend,
    Metadata,
    Fallback,
}

pub struct MethodTable {
    pub impls: fxhash::FxHashMap<Arc<str>, PyObject>,
    pub origin: Origin,
    /// Epoch at which this entry was filled. For promoted entries (MRO hits
    /// copied to an exact-type key), this is the epoch captured at promotion
    /// time; the dispatcher treats such entries as stale when the protocol
    /// epoch advances. For direct extensions, this equals the epoch produced
    /// by the extending `extend_type` call itself.
    pub epoch: u64,
    /// True iff this entry was installed by `try_resolve` as an MRO
    /// promotion (i.e. the exact-type key is *not* the type that was
    /// directly extended). Direct extensions (`extend_type`) bypass the
    /// epoch staleness check — re-extending an unrelated type must not
    /// invalidate an already-correct direct impl.
    pub promoted: bool,
}

pub struct MethodCache {
    pub epoch: AtomicU64,
    pub entries: DashMap<CacheKey, Arc<MethodTable>>,
}

impl MethodCache {
    pub fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            entries: DashMap::new(),
        }
    }
    pub fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }
    pub fn lookup(&self, k: CacheKey) -> Option<Arc<MethodTable>> {
        self.entries.get(&k).map(|e| Arc::clone(e.value()))
    }
}

#[pyclass(module = "clojure._core", name = "Protocol", frozen)]
pub struct Protocol {
    pub name: Py<Symbol>,
    pub method_keys: SmallVec<[Arc<str>; 8]>,
    pub cache: Arc<MethodCache>,
    pub fallback: RwLock<Option<PyObject>>,
    pub via_metadata: bool,
}

#[pymethods]
impl Protocol {
    #[getter]
    fn name(&self, py: Python<'_>) -> Py<Symbol> {
        self.name.clone_ref(py)
    }

    #[getter]
    fn via_metadata(&self) -> bool {
        self.via_metadata
    }

    fn set_fallback(&self, fallback: Option<PyObject>) {
        *self.fallback.write() = fallback;
    }

    #[getter]
    fn fallback(&self, py: Python<'_>) -> Option<PyObject> {
        self.fallback.read().as_ref().map(|o| o.clone_ref(py))
    }

    /// Extend this protocol to a Python type with a map of method-name -> impl fn.
    ///
    /// Writes into the legacy `MethodCache`. The ProtocolFn dispatcher does
    /// a lazy mirror on cache miss (see `dispatch_fallback_miss` in
    /// `protocol_fn.rs`) so impls registered via this legacy entry point
    /// reach the new typed-dispatch path without clobbering any typed
    /// fn-pointer slots that `#[implements]` / the IFn fallback installed
    /// directly into the ProtocolFn.
    pub fn extend_type(
        &self,
        _py: Python<'_>,
        ty: Bound<'_, PyType>,
        impls: Bound<'_, PyDict>,
    ) -> PyResult<()> {
        let mut table = fxhash::FxHashMap::default();
        for (k, v) in impls.iter() {
            let k_str: String = k.extract()?;
            table.insert(Arc::from(k_str.as_str()), v.unbind());
        }
        let key = CacheKey::for_py_type(&ty);
        let new_epoch = self.cache.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.cache.entries.insert(
            key,
            Arc::new(MethodTable {
                impls: table,
                origin: Origin::Extend,
                epoch: new_epoch,
                promoted: false,
            }),
        );
        Ok(())
    }
}

impl Protocol {
    /// Look up an impl for `(type, method)` in the legacy MethodCache. Used
    /// by `ProtocolFn::dispatch_fallback_miss` to honor Python-side
    /// `Protocol.extend_type` calls that didn't write into the new
    /// ProtocolFn directly.
    pub(crate) fn lookup_legacy_impl(
        &self,
        py: Python<'_>,
        key: CacheKey,
        method: &str,
    ) -> Option<PyObject> {
        let entry = self.cache.entries.get(&key)?;
        let py_obj = entry.value().impls.get(method)?;
        Some(py_obj.clone_ref(py))
    }
}

/// Construct a fresh Protocol at runtime. Clojure-layer `defprotocol` uses
/// this to create protocols that participate in the same dispatch machinery
/// as Rust-defined `#[protocol]` traits. `ns`/`name` form the Clojure
/// `Symbol` identifying the protocol; `method_keys` lists the method
/// dispatch keys; `via_metadata` opts into `__clj_meta__` fallback lookup.
#[pyfunction]
#[pyo3(name = "create_protocol", signature = (ns, name, method_keys, via_metadata=false))]
pub fn create_protocol(
    py: Python<'_>,
    ns: String,
    name: String,
    method_keys: Vec<String>,
    via_metadata: bool,
) -> PyResult<Py<Protocol>> {
    let sym = Py::new(
        py,
        Symbol::new(Some(Arc::from(ns.as_str())), Arc::from(name.as_str())),
    )?;
    let keys: SmallVec<[Arc<str>; 8]> = method_keys
        .iter()
        .map(|s| Arc::from(s.as_str()))
        .collect();
    let proto = Protocol {
        name: sym,
        method_keys: keys,
        cache: Arc::new(MethodCache::new()),
        fallback: RwLock::new(None),
        via_metadata,
    };
    let proto_py = Py::new(py, proto)?;

    // Register alongside the ProtocolFn registry so legacy-dispatch
    // callers (and ProtocolFn's fallback-miss path) can find the old
    // Protocol by name.
    crate::protocol_fn::register_old_protocol(&name, proto_py.clone_ref(py));

    // Also create a ProtocolFn per method and register it. Clojure-level
    // `(extend-type T P ...)` attaches impls via ProtocolFn::extend_with_generic.
    for k in method_keys.iter() {
        let pfn = crate::protocol_fn::ProtocolFn::new_py(
            k.clone(),
            name.clone(),
            via_metadata,
        );
        let pfn_py = Py::new(py, pfn)?;
        crate::protocol_fn::register_protocol_fn(&name, k, pfn_py);
    }

    Ok(proto_py)
}

/// Construct a callable protocol-method dispatcher at runtime. Returns the
/// ProtocolFn created by `create_protocol` for this (protocol, method) pair.
/// Falls back to a freshly-created ProtocolFn if the registry lookup fails,
/// which shouldn't happen in practice.
#[pyfunction]
#[pyo3(name = "create_protocol_method")]
pub fn create_protocol_method(
    py: Python<'_>,
    protocol: Py<Protocol>,
    key: String,
) -> PyResult<Py<crate::protocol_fn::ProtocolFn>> {
    let proto_ref = protocol.bind(py).get();
    let proto_name: &str = &proto_ref.name.bind(py).get().name;
    if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, proto_name, &key) {
        return Ok(pfn);
    }
    // Defensive: registry miss (shouldn't happen). Create a fresh PFn and
    // register it so future lookups succeed.
    let via_md = proto_ref.via_metadata;
    let pfn = crate::protocol_fn::ProtocolFn::new_py(
        key.clone(),
        proto_name.to_string(),
        via_md,
    );
    let pfn_py = Py::new(py, pfn)?;
    crate::protocol_fn::register_protocol_fn(proto_name, &key, pfn_py.clone_ref(py));
    Ok(pfn_py)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Protocol>()?;
    m.add_function(wrap_pyfunction!(create_protocol, m)?)?;
    m.add_function(wrap_pyfunction!(create_protocol_method, m)?)?;
    Ok(())
}
