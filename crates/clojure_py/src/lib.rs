//! Python interop bridge for `clojure_rt`.
//!
//! This crate is the only one in the workspace that knows about Python.
//! `clojure_rt` stays language-agnostic; `clojure_core` registers
//! `implements!` blocks for the `PyObject` primitive marker against
//! `clojure_rt` protocols, so a Python list, dict, or string passed in
//! as a `Value(TYPE_PYOBJECT)` dispatches through the same per-type
//! table as native Clojure types.
//!
//! Reference semantics for `Value(TYPE_PYOBJECT)` are currently
//! **borrowed**: the caller (a Python frame, or a PyO3 fn arg) keeps
//! the underlying object alive across the dispatch. Sound for
//! synchronous protocol calls. The day a Clojure collection wants to
//! *store* a Python value, this becomes unsound and we'll add a
//! per-primitive drop hook in `clojure_rt::rc`.

pub mod api;
pub mod counted;
pub mod exception;

use pyo3::prelude::*;

/// `clojure._core` extension module entry point. Maturin links the
/// `cdylib` produced by this crate; this fn is what `import clojure._core`
/// runs.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    clojure_rt::init();
    m.add_function(wrap_pyfunction!(api::count, m)?)?;
    Ok(())
}
