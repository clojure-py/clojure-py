//! Atom — the Clojure reference type for uncoordinated, synchronous,
//! independent state. Value updates go through a CAS loop; watches and
//! validators follow the same protocol as Vars.
//!
//! Backed by `arc_swap::ArcSwap<PyObject>` so reads are lock-free and
//! `swap!` is a natural CAS retry loop.

use crate::exceptions::IllegalArgumentException;
use crate::ideref::IDeref;
use crate::imeta::IMeta;
use arc_swap::ArcSwap;
use clojure_core_macros::implements;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Atom", frozen)]
pub struct Atom {
    /// Current value. Always populated (unlike Var, an Atom must be
    /// constructed with a value).
    pub value: ArcSwap<PyObject>,
    pub meta: ArcSwap<Option<PyObject>>,
    pub validator: ArcSwap<Option<PyObject>>,
    pub watches: RwLock<Py<PyDict>>,
}

impl Atom {
    pub fn new(py: Python<'_>, initial: PyObject) -> Self {
        Self {
            value: ArcSwap::new(Arc::new(initial)),
            meta: ArcSwap::new(Arc::new(None)),
            validator: ArcSwap::new(Arc::new(None)),
            watches: RwLock::new(PyDict::new(py).unbind()),
        }
    }

    fn validate(&self, py: Python<'_>, v: &PyObject) -> PyResult<()> {
        let validator = {
            let g = self.validator.load();
            let opt: &Option<PyObject> = &g;
            opt.as_ref().map(|o| o.clone_ref(py))
        };
        if let Some(vf) = validator {
            let r = vf.bind(py).call1((v.clone_ref(py),))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err(
                    "Invalid reference value",
                ));
            }
        }
        Ok(())
    }

    fn fire_watches(
        &self,
        py: Python<'_>,
        slf: &Py<Atom>,
        old: PyObject,
        new: PyObject,
    ) -> PyResult<()> {
        let watches_snapshot: Vec<(PyObject, PyObject)> = {
            let guard = self.watches.read();
            guard.bind(py).iter().map(|(k, v)| (k.unbind(), v.unbind())).collect()
        };
        for (k, f) in watches_snapshot {
            f.bind(py).call1((
                k,
                slf.clone_ref(py),
                old.clone_ref(py),
                new.clone_ref(py),
            ))?;
        }
        Ok(())
    }

    // --- Public (Rust-level) entry points so the RT helpers can call them ---

    pub fn reset(slf: Py<Self>, py: Python<'_>, new: PyObject) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        this.validate(py, &new)?;
        let old_arc = this.value.swap(Arc::new(new.clone_ref(py)));
        let old: PyObject = (*old_arc).clone_ref(py);
        this.fire_watches(py, &slf, old, new.clone_ref(py))?;
        Ok(new)
    }

    pub fn reset_vals(slf: Py<Self>, py: Python<'_>, new: PyObject) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        this.validate(py, &new)?;
        let old_arc = this.value.swap(Arc::new(new.clone_ref(py)));
        let old: PyObject = (*old_arc).clone_ref(py);
        this.fire_watches(py, &slf, old.clone_ref(py), new.clone_ref(py))?;
        let tup = PyTuple::new(py, &[old, new])?;
        let v = crate::collections::pvector::vector(py, tup)?;
        Ok(v.into_any())
    }

    pub fn compare_and_set(
        slf: Py<Self>,
        py: Python<'_>,
        expected: PyObject,
        new: PyObject,
    ) -> PyResult<bool> {
        let this = slf.bind(py).get();
        this.validate(py, &new)?;
        let current = this.value.load_full();
        let same = crate::rt::equiv(py, (*current).clone_ref(py), expected)?;
        if !same {
            return Ok(false);
        }
        let new_arc = Arc::new(new.clone_ref(py));
        let witnessed = this.value.compare_and_swap(&current, new_arc);
        if Arc::ptr_eq(&witnessed, &current) {
            let old: PyObject = (*current).clone_ref(py);
            this.fire_watches(py, &slf, old, new)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn swap(
        slf: Py<Self>,
        py: Python<'_>,
        f: PyObject,
        args: Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        loop {
            let current = this.value.load_full();
            let current_val = (*current).clone_ref(py);
            let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
            call_args.push(current_val);
            for a in args.iter() {
                call_args.push(a.unbind());
            }
            let new_val = crate::rt::invoke_n(py, f.clone_ref(py), &call_args)?;
            this.validate(py, &new_val)?;
            let new_arc = Arc::new(new_val.clone_ref(py));
            let witnessed = this.value.compare_and_swap(&current, new_arc);
            if Arc::ptr_eq(&witnessed, &current) {
                let old: PyObject = (*current).clone_ref(py);
                this.fire_watches(py, &slf, old, new_val.clone_ref(py))?;
                return Ok(new_val);
            }
        }
    }

    pub fn swap_vals(
        slf: Py<Self>,
        py: Python<'_>,
        f: PyObject,
        args: Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        loop {
            let current = this.value.load_full();
            let current_val = (*current).clone_ref(py);
            let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
            call_args.push(current_val);
            for a in args.iter() {
                call_args.push(a.unbind());
            }
            let new_val = crate::rt::invoke_n(py, f.clone_ref(py), &call_args)?;
            this.validate(py, &new_val)?;
            let new_arc = Arc::new(new_val.clone_ref(py));
            let witnessed = this.value.compare_and_swap(&current, new_arc);
            if Arc::ptr_eq(&witnessed, &current) {
                let old: PyObject = (*current).clone_ref(py);
                this.fire_watches(py, &slf, old.clone_ref(py), new_val.clone_ref(py))?;
                let tup = PyTuple::new(py, &[old, new_val])?;
                let v = crate::collections::pvector::vector(py, tup)?;
                return Ok(v.into_any());
            }
        }
    }
}

#[pymethods]
impl Atom {
    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let g = slf.bind(py).get().value.load();
        let v: &PyObject = &g;
        let s = v.bind(py).repr()?.extract::<String>()?;
        Ok(format!("#<Atom {}>", s))
    }

    #[getter(meta)]
    fn get_meta(&self, py: Python<'_>) -> PyObject {
        let g = self.meta.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn set_validator(&self, py: Python<'_>, validator: Option<PyObject>) -> PyResult<()> {
        // If the current value fails the new validator, reject it.
        if let Some(vf) = validator.as_ref() {
            let cur = {
                let g = self.value.load();
                let v: &PyObject = &g;
                v.clone_ref(py)
            };
            let r = vf.bind(py).call1((cur,))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err(
                    "Invalid reference state",
                ));
            }
        }
        self.validator.store(Arc::new(validator));
        Ok(())
    }

    fn get_validator(&self, py: Python<'_>) -> Option<PyObject> {
        let g = self.validator.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py))
    }

    fn add_watch(&self, py: Python<'_>, key: PyObject, f: PyObject) -> PyResult<()> {
        let guard = self.watches.read();
        guard.bind(py).set_item(key, f)?;
        Ok(())
    }

    fn remove_watch(&self, py: Python<'_>, key: PyObject) -> PyResult<()> {
        let guard = self.watches.read();
        guard.bind(py).del_item(key)?;
        Ok(())
    }

}

#[implements(IDeref)]
impl IDeref for Atom {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let g = this.bind(py).get().value.load();
        let v: &PyObject = &g;
        Ok(v.clone_ref(py))
    }
}

#[implements(IMeta)]
impl IMeta for Atom {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let g = this.bind(py).get().meta.load();
        let opt: &Option<PyObject> = &g;
        Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        // Atom is a reference type; vanilla Clojure uses `reset-meta!` /
        // `alter-meta!` for in-place mutation and treats `with-meta` as a
        // non-op returning the same atom. Match that: store meta and
        // return the same Py<Atom>.
        let a = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        a.meta.store(Arc::new(m));
        Ok(this.into_any())
    }
}

/// Python-side constructor: `clojure._core.atom(initial)`.
#[pyfunction]
#[pyo3(name = "atom")]
pub fn py_atom(py: Python<'_>, initial: PyObject) -> Atom {
    Atom::new(py, initial)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Atom>()?;
    m.add_function(wrap_pyfunction!(py_atom, m)?)?;
    Ok(())
}
