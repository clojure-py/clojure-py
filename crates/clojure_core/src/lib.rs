use pyo3::prelude::*;

pub use clojure_core_macros::{implements, protocol};

#[pymodule]
fn _core(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
