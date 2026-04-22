//! ClojureNamespace — a Python subclass of `types.ModuleType` created at init
//! time via `type()`. We can't use `#[pyclass(extends = PyModule)]` because
//! pyo3 0.28 forbids subclassing PyModule at the Rust level.
//!
//! Clojure namespaces are flat — `clojure.core.async` is not a child of
//! `clojure.core` at the Clojure level. But because each namespace lives in
//! `sys.modules` and Python's import machinery walks dot prefixes, we
//! auto-create bare `types.ModuleType` placeholders for each prefix that
//! doesn't already exist. Those placeholders are NOT Clojure namespaces —
//! they have no metadata dunders and `find-ns` skips them.

use crate::symbol::Symbol;
use once_cell::sync::OnceCell;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

static CLJ_NS_CLASS: OnceCell<PyObject> = OnceCell::new();

/// Build the `ClojureNamespace` class at init time: a Python subclass of
/// `types.ModuleType`. Called from `register` below.
fn build_clojure_namespace_class(py: Python<'_>) -> PyResult<PyObject> {
    let types_mod = py.import("types")?;
    let module_type = types_mod.getattr("ModuleType")?;
    let builtins = py.import("builtins")?;
    let type_metaclass = builtins.getattr("type")?;

    // type("ClojureNamespace", (types.ModuleType,), {"__module__": "clojure._core"})
    let bases = PyTuple::new(py, &[module_type])?;
    let dict = PyDict::new(py);
    dict.set_item("__module__", "clojure._core")?;
    let cls = type_metaclass.call1(("ClojureNamespace", bases, dict))?;
    Ok(cls.unbind())
}

fn clojure_namespace_class(py: Python<'_>) -> PyResult<&'static PyObject> {
    let _ = py;
    CLJ_NS_CLASS.get().ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "ClojureNamespace class not initialized — did the pymodule init run?",
        )
    })
}

/// Populate the Clojure-namespace metadata dunders on a freshly-created
/// `ClojureNamespace` instance.
fn populate_dunders(py: Python<'_>, module: &Bound<'_, PyAny>, full_name: &str) -> PyResult<()> {
    let dict = module.getattr("__dict__")?;
    let dict = dict.downcast::<PyDict>()?;
    let sym = Py::new(py, Symbol::new(None, Arc::from(full_name)))?;
    dict.set_item("__clj_ns__", sym)?;
    dict.set_item("__clj_ns_meta__", py.None())?;
    dict.set_item("__clj_aliases__", PyDict::new(py))?;
    dict.set_item("__clj_refers__", PyDict::new(py))?;
    dict.set_item("__clj_imports__", PyDict::new(py))?;
    Ok(())
}

fn full_dotted_name(sym: &Symbol) -> String {
    match sym.ns.as_deref() {
        Some(ns) => format!("{}.{}", ns, sym.name),
        None => sym.name.to_string(),
    }
}

fn is_clojure_namespace(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let cls = clojure_namespace_class(py)?.bind(py);
    obj.is_instance(cls)
}

#[pyfunction]
pub fn create_ns(py: Python<'_>, sym: Py<Symbol>) -> PyResult<PyObject> {
    let dotted_name = {
        let s = sym.bind(py).get();
        full_dotted_name(s)
    };

    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    let types_mod = py.import("types")?;
    let module_type = types_mod.getattr("ModuleType")?;
    let clj_ns_cls = clojure_namespace_class(py)?.bind(py);

    // Placeholder children that we'll re-wire onto the new ClojureNamespace.
    let mut saved_children: Vec<(String, PyObject)> = Vec::new();

    // Case 2 / 3 detection at the terminal name.
    if let Ok(existing) = modules.get_item(&dotted_name) {
        if is_clojure_namespace(py, &existing)? {
            return Ok(existing.unbind());  // Case 2: idempotent
        }
        // Case 3: snapshot module-valued attributes on the placeholder before dropping it.
        // Every auto-parented child lives on the placeholder's __dict__ as a setattr,
        // and those same children are ALSO in sys.modules under their own dotted names.
        // We only need to preserve the attribute wiring — the sys.modules entries are
        // untouched by the delete-one-key operation below.
        let existing_dict = existing.getattr("__dict__")?;
        let existing_dict = existing_dict.downcast::<PyDict>()?;
        for (k, v) in existing_dict.iter() {
            // Skip Python's own module machinery attributes (they'll be recreated by
            // ModuleType.__init__ when we construct the new namespace).
            let key: String = k.extract()?;
            if key.starts_with("__") && key.ends_with("__") {
                continue;
            }
            saved_children.push((key, v.unbind()));
        }
        modules.del_item(&dotted_name)?;  // Case 3: replace placeholder
    }

    // Build the parent chain.
    let parts: Vec<&str> = dotted_name.split('.').collect();
    let mut running = String::new();
    let mut parent: Option<Bound<'_, PyAny>> = None;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            running.push('.');
        }
        running.push_str(part);
        let is_terminal = i == parts.len() - 1;

        let current = if is_terminal {
            // ClojureNamespace(running_name)
            let m = clj_ns_cls.call1((running.as_str(),))?;
            populate_dunders(py, &m, &running)?;
            // Re-apply any child attributes from a replaced placeholder.
            for (child_name, child_obj) in saved_children.drain(..) {
                m.setattr(child_name.as_str(), child_obj)?;
            }
            modules.set_item(&running, &m)?;
            m
        } else if let Ok(existing) = modules.get_item(&running) {
            existing
        } else {
            // Bare ModuleType placeholder.
            let m = module_type.call1((running.as_str(),))?;
            modules.set_item(&running, &m)?;
            m
        };

        if let Some(p) = parent {
            p.setattr(part.to_string().as_str(), &current)?;
        }
        parent = Some(current);
    }

    Ok(parent.unwrap().unbind())
}

#[pyfunction]
pub fn find_ns(py: Python<'_>, sym: Py<Symbol>) -> PyResult<Option<PyObject>> {
    let dotted_name = {
        let s = sym.bind(py).get();
        full_dotted_name(s)
    };
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    match modules.get_item(&dotted_name) {
        Ok(m) if is_clojure_namespace(py, &m)? => Ok(Some(m.unbind())),
        _ => Ok(None),
    }
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Build the class once and store in OnceCell + on the module.
    let cls = build_clojure_namespace_class(py)?;
    m.add("ClojureNamespace", cls.clone_ref(py))?;
    let _ = CLJ_NS_CLASS.set(cls);

    m.add_function(wrap_pyfunction!(create_ns, m)?)?;
    m.add_function(wrap_pyfunction!(find_ns, m)?)?;
    Ok(())
}
