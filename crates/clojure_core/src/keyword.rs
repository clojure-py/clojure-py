use dashmap::DashMap;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use std::sync::Arc;

type PyObject = Py<PyAny>;

const KEYWORD_HASH_TAG: u64 = 0x4B45_5957_4F52_445F;  // ASCII "KEYWORD_"

type KeywordKey = (Option<Arc<str>>, Arc<str>);
static INTERN: Lazy<DashMap<KeywordKey, Py<Keyword>>> = Lazy::new(DashMap::new);

#[pyclass(module = "clojure._core", name = "Keyword", frozen)]
pub struct Keyword {
    pub ns: Option<Arc<str>>,
    pub name: Arc<str>,
    pub hash_cache: u32,
}

impl Keyword {
    fn compute_hash(ns: Option<&str>, name: &str) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut h = fxhash::FxHasher::default();
        KEYWORD_HASH_TAG.hash(&mut h);
        if let Some(n) = ns { n.hash(&mut h); "/".hash(&mut h); }
        name.hash(&mut h);
        h.finish() as u32
    }

    fn look_up_self(&self, py: Python<'_>) -> PyResult<Py<Keyword>> {
        let key = (self.ns.clone(), self.name.clone());
        INTERN.get(&key).map(|e| e.value().clone_ref(py)).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("keyword not interned")
        })
    }
}

#[pymethods]
impl Keyword {
    #[getter] fn ns(&self) -> Option<&str> { self.ns.as_deref() }
    #[getter] fn name(&self) -> &str { &self.name }

    fn __hash__(&self) -> u32 { self.hash_cache }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        let Ok(o) = other.downcast::<Self>() else { return false; };
        let o = o.get();
        // Interned -> pointer identity is sufficient; value-equality fallback for the brief
        // race window during concurrent insert.
        std::ptr::eq(self as *const _, o as *const _)
            || (self.ns.as_deref() == o.ns.as_deref() && *self.name == *o.name)
    }

    fn __repr__(&self) -> String {
        match &self.ns {
            Some(n) => format!(":{}/{}", n, self.name),
            None    => format!(":{}", self.name),
        }
    }

    fn __str__(&self) -> String { self.__repr__() }

    // Callable form: (:k m) or (:k m default)
    #[pyo3(signature = (coll, default=None))]
    fn __call__(&self, py: Python<'_>, coll: &Bound<'_, PyAny>, default: Option<PyObject>) -> PyResult<PyObject> {
        let self_key = self.look_up_self(py)?;
        // Dict lookup uses __hash__ + __eq__; interned keys -> pointer-equal -> hit.
        if let Ok(d) = coll.downcast::<PyDict>() {
            if let Some(v) = d.get_item(&self_key)? {
                return Ok(v.unbind());
            }
        } else {
            // Fall back to __getitem__ for dict-like objects.
            match coll.get_item(&self_key) {
                Ok(v) => return Ok(v.unbind()),
                Err(_) => {}
            }
        }
        Ok(default.unwrap_or_else(|| py.None()))
    }
}

#[pyfunction]
#[pyo3(signature = (ns_or_name, name=None))]
pub fn keyword(py: Python<'_>, ns_or_name: &str, name: Option<&str>) -> PyResult<Py<Keyword>> {
    let (ns_opt, name_str): (Option<Arc<str>>, Arc<str>) = match name {
        Some(n) => (Some(Arc::from(ns_or_name)), Arc::from(n)),
        None => {
            if let Some((n, nm)) = ns_or_name.split_once('/') {
                if !n.is_empty() && !nm.is_empty() {
                    (Some(Arc::from(n)), Arc::from(nm))
                } else {
                    (None, Arc::from(ns_or_name))
                }
            } else {
                (None, Arc::from(ns_or_name))
            }
        }
    };
    let key = (ns_opt.clone(), name_str.clone());

    // Fast path: already interned.
    if let Some(e) = INTERN.get(&key) {
        return Ok(e.value().clone_ref(py));
    }

    // Slow path: construct, try to insert, return whichever wins the race.
    let h = Keyword::compute_hash(ns_opt.as_deref(), &name_str);
    let new_kw = Py::new(py, Keyword {
        ns: ns_opt.clone(),
        name: name_str.clone(),
        hash_cache: h,
    })?;
    let entry = INTERN.entry(key).or_insert_with(|| new_kw.clone_ref(py));
    Ok(entry.value().clone_ref(py))
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Keyword>()?;
    m.add_function(wrap_pyfunction!(keyword, m)?)?;
    Ok(())
}
