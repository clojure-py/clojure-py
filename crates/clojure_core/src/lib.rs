use pyo3::prelude::*;

mod exceptions;
mod symbol;

pub use clojure_core_macros::{implements, protocol};
pub use exceptions::{ArityException, IllegalArgumentException, IllegalStateException};
pub use symbol::Symbol;

#[pymodule]
fn _core(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    exceptions::register(py, m)?;
    symbol::register(py, m)?;
    Ok(())
}
