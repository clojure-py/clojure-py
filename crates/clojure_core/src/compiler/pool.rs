//! Per-fn constant / var pools and the builder used during compilation.
//!
//! A single `FnPool` is shared across all arity methods of a compiled fn
//! (via `Arc<FnPool>`). Nested `FnTemplate` values are interned as ordinary
//! constants — the bytecode references them via `PushConst`.

use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// The finalized pool — immutable, shared across methods.
pub struct FnPool {
    pub constants: Vec<PyObject>,
    pub vars: Vec<Py<crate::var::Var>>,
}

/// Mutable builder used during compilation. Resolves to an `Arc<FnPool>` at
/// the end. Nil is reserved at `constants[0]` so `PushConst(0)` always pushes
/// None.
pub struct PoolBuilder {
    constants: Vec<PyObject>,
    vars: Vec<Py<crate::var::Var>>,
}

impl PoolBuilder {
    pub fn new(py: Python<'_>) -> Self {
        let mut pb = Self {
            constants: Vec::new(),
            vars: Vec::new(),
        };
        // constants[0] = nil.
        pb.constants.push(py.None());
        pb
    }

    /// Append a constant; returns its pool index.
    ///
    /// No dedup in this first pass — duplicate constants are cheap
    /// (PyObject refs), and dedup requires hashing/equality over arbitrary
    /// Py values. Can be added later if profiling shows pool bloat.
    pub fn intern_const(&mut self, value: PyObject) -> u16 {
        let ix = self.constants.len();
        self.constants.push(value);
        ix as u16
    }

    pub fn nil_ix(&self) -> u16 { 0 }

    /// Pool index for a Var. Dedups by pointer identity (two lookups of the
    /// same qualified symbol resolve to the same Var, so pointer-equality is
    /// sufficient and avoids Py-level equality calls during compilation).
    pub fn intern_var(&mut self, py: Python<'_>, var: Py<crate::var::Var>) -> u16 {
        for (i, existing) in self.vars.iter().enumerate() {
            if existing.as_ptr() == var.as_ptr() {
                let _ = py;  // suppress unused warning; kept for future equality-dedup path
                return i as u16;
            }
        }
        let ix = self.vars.len();
        self.vars.push(var);
        ix as u16
    }

    pub fn finish(self) -> Arc<FnPool> {
        Arc::new(FnPool {
            constants: self.constants,
            vars: self.vars,
        })
    }
}
