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

#[pyclass(module = "clojure._core", name = "ProtocolMethod", frozen)]
pub struct ProtocolMethod {
    pub protocol: Py<Protocol>,
    pub key: Arc<str>,
}

#[pymethods]
impl ProtocolMethod {
    #[getter]
    fn protocol(&self, py: Python<'_>) -> Py<Protocol> {
        self.protocol.clone_ref(py)
    }

    #[getter]
    fn key(&self) -> &str {
        &self.key
    }

    #[pyo3(signature = (target, *args))]
    fn __call__(
        &self,
        py: Python<'_>,
        target: PyObject,
        args: Bound<'_, pyo3::types::PyTuple>,
    ) -> PyResult<PyObject> {
        crate::dispatch::dispatch(py, &self.protocol, &self.key, target, args)
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
        .into_iter()
        .map(|s| Arc::from(s.as_str()))
        .collect();
    let proto = Protocol {
        name: sym,
        method_keys: keys,
        cache: Arc::new(MethodCache::new()),
        fallback: RwLock::new(None),
        via_metadata,
    };
    Py::new(py, proto)
}

/// Construct a ProtocolMethod (callable dispatcher) at runtime.
#[pyfunction]
#[pyo3(name = "create_protocol_method")]
pub fn create_protocol_method(
    py: Python<'_>,
    protocol: Py<Protocol>,
    key: String,
) -> PyResult<Py<ProtocolMethod>> {
    Py::new(
        py,
        ProtocolMethod {
            protocol,
            key: Arc::from(key.as_str()),
        },
    )
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Protocol>()?;
    m.add_class::<ProtocolMethod>()?;
    m.add_function(wrap_pyfunction!(create_protocol, m)?)?;
    m.add_function(wrap_pyfunction!(create_protocol_method, m)?)?;
    Ok(())
}
