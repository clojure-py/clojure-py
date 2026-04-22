//! Bytecode compiler: AST (read-forms) → Op stream.
//!
//! Pipeline: macroexpand → analyze → emit. See plan at
//! `~/.claude/plans/we-currently-have-a-floating-reef.md`.

pub mod op;
pub mod pool;
pub mod method;
pub mod analyzer;
pub mod emit;

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

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    method::register(py, m)?;
    Ok(())
}
