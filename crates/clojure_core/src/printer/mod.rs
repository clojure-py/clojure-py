//! Printer — pr, pr_str.

pub mod print;

use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyfunction]
#[pyo3(name = "pr_str")]
pub fn pr_str_py(py: Python<'_>, x: PyObject) -> PyResult<String> {
    print::pr_str(py, x)
}

#[pyfunction]
#[pyo3(name = "print_str")]
pub fn print_str_py(py: Python<'_>, x: PyObject) -> PyResult<String> {
    print::print_str(py, x)
}

#[pyfunction]
#[pyo3(name = "pr")]
pub fn pr_py(py: Python<'_>, x: PyObject) -> PyResult<()> {
    let s = print::pr_str(py, x)?;
    let builtins = py.import("builtins")?;
    let print_fn = builtins.getattr("print")?;
    print_fn.call1((s,))?;
    Ok(())
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pr_str_py, m)?)?;
    m.add_function(wrap_pyfunction!(print_str_py, m)?)?;
    m.add_function(wrap_pyfunction!(pr_py, m)?)?;
    Ok(())
}
