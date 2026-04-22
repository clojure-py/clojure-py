//! Clojure reader — recursive-descent parser.

pub mod errors;
pub mod lexer;
pub mod number;
pub mod source;
pub mod string;

use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

// Test-only pyfunctions so Python can exercise the primitives before read_string lands (R2).
#[pyfunction]
#[pyo3(name = "_test_parse_number")]
pub fn test_parse_number(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let mut src = source::Source::new(s);
    number::parse_number(&mut src, py)
}

#[pyfunction]
#[pyo3(name = "_test_parse_string")]
pub fn test_parse_string(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let mut src = source::Source::new(s);
    string::parse_string(&mut src, py)
}

#[pyfunction]
#[pyo3(name = "_test_parse_char")]
pub fn test_parse_char(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let mut src = source::Source::new(s);
    string::parse_char(&mut src, py)
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(py, m)?;
    m.add_function(wrap_pyfunction!(test_parse_number, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_string, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_char, m)?)?;
    Ok(())
}
