//! Var — Clojure's namespace-scoped reference type.
//!
//! Each Var has a root value (or unbound), optional metadata, optional
//! validator, watches, and a dynamic flag. Dynamic binding stack + bound-fn*
//! come in later tasks (Phase 6). IFn impl + delegation dunders land in
//! Tasks 26-27.

use crate::exceptions::{IllegalArgumentException, IllegalStateException};
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};
use std::sync::atomic::{AtomicBool, Ordering};

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Var", frozen)]
pub struct Var {
    pub ns: PyObject,    // any module-like object (ClojureNamespace arrives in Phase 7)
    pub sym: PyObject,   // Symbol
    pub root: RwLock<Option<PyObject>>,
    pub dynamic: AtomicBool,
    pub meta: RwLock<Option<PyObject>>,
    pub watches: RwLock<Py<PyDict>>,
    pub validator: RwLock<Option<PyObject>>,
}

#[pymethods]
impl Var {
    #[new]
    pub fn new(py: Python<'_>, ns: PyObject, sym: PyObject) -> PyResult<Self> {
        Ok(Self {
            ns,
            sym,
            root: RwLock::new(None),
            dynamic: AtomicBool::new(false),
            meta: RwLock::new(None),
            watches: RwLock::new(PyDict::new(py).unbind()),
            validator: RwLock::new(None),
        })
    }

    fn deref(slf: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        if this.dynamic.load(std::sync::atomic::Ordering::Acquire) {
            let key: Py<PyAny> = slf.clone_ref(py).into_any();
            if let Some(v) = crate::binding::lookup_binding(py, &key) {
                return Ok(v);
            }
        }
        let guard = this.root.read();
        match guard.as_ref() {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(IllegalStateException::new_err(format!(
                "Var {}/{} is unbound",
                this.ns_name(py)?,
                this.sym_name(py)?
            ))),
        }
    }

    #[getter]
    fn ns(&self, py: Python<'_>) -> PyObject {
        self.ns.clone_ref(py)
    }

    #[getter]
    fn sym(&self, py: Python<'_>) -> PyObject {
        self.sym.clone_ref(py)
    }

    #[getter]
    fn is_dynamic(&self) -> bool {
        self.dynamic.load(Ordering::Acquire)
    }

    #[getter]
    fn is_bound(&self) -> bool {
        self.root.read().is_some()
    }

    fn set_dynamic(&self, v: bool) {
        self.dynamic.store(v, Ordering::Release);
    }

    /// Set the root value directly. Runs the validator (if any) before installing.
    /// Watches fire after a successful change.
    fn bind_root(slf: Py<Self>, py: Python<'_>, value: PyObject) -> PyResult<()> {
        let this = slf.bind(py).get();
        this.validate(py, &value)?;
        let old = {
            let mut guard = this.root.write();
            let prev = guard.take();
            *guard = Some(value.clone_ref(py));
            prev
        };
        this.fire_watches(py, &slf, old, Some(value))?;
        Ok(())
    }

    /// `(set! v val)` — mutate the current binding frame's entry for this var.
    /// Errors if called outside a `binding` block or if the var isn't in the current frame.
    fn set_bang(slf: Py<Self>, py: Python<'_>, val: PyObject) -> PyResult<()> {
        let key: Py<PyAny> = slf.clone_ref(py).into_any();
        crate::binding::set_binding(py, &key, val)
    }

    /// `(alter-var-root v f args)` — atomically set root to `(f old-root args...)`.
    /// Retries on contention; validates the proposed value before installing; fires watches.
    #[pyo3(signature = (f, *args))]
    fn alter_root(
        slf: Py<Self>,
        py: Python<'_>,
        f: PyObject,
        args: Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        loop {
            // Snapshot current value (or unbound).
            let current = this.root.read().as_ref().map(|v| v.clone_ref(py));
            let current_for_f = current
                .as_ref()
                .map(|v| v.clone_ref(py))
                .unwrap_or_else(|| py.None());

            // Call f(current, *args) — outside any lock, so Python code in f can
            // re-enter Var APIs safely.
            let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
            call_args.push(current_for_f);
            for a in args.iter() {
                call_args.push(a.unbind());
            }
            let tup = PyTuple::new(py, &call_args)?;
            let new_val = f.bind(py).call1(tup)?.unbind();
            this.validate(py, &new_val)?;

            // Install under write lock if the observed state still matches.
            let installed = {
                let mut guard = this.root.write();
                let observed = guard.as_ref().map(|v| v.clone_ref(py));
                if same_py(py, observed.as_ref(), current.as_ref()) {
                    *guard = Some(new_val.clone_ref(py));
                    Some((
                        current.as_ref().map(|v| v.clone_ref(py)),
                        new_val.clone_ref(py),
                    ))
                } else {
                    None
                }
            };

            match installed {
                Some((old, new)) => {
                    this.fire_watches(py, &slf, old, Some(new.clone_ref(py)))?;
                    return Ok(new);
                }
                None => {
                    // Retry — someone else changed root between snapshot and swap.
                    continue;
                }
            }
        }
    }

    fn set_validator(&self, validator: Option<PyObject>) {
        *self.validator.write() = validator;
    }

    fn get_validator(&self, py: Python<'_>) -> Option<PyObject> {
        self.validator.read().as_ref().map(|o| o.clone_ref(py))
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

    #[getter]
    fn watches(&self, py: Python<'_>) -> Py<PyDict> {
        self.watches.read().clone_ref(py)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "#'{}/{}",
            self.ns_name(py)?,
            self.sym_name(py)?
        ))
    }

    /// Var-as-callable: deref the root, then invoke it through the IFn protocol.
    /// Routing through `rt::invoke_n` preserves the design principle — the root
    /// is never called via Python `__call__` directly; IFn-implementing roots
    /// hit their method cache, plain Python callables go through the IFn
    /// fallback installed at module init.
    #[pyo3(signature = (*args))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        let items: Vec<PyObject> = (0..args.len())
            .map(|i| -> PyResult<_> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        crate::rt::invoke_n(py, root, &items)
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }

    fn set_meta(&self, meta: Option<PyObject>) {
        *self.meta.write() = meta;
    }
}

impl Var {
    /// Like `deref`, but callable from Rust code without a `Py<Self>`.
    /// Returns the root, or IllegalStateException if unbound.
    fn deref_raw(&self, py: Python<'_>) -> PyResult<PyObject> {
        match self.root.read().as_ref() {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(crate::exceptions::IllegalStateException::new_err(format!(
                "Var {}/{} is unbound",
                self.ns_name(py)?,
                self.sym_name(py)?
            ))),
        }
    }
    fn ns_name(&self, py: Python<'_>) -> PyResult<String> {
        let n = self.ns.bind(py).getattr("__name__")?;
        n.extract()
    }
    fn sym_name(&self, py: Python<'_>) -> PyResult<String> {
        // Symbol exposes `.name` as a getter from Task 8.
        let n = self.sym.bind(py).getattr("name")?;
        n.extract()
    }
    fn validate(&self, py: Python<'_>, v: &PyObject) -> PyResult<()> {
        let validator = self.validator.read().as_ref().map(|o| o.clone_ref(py));
        if let Some(validator) = validator {
            let r = validator.bind(py).call1((v.clone_ref(py),))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err("Invalid reference value"));
            }
        }
        Ok(())
    }
    fn fire_watches(
        &self,
        py: Python<'_>,
        slf: &Py<Var>,
        old: Option<PyObject>,
        new: Option<PyObject>,
    ) -> PyResult<()> {
        let watches_snapshot: Vec<(PyObject, PyObject)> = {
            let guard = self.watches.read();
            let bound = guard.bind(py);
            bound
                .iter()
                .map(|(k, v)| (k.unbind(), v.unbind()))
                .collect()
        };
        let old_obj = old.unwrap_or_else(|| py.None());
        let new_obj = new.unwrap_or_else(|| py.None());
        for (k, f) in watches_snapshot {
            f.bind(py).call1((
                k,
                slf.clone_ref(py),
                old_obj.clone_ref(py),
                new_obj.clone_ref(py),
            ))?;
        }
        Ok(())
    }
}

fn same_py(_py: Python<'_>, a: Option<&PyObject>, b: Option<&PyObject>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => std::ptr::eq(x.as_ptr(), y.as_ptr()),
        _ => false,
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Var>()?;
    Ok(())
}

#[pymethods]
impl Var {
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, pyo3::types::PyAny>) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).eq(other)
    }
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let r = self.deref_raw(py)?;
        r.bind(py).hash()
    }
    fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).is_truthy()
    }
    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let r = self.deref_raw(py)?;
        r.bind(py).str()?.extract()
    }
    fn __add__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__add__", (other,))?.unbind())
    }
    fn __radd__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__radd__", (other,))?.unbind())
    }
    fn __sub__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__sub__", (other,))?.unbind())
    }
    fn __rsub__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__rsub__", (other,))?.unbind())
    }
    fn __mul__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__mul__", (other,))?.unbind())
    }
    fn __rmul__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__rmul__", (other,))?.unbind())
    }
    fn __truediv__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__truediv__", (other,))?.unbind())
    }
    fn __floordiv__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__floordiv__", (other,))?.unbind())
    }
    fn __mod__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method1("__mod__", (other,))?.unbind())
    }
    fn __neg__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).call_method0("__neg__")?.unbind())
    }
    fn __lt__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).lt(&other)
    }
    fn __le__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).le(&other)
    }
    fn __gt__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).gt(&other)
    }
    fn __ge__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).ge(&other)
    }
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let r = self.deref_raw(py)?;
        r.bind(py).len()
    }
    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).try_iter()?.unbind().into_any())
    }
    fn __contains__(&self, py: Python<'_>, item: PyObject) -> PyResult<bool> {
        let r = self.deref_raw(py)?;
        r.bind(py).contains(&item)
    }
    fn __getitem__(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).get_item(&key)?.unbind())
    }
    fn __getattr__(&self, py: Python<'_>, name: String) -> PyResult<PyObject> {
        let r = self.deref_raw(py)?;
        Ok(r.bind(py).getattr(name.as_str())?.unbind())
    }
}

use crate::ifn::IFn;
use clojure_core_macros::implements;

#[implements(IFn)]
impl IFn for Var {
    fn invoke0(&self, py: Python<'_>) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[])
    }
    fn invoke1(&self, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0])
    }
    fn invoke2(&self, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1])
    }
    fn invoke3(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2])
    }
    fn invoke4(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3])
    }
    fn invoke5(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4])
    }
    fn invoke6(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5])
    }
    fn invoke7(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6])
    }
    fn invoke8(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7])
    }
    fn invoke9(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8])
    }
    fn invoke10(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9])
    }
    fn invoke11(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10])
    }
    fn invoke12(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11])
    }
    fn invoke13(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12])
    }
    fn invoke14(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13])
    }
    fn invoke15(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14])
    }
    fn invoke16(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15])
    }
    fn invoke17(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16])
    }
    fn invoke18(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17])
    }
    fn invoke19(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18])
    }
    fn invoke20(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject, a19: PyObject) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18, a19])
    }
    fn invoke_variadic(&self, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        let root = self.deref_raw(py)?;
        let items: Vec<PyObject> = (0..args.len()).map(|i| -> PyResult<_> {
            Ok(args.get_item(i)?.unbind())
        }).collect::<PyResult<_>>()?;
        crate::rt::invoke_n(py, root, &items)
    }
}
