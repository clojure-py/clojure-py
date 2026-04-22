//! Symbol resolution. E2: locals, then namespace-qualified or current-ns attribute lookup,
//! derefing Vars transparently.

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

    // 1. Locals first (unqualified symbols only).
    if s.ns.is_none() {
        if let Some(v) = env.lookup_local(&s.name, py) {
            return Ok(v);
        }
    }

    // 2. Namespace lookup.
    // If qualified, look up the target namespace; else use current_ns.
    let target_ns = match s.ns.as_deref() {
        Some(ns_name) => {
            // Find the namespace module from sys.modules.
            let sys = py.import("sys")?;
            let modules = sys.getattr("modules")?;
            match modules.get_item(ns_name) {
                Ok(m) => m,
                Err(_) => {
                    return Err(errors::err(format!(
                        "No namespace: {} found while resolving {}/{}",
                        ns_name, ns_name, s.name
                    )));
                }
            }
        }
        None => env.current_ns.bind(py).clone(),
    };

    // Look up the symbol name as an attribute of the namespace.
    let name_str = s.name.as_ref();
    match target_ns.getattr(name_str) {
        Ok(attr) => {
            // If it's a Var, deref it. If it's a plain Python callable, return as-is.
            let attr_b = attr.clone();
            if let Ok(var) = attr_b.downcast::<crate::var::Var>() {
                // Call .deref() via pymethod.
                let deref_result = var.call_method0("deref")?;
                return Ok(deref_result.unbind());
            }
            Ok(attr.unbind())
        }
        Err(_) => Err(errors::err(format!(
            "Unable to resolve symbol: {} in this context",
            if let Some(ns) = s.ns.as_deref() {
                format!("{}/{}", ns, s.name)
            } else {
                s.name.to_string()
            }
        ))),
    }
}
