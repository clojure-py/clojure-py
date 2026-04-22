//! Symbol resolution. E3: locals, then current-ns, then clojure.core fallback.
//! Qualified symbols resolve in their specified namespace only. Vars are
//! derefed transparently.

use crate::eval::env::Env;
use crate::eval::errors;
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

pub fn resolve_symbol(py: Python<'_>, sym: PyObject, env: &Env) -> PyResult<PyObject> {
    let b = sym.bind(py);
    let sym_ref = b.downcast::<Symbol>().map_err(|_| {
        errors::err(format!("resolve_symbol called on non-Symbol: {:?}", b.repr()))
    })?;
    let s = sym_ref.get();

    // Qualified symbol: look up in specified ns only.
    if let Some(ns_name) = s.ns.as_deref() {
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let target_ns = modules.get_item(ns_name).map_err(|_| {
            errors::err(format!("No namespace: {}", ns_name))
        })?;
        let attr = target_ns.getattr(s.name.as_ref()).map_err(|_| {
            errors::err(format!("Unable to resolve: {}/{}", ns_name, s.name))
        })?;
        return deref_if_var(attr);
    }

    // Unqualified: locals, current-ns, clojure.core.
    if let Some(v) = env.lookup_local(&s.name, py) {
        return Ok(v);
    }
    let current_ns = env.current_ns.bind(py);
    if let Ok(attr) = current_ns.getattr(s.name.as_ref()) {
        return deref_if_var(attr);
    }
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if let Ok(core_ns) = modules.get_item("clojure.core") {
        if let Ok(attr) = core_ns.getattr(s.name.as_ref()) {
            return deref_if_var(attr);
        }
    }
    Err(errors::err(format!("Unable to resolve symbol: {} in this context", s.name)))
}

fn deref_if_var<'py>(attr: Bound<'py, PyAny>) -> PyResult<PyObject> {
    if let Ok(var) = attr.clone().downcast::<crate::var::Var>() {
        let d = var.call_method0("deref")?;
        return Ok(d.unbind());
    }
    Ok(attr.unbind())
}
