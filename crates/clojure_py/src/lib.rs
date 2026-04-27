//! Python interop bridge for `clojure_rt`.
//!
//! `clojure_rt` is the language-agnostic runtime; `clojure_py` is the
//! only crate that knows about Python. Each Python class is interned
//! lazily as a `clojure_rt` `TypeId` (see `intern`); ABCs from
//! `collections.abc` act as inheritance metadata so a single
//! `Counted for Sized` impl transparently covers `list`, `dict`,
//! `str`, and any user class with `__len__`.

pub mod abcs;
pub mod api;
pub mod counted;
pub mod exception;
pub mod intern;

use std::sync::Once;

use pyo3::prelude::*;

static INIT: Once = Once::new();

/// Initialize the Python-interop layer. Idempotent; safe to call from
/// both the `#[pymodule]` entry and from cargo tests. Acquires the GIL
/// internally for the ABC bootstrap.
pub fn init() {
    INIT.call_once(|| {
        clojure_rt::init();
        intern::install_foreign_resolver();
        Python::attach(|py| {
            abcs::init(py);
        });
    });
}

/// `clojure._core` extension module entry point. Maturin links the
/// `cdylib` produced by this crate; this fn runs on `import clojure._core`.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    init();
    m.add_function(wrap_pyfunction!(api::count, m)?)?;
    Ok(())
}
