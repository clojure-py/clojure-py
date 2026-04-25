//! Var — Clojure's namespace-scoped reference type.
//!
//! Each Var has a root value (or unbound), optional metadata, optional
//! validator, watches, and a dynamic flag. Dynamic binding stack + bound-fn*
//! come in later tasks (Phase 6). IFn impl + delegation dunders land in
//! Tasks 26-27.

use crate::exceptions::{IllegalArgumentException, IllegalStateException};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Lock-free optional slot used for root / meta / validator. Readers always
/// see a consistent snapshot via `load()`; writers install a new snapshot
/// via `store()` or `compare_and_swap()` (used by alter_var_root).
type Slot = ArcSwap<Option<PyObject>>;

fn empty_slot() -> Slot {
    ArcSwap::new(Arc::new(None))
}

#[pyclass(module = "clojure._core", name = "Var", frozen)]
pub struct Var {
    pub ns: PyObject,    // any module-like object (ClojureNamespace arrives in Phase 7)
    pub sym: PyObject,   // Symbol
    pub root: Slot,
    pub dynamic: AtomicBool,
    pub meta: Slot,
    pub watches: RwLock<Py<PyDict>>,  // dict container — mutated in place via set_item/del_item
    pub validator: Slot,
}

#[pymethods]
impl Var {
    #[new]
    pub fn new(py: Python<'_>, ns: PyObject, sym: PyObject) -> PyResult<Self> {
        Ok(Self {
            ns,
            sym,
            root: empty_slot(),
            dynamic: AtomicBool::new(false),
            meta: empty_slot(),
            watches: RwLock::new(PyDict::new(py).unbind()),
            validator: empty_slot(),
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
        let guard = this.root.load();
        match (**guard).as_ref() {
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
        self.root.load().is_some()
    }

    pub fn set_dynamic(&self, v: bool) {
        self.dynamic.store(v, Ordering::Release);
    }

    /// Set the root value directly. Runs the validator (if any) before installing.
    /// Watches fire after a successful change.
    fn bind_root(slf: Py<Self>, py: Python<'_>, value: PyObject) -> PyResult<()> {
        let this = slf.bind(py).get();
        this.validate(py, &value)?;
        let prev = this.root.swap(Arc::new(Some(value.clone_ref(py))));
        let old = (*prev).as_ref().map(|v| v.clone_ref(py));
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
    /// CAS loop via ArcSwap; validates the proposed value before installing; fires watches.
    #[pyo3(signature = (f, *args))]
    fn alter_root(
        slf: Py<Self>,
        py: Python<'_>,
        f: PyObject,
        args: Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        loop {
            // Snapshot the current Arc — we CAS against this exact pointer.
            let current_arc = this.root.load_full();
            let current_for_f = (*current_arc)
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

            let new_arc = Arc::new(Some(new_val.clone_ref(py)));
            // compare_and_swap returns the Arc that was in the slot at the
            // time of the CAS. If it's pointer-equal to `current_arc`, the
            // swap succeeded; otherwise someone else won the race.
            let witnessed = this.root.compare_and_swap(&current_arc, new_arc);
            if Arc::ptr_eq(&witnessed, &current_arc) {
                let old = (*current_arc).as_ref().map(|v| v.clone_ref(py));
                this.fire_watches(py, &slf, old, Some(new_val.clone_ref(py)))?;
                return Ok(new_val);
            }
            // Lost the race — retry.
        }
    }

    fn set_validator(&self, validator: Option<PyObject>) {
        self.validator.store(Arc::new(validator));
    }

    fn get_validator(&self, py: Python<'_>) -> Option<PyObject> {
        let g = self.validator.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py))
    }

    fn add_watch(&self, py: Python<'_>, key: PyObject, f: PyObject) -> PyResult<()> {
        let guard = self.watches.write();
        guard.bind(py).set_item(key, f)?;
        Ok(())
    }

    fn remove_watch(&self, py: Python<'_>, key: PyObject) -> PyResult<()> {
        let guard = self.watches.write();
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
        slf: Py<Self>,
        py: Python<'_>,
        args: Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let target = Var::deref_fast(&slf, py)?;
        let items: Vec<PyObject> = (0..args.len())
            .map(|i| -> PyResult<_> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        crate::rt::invoke_n(py, target, &items)
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> Option<PyObject> {
        let g = self.meta.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py))
    }

    pub fn set_meta(&self, meta: Option<PyObject>) {
        self.meta.store(Arc::new(meta));
    }
}

impl Var {
    /// Read the `:macro` bit from metadata. Returns `false` on any error
    /// (safe default during compile-time dispatch).
    pub fn is_macro(&self, py: Python<'_>) -> bool {
        let meta = match (*self.meta.load()).as_ref() {
            Some(m) => m.clone_ref(py),
            None => return false,
        };
        let kw = match crate::keyword::keyword(py, "macro", None) {
            Ok(k) => k.into_any(),
            Err(_) => return false,
        };
        let val = match crate::rt::get(py, meta, kw, py.None()) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if val.is_none(py) { return false; }
        if let Ok(b) = val.bind(py).cast::<pyo3::types::PyBool>() {
            return b.is_true();
        }
        true
    }

    /// Read the `:private` bit from metadata. Returns `false` on any error
    /// (safe default during compile-time dispatch).
    pub fn is_private(&self, py: Python<'_>) -> bool {
        let meta = match (*self.meta.load()).as_ref() {
            Some(m) => m.clone_ref(py),
            None => return false,
        };
        let kw = match crate::keyword::keyword(py, "private", None) {
            Ok(k) => k.into_any(),
            Err(_) => return false,
        };
        let val = match crate::rt::get(py, meta, kw, py.None()) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if val.is_none(py) { return false; }
        if let Ok(b) = val.bind(py).cast::<pyo3::types::PyBool>() {
            return b.is_true();
        }
        true
    }

    /// Tag this Var as a macro — `(alter-meta! v assoc :macro true)`. If
    /// meta is currently nil, installs `{:macro true}` as a fresh arraymap.
    /// Uses a CAS loop so concurrent set_macro_flag / set_meta stay safe.
    pub fn set_macro_flag(&self, py: Python<'_>) -> PyResult<()> {
        let kw = crate::keyword::keyword(py, "macro", None)?.into_any();
        let true_py: PyObject = pyo3::types::PyBool::new(py, true)
            .to_owned()
            .unbind()
            .into_any();
        loop {
            let current = self.meta.load_full();
            let new_meta: PyObject = match (*current).as_ref() {
                Some(m) => m
                    .bind(py)
                    .call_method1("assoc", (kw.clone_ref(py), true_py.clone_ref(py)))?
                    .unbind(),
                None => {
                    let tup = pyo3::types::PyTuple::new(py, &[kw.clone_ref(py), true_py.clone_ref(py)])?;
                    crate::collections::parraymap::array_map(py, tup)?
                }
            };
            let new_arc = Arc::new(Some(new_meta));
            let witnessed = self.meta.compare_and_swap(&current, new_arc);
            if Arc::ptr_eq(&witnessed, &current) {
                return Ok(());
            }
            // Lost race — retry with the newly-installed meta.
        }
    }

    /// Like `deref`, but callable from Rust code without a `Py<Self>`.
    /// Returns the root, or IllegalStateException if unbound. Does NOT
    /// consult the dynamic-binding stack — callers that need full `@v`
    /// semantics should use `deref_fast` instead.
    fn deref_raw(&self, py: Python<'_>) -> PyResult<PyObject> {
        match (**self.root.load()).as_ref() {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(crate::exceptions::IllegalStateException::new_err(format!(
                "Var {}/{} is unbound",
                self.ns_name(py)?,
                self.sym_name(py)?
            ))),
        }
    }

    /// Rust-side fast path equivalent to `Var.deref()` — checks the
    /// dynamic-binding stack for dynamic Vars, then falls through to
    /// the root. Bypasses `call_method0("deref")` so VM `Op::Deref`
    /// doesn't pay CPython attribute-lookup overhead per hit.
    pub fn deref_fast(slf: &Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        if this.dynamic.load(Ordering::Acquire) {
            // Dynamic path — consult the bound thread's stack.
            let key: Py<PyAny> = slf.clone_ref(py).into_any();
            if let Some(v) = crate::binding::lookup_binding(py, &key) {
                return Ok(v);
            }
        }
        this.deref_raw(py)
    }
    fn ns_name(&self, py: Python<'_>) -> PyResult<String> {
        // Anonymous Vars (created via `Var::create` for `with-local-vars`)
        // have ns=None; report as "--anon--" in diagnostic strings.
        if self.ns.is_none(py) {
            return Ok(String::from("--anon--"));
        }
        let n = self.ns.bind(py).getattr("__name__")?;
        n.extract()
    }
    fn sym_name(&self, py: Python<'_>) -> PyResult<String> {
        if self.sym.is_none(py) {
            return Ok(String::from("--anon--"));
        }
        // Symbol exposes `.name` as a getter from Task 8.
        let n = self.sym.bind(py).getattr("name")?;
        n.extract()
    }
    fn validate(&self, py: Python<'_>, v: &PyObject) -> PyResult<()> {
        let validator = {
            let g = self.validator.load();
            let opt: &Option<PyObject> = &g;
            opt.as_ref().map(|o| o.clone_ref(py))
        };
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

/// Create an anonymous, dynamic Var — used by `with-local-vars`. The var
/// has no namespace and no symbol; its root is unbound. All access happens
/// through the thread-binding stack.
#[pyfunction]
#[pyo3(name = "create_var")]
pub fn create_var(py: Python<'_>) -> PyResult<Var> {
    let v = Var::new(py, py.None(), py.None())?;
    v.set_dynamic(true);
    Ok(v)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Var>()?;
    m.add_function(wrap_pyfunction!(create_var, m)?)?;
    Ok(())
}

#[pymethods]
impl Var {
    /// Vars are compared and hashed by identity (matching vanilla JVM
    /// Clojure: `Var.equals` is `Object.equals`, `Var.hashCode` is
    /// `Object.hashCode`). Using root-value equality would break common
    /// uses — e.g. as keys in a `hash-map` passed to
    /// `push-thread-bindings`, and for `with-local-vars` on unbound vars.
    fn __eq__(slf: Py<Self>, py: Python<'_>, other: &Bound<'_, pyo3::types::PyAny>) -> PyResult<bool> {
        let lhs_ptr = slf.as_ptr() as usize;
        let rhs_ptr = other.as_ptr() as usize;
        Ok(lhs_ptr == rhs_ptr)
    }
    fn __hash__(slf: Py<Self>, _py: Python<'_>) -> PyResult<isize> {
        Ok(slf.as_ptr() as isize)
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

use crate::ideref::IDeref;
use crate::ifn::IFn;
use crate::imeta::IMeta;
use clojure_core_macros::implements;

#[implements(IMeta)]
impl IMeta for Var {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let self_ref: &Var = this.bind(py).get();
        let g = self_ref.meta.load();
        let opt: &Option<PyObject> = &g;
        Ok(opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        // Vars are mutable reference types — `with-meta` mutates in place
        // (matches JVM Clojure's behavior of clojure.lang.Var.alterMeta).
        let self_ref: &Var = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        self_ref.meta.store(std::sync::Arc::new(m));
        Ok(this.into_any())
    }
}

#[implements(IDeref)]
impl IDeref for Var {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let self_ref: &Var = this.bind(py).get();
        if self_ref.dynamic.load(Ordering::Acquire) {
            let key: Py<PyAny> = this.clone_ref(py).into_any();
            if let Some(v) = crate::binding::lookup_binding(py, &key) {
                return Ok(v);
            }
        }
        self_ref.deref_raw(py)
    }
}

#[implements(IFn)]
impl IFn for Var {
    fn invoke0(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[])
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0])
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1])
    }
    fn invoke3(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2])
    }
    fn invoke4(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3])
    }
    fn invoke5(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4])
    }
    fn invoke6(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5])
    }
    fn invoke7(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6])
    }
    fn invoke8(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7])
    }
    fn invoke9(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8])
    }
    fn invoke10(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9])
    }
    fn invoke11(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10])
    }
    fn invoke12(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11])
    }
    fn invoke13(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12])
    }
    fn invoke14(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13])
    }
    fn invoke15(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14])
    }
    fn invoke16(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15])
    }
    fn invoke17(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16])
    }
    fn invoke18(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17])
    }
    fn invoke19(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18])
    }
    fn invoke20(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject, a19: PyObject) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        crate::rt::invoke_n(py, target,&[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18, a19])
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        let target = Var::deref_fast(&this, py)?;
        let items: Vec<PyObject> = (0..args.len()).map(|i| -> PyResult<_> {
            Ok(args.get_item(i)?.unbind())
        }).collect::<PyResult<_>>()?;
        crate::rt::invoke_n(py, target, &items)
    }
}
