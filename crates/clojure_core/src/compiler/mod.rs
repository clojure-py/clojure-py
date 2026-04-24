//! Bytecode compiler: AST (read-forms) → Op stream.
//!
//! Pipeline: macroexpand → analyze → emit.

pub mod op;
pub mod pool;
pub mod method;
pub mod analyzer;
pub mod emit;
pub mod letfn_cell;

use crate::compiler::emit::Compiler;
use crate::compiler::method::CompiledMethod;
use crate::compiler::pool::FnPool;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Compile a top-level form to a 0-arity `CompiledMethod` + its pool.
/// `vm::run(method, pool, &[], &[])` yields the form's value.
pub fn compile_top_level(
    py: Python<'_>,
    form: PyObject,
    current_ns: PyObject,
) -> PyResult<(CompiledMethod, Arc<FnPool>)> {
    let mut c = Compiler::new(py, current_ns);
    c.compile_form(py, form)?;
    Ok(c.finish_top_level())
}

/// Single-step macroexpansion. If `form` is a list whose head resolves to
/// a `:macro`-tagged Var, invokes the macro (passing `&form` and an empty
/// `&env`) and returns the expanded form. Otherwise returns `form` unchanged.
///
/// Used by `clojure.core/macroexpand-1`. Runs in a fresh compile ctx with
/// no locals — the macro is invoked outside of any enclosing fn.
pub fn macroexpand_1(
    py: Python<'_>,
    form: PyObject,
    current_ns: PyObject,
) -> PyResult<PyObject> {
    // Only list forms can expand; everything else is identity.
    let b = form.bind(py);
    let Ok(pl) = b.cast::<crate::collections::plist::PersistentList>() else {
        return Ok(form);
    };
    let head = pl.get().head.clone_ref(py);
    let mut c = Compiler::new(py, current_ns);
    match c.try_macroexpand_user_public(py, &form, &head)? {
        Some(expanded) => Ok(expanded),
        None => Ok(form),
    }
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    method::register(py, m)?;
    Ok(())
}
