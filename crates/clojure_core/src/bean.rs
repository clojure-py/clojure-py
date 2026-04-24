//! `Bean` — live, lazy view of a Python object's bean-style attributes.
//!
//! Mirrors vanilla `clojure.core/bean`: at construction we reflect on the
//! wrapped object's attribute names *once* (the analogue of vanilla's
//! `Introspector.getBeanInfo` snapshot of property descriptors), but each
//! `val_at` / `seq` access calls `getattr` on the original object — so
//! mutations to the underlying object are reflected by subsequent
//! lookups, matching vanilla's APersistentMap proxy semantics.
//!
//! Filters: skip attribute names starting with `_` (dunders + private
//! convention) and skip values that are `callable` (methods,
//! classmethods, staticmethods). `@property`-decorated values pass
//! through because they're invoked at `getattr` time and the *result* is
//! what we test.

use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::ifn::IFn;
use crate::ihasheq::IHashEq;
use crate::ilookup::ILookup;
use crate::iseqable::ISeqable;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Bean", frozen)]
pub struct Bean {
    pub obj: PyObject,
    /// Property names captured once at bean creation. Each lookup uses
    /// these to validate "did we know about this key" before calling
    /// `getattr` (so we don't accidentally expose newly-added attributes
    /// or callable shadows).
    pub keys: Vec<Arc<str>>,
}

impl Bean {
    pub fn create(py: Python<'_>, obj: PyObject) -> PyResult<Self> {
        let names_obj = py
            .import("builtins")?
            .getattr("dir")?
            .call1((obj.bind(py),))?;
        let names: Vec<String> = names_obj.extract()?;
        let mut keys = Vec::new();
        for n in names {
            if n.starts_with('_') {
                continue;
            }
            let attr = match obj.bind(py).getattr(n.as_str()) {
                Ok(a) => a,
                Err(_) => continue,
            };
            if attr.is_callable() {
                continue;
            }
            keys.push(Arc::from(n.as_str()));
        }
        Ok(Bean { obj, keys })
    }

    /// Extract the attribute name from a key (Keyword, str, or Symbol).
    /// Returns None for unsupported key types — falls through to "absent".
    fn name_of_key(py: Python<'_>, k: &PyObject) -> Option<String> {
        let b = k.bind(py);
        if let Ok(kw) = b.cast::<crate::keyword::Keyword>() {
            return Some(kw.get().name.to_string());
        }
        if let Ok(sym) = b.cast::<crate::symbol::Symbol>() {
            return Some(sym.get().name.to_string());
        }
        b.extract::<String>().ok()
    }

    fn has_key(&self, name: &str) -> bool {
        self.keys.iter().any(|k| k.as_ref() == name)
    }

    fn fetch(&self, py: Python<'_>, name: &str) -> Option<PyObject> {
        self.obj
            .bind(py)
            .getattr(name)
            .ok()
            .map(|a| a.unbind())
    }
}

#[pymethods]
impl Bean {
    fn __len__(&self) -> usize {
        self.keys.len()
    }

    fn __contains__(&self, py: Python<'_>, k: PyObject) -> bool {
        match Bean::name_of_key(py, &k) {
            Some(n) => self.has_key(&n),
            None => false,
        }
    }

    fn __getitem__(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let name = Bean::name_of_key(py, &k).ok_or_else(|| {
            pyo3::exceptions::PyKeyError::new_err("invalid bean key type")
        })?;
        if !self.has_key(&name) {
            return Err(pyo3::exceptions::PyKeyError::new_err(name));
        }
        self.fetch(py, &name)
            .ok_or_else(|| pyo3::exceptions::PyAttributeError::new_err(name))
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<crate::seqs::cons::ConsIter>> {
        // Iterate the seq (yields MapEntries for compatibility with map iter).
        let s = <Bean as ISeqable>::seq(slf, py)?;
        Py::new(py, crate::seqs::cons::ConsIter { current: s })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let s = slf.bind(py).get();
        let mut out = String::from("{");
        let mut first = true;
        for k in s.keys.iter() {
            let v = match s.fetch(py, k.as_ref()) {
                Some(v) => v,
                None => continue,
            };
            if !first {
                out.push_str(", ");
            }
            first = false;
            out.push(':');
            out.push_str(k);
            out.push(' ');
            out.push_str(&crate::printer::print::pr_str(py, v)?);
        }
        out.push('}');
        Ok(out)
    }
}

#[implements(Counted)]
impl Counted for Bean {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().keys.len())
    }
}

#[implements(ILookup)]
impl ILookup for Bean {
    fn val_at(
        this: Py<Self>,
        py: Python<'_>,
        k: PyObject,
        not_found: PyObject,
    ) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let name = match Bean::name_of_key(py, &k) {
            Some(n) => n,
            None => return Ok(not_found),
        };
        if !s.has_key(&name) {
            return Ok(not_found);
        }
        Ok(s.fetch(py, &name).unwrap_or(not_found))
    }
}

#[implements(ISeqable)]
impl ISeqable for Bean {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.keys.is_empty() {
            return Ok(py.None());
        }
        // Build a seq of MapEntries by reading values right now (live).
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for k in s.keys.iter().rev() {
            let v = match s.fetch(py, k.as_ref()) {
                Some(v) => v,
                None => continue,
            };
            let kw = crate::keyword::keyword(py, k.as_ref(), None)?;
            let me = crate::collections::map_entry::MapEntry::new(kw.into_any(), v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            let cons = crate::seqs::cons::Cons::new(me_py, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
}

#[implements(IFn)]
impl IFn for Bean {
    fn invoke1(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        <Bean as ILookup>::val_at(this, py, k, py.None())
    }
    fn invoke2(
        this: Py<Self>,
        py: Python<'_>,
        k: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        <Bean as ILookup>::val_at(this, py, k, default)
    }
}

#[implements(IEquiv)]
impl IEquiv for Bean {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        // Snapshot ourselves and the other side, then compare entry-wise.
        // Matches vanilla bean's APersistentMap.equiv (which materializes
        // entries on demand).
        let s = this.bind(py).get();
        let other_count = match crate::rt::count(py, other.clone_ref(py)) {
            Ok(n) => n,
            Err(_) => return Ok(false),
        };
        if other_count != s.keys.len() {
            return Ok(false);
        }
        let sentinel: PyObject = pyo3::types::PyList::empty(py).unbind().into_any();
        for k in s.keys.iter() {
            let v_self = match s.fetch(py, k.as_ref()) {
                Some(v) => v,
                None => return Ok(false),
            };
            let kw = crate::keyword::keyword(py, k.as_ref(), None)?;
            let kw_obj: PyObject = kw.into_any();
            let v_other = crate::rt::get(
                py,
                other.clone_ref(py),
                kw_obj,
                sentinel.clone_ref(py),
            )?;
            if crate::rt::identical(py, v_other.clone_ref(py), sentinel.clone_ref(py)) {
                return Ok(false);
            }
            if !crate::rt::equiv(py, v_self, v_other)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[implements(IHashEq)]
impl IHashEq for Bean {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // XOR-fold over (hash key XOR hash value) — matches the
        // PersistentHashMap convention (insertion-order-independent).
        let s = this.bind(py).get();
        let mut h: i64 = 0;
        for k in s.keys.iter() {
            let v = match s.fetch(py, k.as_ref()) {
                Some(v) => v,
                None => continue,
            };
            let kw = crate::keyword::keyword(py, k.as_ref(), None)?;
            let kw_obj: PyObject = kw.into_any();
            let kh = crate::rt::hash_eq(py, kw_obj)?;
            let vh = crate::rt::hash_eq(py, v)?;
            h = h.wrapping_add(kh ^ vh);
        }
        Ok(h)
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Bean>()?;
    Ok(())
}
