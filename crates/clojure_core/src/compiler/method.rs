//! Compiled artifacts: `CompiledMethod` (one per arity of a fn),
//! `FnTemplate` (blueprint pushed as a constant before `_make-closure`).

use crate::compiler::op::Op;
use crate::compiler::pool::FnPool;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// One arity's compiled code + frame layout.
#[derive(Clone)]
pub struct CompiledMethod {
    pub arity: u16,                 // fixed arity; for variadic, the required-arg count
    pub is_variadic: bool,
    pub local_slots: u16,           // params + lets + loop-bindings; frame allocates this many
    pub code: Vec<Op>,
}

/// Describes where each capture slot reads from in the enclosing frame at
/// `_make-closure` time.
#[derive(Clone, Debug)]
pub enum CaptureSource {
    /// Read `frame.locals[idx]` of the enclosing frame.
    Local(u16),
    /// Read `enclosing_fn.captures[idx]` — the enclosing fn already captured it.
    Capture(u16),
    /// Read the enclosing fn's "self" — emitted as `Op::LoadSelf`. Used so
    /// nested fns can capture an outer fn's name for self/mutual recursion.
    SelfRef,
}

/// Compiler output for a `fn*`. Pushed as a constant; `_make-closure`
/// consumes it along with the captures to produce a runtime `Fn`.
#[pyclass(module = "clojure._core", name = "FnTemplate", frozen)]
pub struct FnTemplate {
    pub name: Option<String>,
    pub current_ns: PyObject,
    pub capture_sources: Vec<CaptureSource>,
    pub methods: Vec<CompiledMethod>,
    pub variadic: Option<CompiledMethod>,
    pub pool: Arc<FnPool>,
}

#[pymethods]
impl FnTemplate {
    fn __repr__(&self) -> String {
        format!("#<FnTemplate {}>", self.name.as_deref().unwrap_or("anonymous"))
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FnTemplate>()?;
    Ok(())
}
