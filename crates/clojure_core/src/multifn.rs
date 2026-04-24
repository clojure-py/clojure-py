//! MultiFn — Clojure's multimethod reference type. Dispatches invocation
//! through a user-supplied `dispatch-fn` that computes a dispatch value
//! from the args, then looks up a method in the method table by
//! `isa?`-matching the dispatch value against registered keys.
//!
//! Matches JVM `clojure.lang.MultiFn`:
//! - `addMethod` / `removeMethod` / `preferMethod`
//! - hierarchy change invalidates the resolved-method cache
//! - ambiguous matches resolved via the prefer-table, else raise
//! - fall back to the default method (defaults to `:default`)

use crate::exceptions::{IllegalArgumentException, IllegalStateException};
use crate::ifn::IFn;
use arc_swap::ArcSwap;
use clojure_core_macros::implements;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "MultiFn", frozen)]
pub struct MultiFn {
    pub name: String,
    pub dispatch_fn: PyObject,
    pub default_val: PyObject,
    /// The Var holding the global hierarchy. Its root value is the
    /// hierarchy map directly; updates go through `alter-var-root`.
    pub hierarchy_var: PyObject,
    /// Registered methods — dispatch-val → method fn.
    pub method_table: Py<PyDict>,
    /// Preference table — dispatch-val-A → list of dispatch-val-Bs that
    /// A is preferred over (for ambiguity resolution).
    pub prefer_table: Py<PyDict>,
    /// Resolved-method cache — dispatch-val → method. Invalidated when
    /// the hierarchy map changes identity.
    pub method_cache: Mutex<Py<PyDict>>,
    /// Last-seen hierarchy (by object identity) that we cached against.
    pub cached_hierarchy: ArcSwap<Option<PyObject>>,
}

impl MultiFn {
    fn new_inner(
        py: Python<'_>,
        name: String,
        dispatch_fn: PyObject,
        default_val: PyObject,
        hierarchy_var: PyObject,
    ) -> PyResult<Self> {
        Ok(Self {
            name,
            dispatch_fn,
            default_val,
            hierarchy_var,
            method_table: PyDict::new(py).unbind(),
            prefer_table: PyDict::new(py).unbind(),
            method_cache: Mutex::new(PyDict::new(py).unbind()),
            cached_hierarchy: ArcSwap::new(Arc::new(None)),
        })
    }

    /// Deref the hierarchy Var to get the current hierarchy map. The
    /// Var's root value *is* the hierarchy (vanilla layout).
    fn current_hierarchy(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.hierarchy_var.bind(py).call_method0("deref")?.unbind())
    }

    /// Test invariant: hierarchy hasn't changed since we last cached. Uses
    /// pointer identity — swapping a new map in makes the pointer differ
    /// even if structurally equal.
    fn hierarchy_changed(&self, py: Python<'_>, current: &PyObject) -> bool {
        let guard = self.cached_hierarchy.load();
        let opt: &Option<PyObject> = &guard;
        match opt {
            Some(prev) => prev.as_ptr() != current.as_ptr(),
            None => true,
        }
    }

    fn reset_cache(&self, py: Python<'_>, current: PyObject) {
        let dict = PyDict::new(py).unbind();
        *self.method_cache.lock() = dict;
        self.cached_hierarchy.store(Arc::new(Some(current)));
    }

    /// Invalidate the method cache (call after method-table or prefer-table
    /// mutations).
    fn invalidate_cache(&self, py: Python<'_>) {
        let dict = PyDict::new(py).unbind();
        *self.method_cache.lock() = dict;
        self.cached_hierarchy.store(Arc::new(None));
    }

    /// Dispatch `args` through the multimethod. Computes dispatch-val,
    /// resolves the method, invokes it.
    fn dispatch(this: Py<Self>, py: Python<'_>, args: Vec<PyObject>) -> PyResult<PyObject> {
        let self_ref = this.bind(py).get();
        // 1. dispatch value
        let dv = crate::rt::invoke_n(py, self_ref.dispatch_fn.clone_ref(py), &args)?;
        // 2. resolve (cached)
        let method = self_ref.find_and_cache_method(py, dv.clone_ref(py))?;
        // 3. invoke
        crate::rt::invoke_n(py, method, &args)
    }

    fn find_and_cache_method(&self, py: Python<'_>, dv: PyObject) -> PyResult<PyObject> {
        let h = self.current_hierarchy(py)?;
        if self.hierarchy_changed(py, &h) {
            self.reset_cache(py, h.clone_ref(py));
        }
        // Cache hit?
        {
            let cache = self.method_cache.lock();
            if let Ok(Some(existing)) = cache.bind(py).get_item(&dv) {
                return Ok(existing.unbind());
            }
        }
        // Miss — resolve, cache.
        let method = self.resolve_method(py, &dv, &h)?;
        let cache = self.method_cache.lock();
        cache.bind(py).set_item(&dv, &method)?;
        Ok(method)
    }

    /// Walk method-table looking for the best match for `dv`.
    fn resolve_method(
        &self,
        py: Python<'_>,
        dv: &PyObject,
        h: &PyObject,
    ) -> PyResult<PyObject> {
        // Exact match first.
        let mt = self.method_table.bind(py);
        if let Ok(Some(m)) = mt.get_item(dv) {
            return Ok(m.unbind());
        }
        // isa?-match across all keys.
        let isa_fn = isa_fn(py)?;
        let mut matches: Vec<(PyObject, PyObject)> = Vec::new();
        for (k, v) in mt.iter() {
            let hit = isa_fn
                .bind(py)
                .call1((h.clone_ref(py), dv.clone_ref(py), k.clone().unbind()))?
                .is_truthy()?;
            if hit {
                matches.push((k.unbind(), v.unbind()));
            }
        }
        if matches.len() == 1 {
            return Ok(matches.into_iter().next().unwrap().1);
        }
        if matches.is_empty() {
            // Fall back to default.
            if let Ok(Some(m)) = mt.get_item(&self.default_val) {
                return Ok(m.unbind());
            }
            return Err(IllegalArgumentException::new_err(format!(
                "No method in multimethod '{}' for dispatch value: {}",
                self.name,
                dv.bind(py).str()?.to_string_lossy()
            )));
        }
        // Multiple matches → prefer-table.
        let best = self.pick_preferred(py, &matches, h)?;
        match best {
            Some(m) => Ok(m),
            None => {
                // Build a list of the clashing keys for the error message.
                let keys: Vec<String> = matches
                    .iter()
                    .map(|(k, _)| k.bind(py).str().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
                    .collect();
                Err(IllegalArgumentException::new_err(format!(
                    "Multiple methods in multimethod '{}' match dispatch value: {} -> {} and {}, and neither is preferred",
                    self.name,
                    dv.bind(py).str()?.to_string_lossy(),
                    keys.first().cloned().unwrap_or_default(),
                    keys.get(1).cloned().unwrap_or_default()
                )))
            }
        }
    }

    /// Among `matches`, return the unique key that's preferred over all
    /// the others (either directly in `prefer_table` or via hierarchy
    /// ancestor relationships). None if ambiguous.
    fn pick_preferred(
        &self,
        py: Python<'_>,
        matches: &[(PyObject, PyObject)],
        h: &PyObject,
    ) -> PyResult<Option<PyObject>> {
        let mut best: Option<&(PyObject, PyObject)> = None;
        for m in matches {
            let mut beats_all = true;
            for other in matches {
                if std::ptr::eq(m, other) {
                    continue;
                }
                if !self.prefers(py, &m.0, &other.0, h)? {
                    beats_all = false;
                    break;
                }
            }
            if beats_all {
                if best.is_some() {
                    return Ok(None); // more than one candidate dominates
                }
                best = Some(m);
            }
        }
        Ok(best.map(|(_, v)| v.clone_ref(py)))
    }

    /// Vanilla semantics (`clojure.lang.MultiFn.prefers`):
    ///   1. Direct preference: a's prefer-list contains b.
    ///   2. a is preferred over any parent of b (recursively).
    ///   3. Any parent of a is preferred over b (recursively).
    fn prefers(
        &self,
        py: Python<'_>,
        a: &PyObject,
        b: &PyObject,
        h: &PyObject,
    ) -> PyResult<bool> {
        let pt = self.prefer_table.bind(py);
        if let Ok(Some(list)) = pt.get_item(a) {
            let mut cur = crate::rt::seq(py, list.unbind())?;
            while !cur.is_none(py) {
                let head = crate::rt::first(py, cur.clone_ref(py))?;
                if head.bind(py).eq(b)? {
                    return Ok(true);
                }
                cur = crate::rt::next_(py, cur)?;
            }
        }
        // Walk b's parents — if a is preferred over any of them, a wins.
        let bp = parents_fn(py)?
            .bind(py)
            .call1((h.clone_ref(py), b.clone_ref(py)))?
            .unbind();
        let mut cur = crate::rt::seq(py, bp)?;
        while !cur.is_none(py) {
            let head = crate::rt::first(py, cur.clone_ref(py))?;
            if self.prefers(py, a, &head, h)? {
                return Ok(true);
            }
            cur = crate::rt::next_(py, cur)?;
        }
        // Walk a's parents — if any of them is preferred over b, a wins.
        let ap = parents_fn(py)?
            .bind(py)
            .call1((h.clone_ref(py), a.clone_ref(py)))?
            .unbind();
        let mut cur = crate::rt::seq(py, ap)?;
        while !cur.is_none(py) {
            let head = crate::rt::first(py, cur.clone_ref(py))?;
            if self.prefers(py, &head, b, h)? {
                return Ok(true);
            }
            cur = crate::rt::next_(py, cur)?;
        }
        Ok(false)
    }
}

// --- Python-facing methods ---

#[pymethods]
impl MultiFn {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[pyo3(name = "addMethod")]
    fn add_method(this: Py<Self>, py: Python<'_>, dv: PyObject, method: PyObject) -> PyResult<Py<Self>> {
        let s = this.bind(py).get();
        s.method_table.bind(py).set_item(&dv, &method)?;
        s.invalidate_cache(py);
        Ok(this)
    }

    #[pyo3(name = "removeMethod")]
    fn remove_method(this: Py<Self>, py: Python<'_>, dv: PyObject) -> PyResult<Py<Self>> {
        let s = this.bind(py).get();
        // Ignore KeyError — removing an absent method is a no-op.
        let _ = s.method_table.bind(py).del_item(&dv);
        s.invalidate_cache(py);
        Ok(this)
    }

    #[pyo3(name = "removeAllMethods")]
    fn remove_all_methods(this: Py<Self>, py: Python<'_>) -> PyResult<Py<Self>> {
        let s = this.bind(py).get();
        s.method_table.bind(py).call_method0("clear")?;
        s.invalidate_cache(py);
        Ok(this)
    }

    #[pyo3(name = "preferMethod")]
    fn prefer_method(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<Py<Self>> {
        let s = this.bind(py).get();
        // Disallow cycles: if b is already preferred over a, error.
        let h = s.current_hierarchy(py)?;
        if s.prefers(py, &b, &a, &h)? {
            return Err(IllegalStateException::new_err(format!(
                "Preference conflict in multimethod '{}': {} is already preferred to {}",
                s.name,
                b.bind(py).str()?.to_string_lossy(),
                a.bind(py).str()?.to_string_lossy()
            )));
        }
        let pt = s.prefer_table.bind(py);
        // Build/append: pt[a] = conj (pt[a] or []) b
        let existing: PyObject = match pt.get_item(&a) {
            Ok(Some(v)) => v.unbind(),
            _ => {
                let empty = crate::collections::pvector::vector(py, PyTuple::empty(py))?;
                empty.into_any()
            }
        };
        let appended = crate::rt::conj(py, existing, b)?;
        pt.set_item(&a, appended)?;
        s.invalidate_cache(py);
        Ok(this)
    }

    #[pyo3(name = "methodTable")]
    fn method_table_py(&self, py: Python<'_>) -> PyResult<PyObject> {
        dict_to_persistent_map(py, self.method_table.bind(py))
    }

    #[pyo3(name = "preferTable")]
    fn prefer_table_py(&self, py: Python<'_>) -> PyResult<PyObject> {
        // The values in prefer_table are vectors of preferred-over dispatch
        // values; vanilla's `prefers` fn exposes them as sets. Convert on
        // the way out.
        let d = self.prefer_table.bind(py);
        let mut m = crate::collections::phashmap::PersistentHashMap::new_empty();
        for (k, v) in d.iter() {
            let set = vector_to_hash_set(py, v.unbind())?;
            m = m.assoc_internal(py, k.unbind(), set)?;
        }
        Ok(Py::new(py, m)?.into_any())
    }

    #[pyo3(name = "getMethod")]
    fn get_method(this: Py<Self>, py: Python<'_>, dv: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        match s.find_and_cache_method(py, dv) {
            Ok(m) => Ok(m),
            Err(_) => Ok(py.None()),
        }
    }

    fn __repr__(&self) -> String {
        format!("#<MultiFn {}>", self.name)
    }
}

// --- IFn impl: route every arity through `dispatch`. ---

#[implements(IFn)]
impl IFn for MultiFn {
    fn invoke0(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![])
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a])
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b])
    }
    fn invoke3(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject, c: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b, c])
    }
    fn invoke4(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject, c: PyObject, d: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b, c, d])
    }
    fn invoke5(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject, c: PyObject, d: PyObject, e: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b, c, d, e])
    }
    fn invoke6(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject, c: PyObject, d: PyObject, e: PyObject, f: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b, c, d, e, f])
    }
    fn invoke7(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject, c: PyObject, d: PyObject, e: PyObject, f: PyObject, g: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b, c, d, e, f, g])
    }
    fn invoke8(this: Py<Self>, py: Python<'_>, a: PyObject, b: PyObject, c: PyObject, d: PyObject, e: PyObject, f: PyObject, g: PyObject, h: PyObject) -> PyResult<PyObject> {
        MultiFn::dispatch(this, py, vec![a, b, c, d, e, f, g, h])
    }
}

// --- Module-level create fn, cached isa? Var ---

static ISA_FN: OnceCell<PyObject> = OnceCell::new();
static PARENTS_FN: OnceCell<PyObject> = OnceCell::new();

fn resolve_core_fn(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    let core = modules.get_item("clojure.core")?;
    let var = core.getattr(name)?;
    Ok(var.call_method0("deref")?.unbind())
}

fn isa_fn(py: Python<'_>) -> PyResult<&'static PyObject> {
    if let Some(f) = ISA_FN.get() {
        return Ok(f);
    }
    let f = resolve_core_fn(py, "isa?")?;
    let _ = ISA_FN.set(f);
    Ok(ISA_FN.get().unwrap())
}

fn parents_fn(py: Python<'_>) -> PyResult<&'static PyObject> {
    if let Some(f) = PARENTS_FN.get() {
        return Ok(f);
    }
    let f = resolve_core_fn(py, "parents")?;
    let _ = PARENTS_FN.set(f);
    Ok(PARENTS_FN.get().unwrap())
}

#[pyfunction]
#[pyo3(name = "MultiFn_create")]
pub fn py_multifn_create(
    py: Python<'_>,
    name: String,
    dispatch_fn: PyObject,
    default_val: PyObject,
    hierarchy_var: PyObject,
) -> PyResult<MultiFn> {
    MultiFn::new_inner(py, name, dispatch_fn, default_val, hierarchy_var)
}

/// Convert a PyDict (our internal method/prefer-table storage) to a
/// PersistentHashMap so Clojure-level code sees an IPersistentMap.
fn dict_to_persistent_map(py: Python<'_>, d: &Bound<'_, PyDict>) -> PyResult<PyObject> {
    let mut m = crate::collections::phashmap::PersistentHashMap::new_empty();
    for (k, v) in d.iter() {
        m = m.assoc_internal(py, k.unbind(), v.unbind())?;
    }
    Ok(Py::new(py, m)?.into_any())
}

/// Convert a Clojure vector (our prefer-table value) to a PersistentHashSet
/// — vanilla's `prefers` fn exposes each value as a set of dispatch vals.
fn vector_to_hash_set(py: Python<'_>, v: PyObject) -> PyResult<PyObject> {
    let mut s = crate::collections::phashset::PersistentHashSet::new_empty(py)?;
    let mut cur = crate::rt::seq(py, v)?;
    while !cur.is_none(py) {
        let head = crate::rt::first(py, cur.clone_ref(py))?;
        s = s.conj_internal(py, head)?;
        cur = crate::rt::next_(py, cur)?;
    }
    Ok(Py::new(py, s)?.into_any())
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MultiFn>()?;
    m.add_function(wrap_pyfunction!(py_multifn_create, m)?)?;
    Ok(())
}
