use pyo3::prelude::*;

#[pymodule]
fn clojure_core(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
