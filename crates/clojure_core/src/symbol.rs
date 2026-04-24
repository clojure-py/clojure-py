use crate::ifn::IFn;
use crate::imeta::IMeta;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

const SYMBOL_HASH_TAG: u64 = 0x5359_4D42_4F4C_5F5F; // ASCII "SYMBOL__"

#[pyclass(module = "clojure._core", name = "Symbol", frozen)]
pub struct Symbol {
    pub ns: Option<Arc<str>>,
    pub name: Arc<str>,
    pub hash_cache: u32,
    pub meta: Option<PyObject>,
}

impl Symbol {
    pub fn new(ns: Option<Arc<str>>, name: Arc<str>) -> Self {
        let h = compute_hash(ns.as_deref(), &name);
        Self { ns, name, hash_cache: h, meta: None }
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
        let Ok(o) = other.cast::<Self>() else { return false; };
        let o = o.get();
        self.ns.as_deref() == o.ns.as_deref() && *self.name == *o.name
    }

    fn __hash__(&self) -> u32 {
        self.hash_cache
    }

    pub fn __repr__(&self) -> String {
        match &self.ns {
            Some(n) => format!("{}/{}", n, self.name),
            None => self.name.to_string(),
        }
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.as_ref().map(|o| o.clone_ref(py))
    }

    // Callable form: ('s m) / ('s m default) — matches vanilla Symbol.invoke.
    #[pyo3(signature = (coll, default=None))]
    fn __call__(
        slf: Py<Self>,
        py: Python<'_>,
        coll: PyObject,
        default: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let key: PyObject = slf.into_any();
        crate::rt::get(py, coll, key, default.unwrap_or_else(|| py.None()))
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

#[implements(IFn)]
impl IFn for Symbol {
    fn invoke1(this: Py<Self>, py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
        let key: PyObject = this.into_any();
        crate::rt::get(py, coll, key, py.None())
    }
    fn invoke2(
        this: Py<Self>,
        py: Python<'_>,
        coll: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        let key: PyObject = this.into_any();
        crate::rt::get(py, coll, key, default)
    }
    fn invoke_variadic(
        this: Py<Self>,
        py: Python<'_>,
        args: Bound<'_, pyo3::types::PyTuple>,
    ) -> PyResult<PyObject> {
        match args.len() {
            1 => {
                let coll = args.get_item(0)?.unbind();
                <Symbol as IFn>::invoke1(this, py, coll)
            }
            2 => {
                let coll = args.get_item(0)?.unbind();
                let default = args.get_item(1)?.unbind();
                <Symbol as IFn>::invoke2(this, py, coll, default)
            }
            n => Err(crate::exceptions::ArityException::new_err(format!(
                "Wrong number of args ({}) passed to symbol",
                n
            ))),
        }
    }
}

#[implements(IMeta)]
impl IMeta for Symbol {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, Symbol {
            ns: s.ns.clone(),
            name: s.name.clone(),
            hash_cache: s.hash_cache,
            meta: m,
        })?.into_any())
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Symbol>()?;
    m.add_function(wrap_pyfunction!(symbol, m)?)?;
    Ok(())
}
