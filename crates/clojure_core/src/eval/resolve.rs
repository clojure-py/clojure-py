//! Symbol resolution. E1: locals only. E3: extend with namespace lookup.

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
    // E1: locals only. If the symbol is namespaced, raise — E3 handles.
    if s.ns.is_some() {
        return Err(errors::err(format!(
            "Unable to resolve namespaced symbol: {}/{} (ns resolution pending Phase E3)",
            s.ns.as_deref().unwrap(),
            s.name
        )));
    }
    if let Some(v) = env.lookup_local(&s.name, py) {
        return Ok(v);
    }
    Err(errors::err(format!(
        "Unable to resolve symbol: {} in this context",
        s.name
    )))
}
