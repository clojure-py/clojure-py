//! PersistentHashMap — 32-way HAMT + separate nil-key slot.
//!
//! Port of clojure/lang/PersistentHashMap.java. This phase (8A) lands the
//! core struct and its pymethods; protocol trait impls follow in Phase 8B
//! and the TransientHashMap variant in 8C.

use crate::collections::phashmap_node::{fold_hash_i64, MNode};
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "PersistentHashMap", frozen)]
pub struct PersistentHashMap {
    pub count: u32,
    /// `None` means no non-nil entries.
    pub root: Option<Arc<MNode>>,
    pub has_null: bool,
    pub null_value: RwLock<Option<PyObject>>,
    /// 0 = uncomputed; else folded hash + 1.
    pub hash_cache: AtomicI64,
    pub meta: RwLock<Option<PyObject>>,
}

impl PersistentHashMap {
    pub fn new_empty() -> Self {
        Self {
            count: 0,
            root: None,
            has_null: false,
            null_value: RwLock::new(None),
            hash_cache: AtomicI64::new(0),
            meta: RwLock::new(None),
        }
    }

    fn clone_meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }

    fn clone_null(&self, py: Python<'_>) -> Option<PyObject> {
        self.null_value.read().as_ref().map(|o| o.clone_ref(py))
    }

    pub fn val_at_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        if key.is_none(py) {
            if self.has_null {
                return Ok(self.clone_null(py).unwrap_or_else(|| py.None()));
            }
            return Ok(py.None());
        }
        let Some(root) = &self.root else {
            return Ok(py.None());
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        Ok(root.find(py, 0, h, key)?.unwrap_or_else(|| py.None()))
    }

    pub fn val_at_default_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        if key.is_none(py) {
            return Ok(if self.has_null {
                self.clone_null(py).unwrap_or(default)
            } else {
                default
            });
        }
        let Some(root) = &self.root else {
            return Ok(default);
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        root.find_or_default(py, 0, h, key, default)
    }

    pub fn contains_key_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        if key.is_none(py) {
            return Ok(self.has_null);
        }
        let Some(root) = &self.root else { return Ok(false); };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        Ok(root.find(py, 0, h, key)?.is_some())
    }

    pub fn assoc_internal(
        &self,
        py: Python<'_>,
        key: PyObject,
        val: PyObject,
    ) -> PyResult<Self> {
        if key.is_none(py) {
            let new_count = if self.has_null { self.count } else { self.count + 1 };
            return Ok(Self {
                count: new_count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: true,
                null_value: RwLock::new(Some(val)),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        }
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let (new_root, added) = match &self.root {
            Some(r) => r.assoc(py, 0, h, key, val)?,
            None => MNode::create_leaf(py, 0, h, key, val)?,
        };
        Ok(Self {
            count: if added { self.count + 1 } else { self.count },
            root: Some(new_root),
            has_null: self.has_null,
            null_value: RwLock::new(self.clone_null(py)),
            hash_cache: AtomicI64::new(0),
            meta: RwLock::new(self.clone_meta(py)),
        })
    }

    pub fn without_internal(&self, py: Python<'_>, key: PyObject) -> PyResult<Self> {
        if key.is_none(py) {
            if !self.has_null {
                // no-op: return clone
                return Ok(Self {
                    count: self.count,
                    root: self.root.as_ref().map(Arc::clone),
                    has_null: false,
                    null_value: RwLock::new(None),
                    hash_cache: AtomicI64::new(0),
                    meta: RwLock::new(self.clone_meta(py)),
                });
            }
            return Ok(Self {
                count: self.count - 1,
                root: self.root.as_ref().map(Arc::clone),
                has_null: false,
                null_value: RwLock::new(None),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        }
        let Some(root) = &self.root else {
            return Ok(Self {
                count: self.count,
                root: None,
                has_null: self.has_null,
                null_value: RwLock::new(self.clone_null(py)),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        };
        let h = fold_hash_i64(crate::rt::hash_eq(py, key.clone_ref(py))?);
        let (new_root_opt, removed) = root.without(py, 0, h, key)?;
        if !removed {
            return Ok(Self {
                count: self.count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: self.has_null,
                null_value: RwLock::new(self.clone_null(py)),
                hash_cache: AtomicI64::new(0),
                meta: RwLock::new(self.clone_meta(py)),
            });
        }
        Ok(Self {
            count: self.count - 1,
            root: new_root_opt,
            has_null: self.has_null,
            null_value: RwLock::new(self.clone_null(py)),
            hash_cache: AtomicI64::new(0),
            meta: RwLock::new(self.clone_meta(py)),
        })
    }

    /// Collect every (key, value) pair, including the nil-key slot if present.
    pub fn collect_entries(&self, py: Python<'_>) -> Vec<(PyObject, PyObject)> {
        let mut out: Vec<(PyObject, PyObject)> = Vec::with_capacity(self.count as usize);
        if self.has_null {
            let v = self.clone_null(py).unwrap_or_else(|| py.None());
            out.push((py.None(), v));
        }
        if let Some(r) = &self.root {
            r.collect_entries(py, &mut out);
        }
        out
    }
}

#[pymethods]
impl PersistentHashMap {
    fn __len__(&self) -> usize {
        self.count as usize
    }

    fn __bool__(&self) -> bool {
        self.count > 0
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentHashMapKeyIter>> {
        let s = slf.bind(py).get();
        let entries = s.collect_entries(py);
        let keys: Vec<PyObject> = entries.into_iter().map(|(k, _)| k).collect();
        Py::new(py, PersistentHashMapKeyIter { keys, pos: 0 })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __getitem__(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        if !self.contains_key_internal(py, key.clone_ref(py))? {
            return Err(pyo3::exceptions::PyKeyError::new_err(
                key.bind(py).repr()?.extract::<String>()?,
            ));
        }
        self.val_at_internal(py, key)
    }

    fn __contains__(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.contains_key_internal(py, key)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let entries = self.collect_entries(py);
        let mut parts: Vec<String> = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let ks = k.bind(py).repr()?.extract::<String>()?;
            let vs = v.bind(py).repr()?.extract::<String>()?;
            parts.push(format!("{ks} {vs}"));
        }
        Ok(format!("{{{}}}", parts.join(", ")))
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.__repr__(py)
    }

    #[pyo3(signature = (key, /))]
    fn val_at(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        self.val_at_internal(py, key)
    }

    #[pyo3(signature = (key, default, /))]
    fn val_at_default(&self, py: Python<'_>, key: PyObject, default: PyObject) -> PyResult<PyObject> {
        self.val_at_default_internal(py, key, default)
    }

    #[pyo3(signature = (key, val, /))]
    fn assoc(&self, py: Python<'_>, key: PyObject, val: PyObject) -> PyResult<Py<PersistentHashMap>> {
        let new = self.assoc_internal(py, key, val)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (key, /))]
    fn without(&self, py: Python<'_>, key: PyObject) -> PyResult<Py<PersistentHashMap>> {
        let new = self.without_internal(py, key)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (key, /))]
    fn contains_key(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.contains_key_internal(py, key)
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta
            .read()
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<PersistentHashMap>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(
            py,
            Self {
                count: self.count,
                root: self.root.as_ref().map(Arc::clone),
                has_null: self.has_null,
                null_value: RwLock::new(self.clone_null(py)),
                hash_cache: AtomicI64::new(self.hash_cache.load(Ordering::Relaxed)),
                meta: RwLock::new(m),
            },
        )
    }
}

#[pyclass(module = "clojure._core", name = "PersistentHashMapKeyIter")]
pub struct PersistentHashMapKeyIter {
    keys: Vec<PyObject>,
    pos: usize,
}

#[pymethods]
impl PersistentHashMapKeyIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        if self.pos >= self.keys.len() {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        let item = self.keys[self.pos].clone_ref(py);
        self.pos += 1;
        Ok(item)
    }
}

// --- Python-facing constructor: (hash-map k1 v1 k2 v2 ...). ---

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn hash_map(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<Py<PersistentHashMap>> {
    if args.len() % 2 != 0 {
        return Err(crate::exceptions::IllegalArgumentException::new_err(
            "hash-map requires an even number of arguments",
        ));
    }
    let mut m = PersistentHashMap::new_empty();
    let mut i = 0usize;
    while i < args.len() {
        let k = args.get_item(i)?.unbind();
        let v = args.get_item(i + 1)?.unbind();
        m = m.assoc_internal(py, k, v)?;
        i += 2;
    }
    Py::new(py, m)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentHashMap>()?;
    m.add_class::<PersistentHashMapKeyIter>()?;
    m.add_function(wrap_pyfunction!(hash_map, m)?)?;
    Ok(())
}
