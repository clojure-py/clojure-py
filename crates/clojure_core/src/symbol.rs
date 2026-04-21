use pyo3::prelude::*;
use pyo3::types::PyAny;
use parking_lot::RwLock;
use std::sync::Arc;

type PyObject = Py<PyAny>;

const SYMBOL_HASH_TAG: u64 = 0x5359_4D42_4F4C_5F5F; // ASCII "SYMBOL__"

#[pyclass(module = "clojure._core", name = "Symbol", frozen)]
pub struct Symbol {
    pub ns: Option<Arc<str>>,
    pub name: Arc<str>,
    pub hash_cache: u32,
    pub meta: RwLock<Option<PyObject>>,
}

impl Symbol {
    pub fn new(ns: Option<Arc<str>>, name: Arc<str>) -> Self {
        let h = compute_hash(ns.as_deref(), &name);
        Self { ns, name, hash_cache: h, meta: RwLock::new(None) }
    }
}

fn compute_hash(ns: Option<&str>, name: &str) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = fxhash::FxHasher::default();
    SYMBOL_HASH_TAG.hash(&mut h);
    if let Some(n) = ns {
        n.hash(&mut h);
        "/".hash(&mut h);
    }
    name.hash(&mut h);
    h.finish() as u32
}

#[pymethods]
impl Symbol {
    #[getter]
    fn ns(&self) -> Option<&str> {
        self.ns.as_deref()
    }

    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        let Ok(o) = other.downcast::<Self>() else { return false; };
        let o = o.get();
        self.ns.as_deref() == o.ns.as_deref() && *self.name == *o.name
    }

    fn __hash__(&self) -> u32 {
        self.hash_cache
    }

    fn __repr__(&self) -> String {
        match &self.ns {
            Some(n) => format!("{}/{}", n, self.name),
            None => self.name.to_string(),
        }
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn with_meta(&self, meta: PyObject) -> Self {
        Self {
            ns: self.ns.clone(),
            name: self.name.clone(),
            hash_cache: self.hash_cache,
            meta: RwLock::new(Some(meta)),
        }
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }
}

#[pyfunction]
#[pyo3(signature = (ns_or_name, name=None))]
pub fn symbol(ns_or_name: &str, name: Option<&str>) -> Symbol {
    match name {
        Some(n) => Symbol::new(Some(Arc::from(ns_or_name)), Arc::from(n)),
        None => {
            if let Some((ns, nm)) = ns_or_name.split_once('/') {
                if !ns.is_empty() && !nm.is_empty() {
                    return Symbol::new(Some(Arc::from(ns)), Arc::from(nm));
                }
            }
            Symbol::new(None, Arc::from(ns_or_name))
        }
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Symbol>()?;
    m.add_function(wrap_pyfunction!(symbol, m)?)?;
    Ok(())
}
