//! Clojure reader — recursive-descent parser.

pub mod dispatch;
pub mod errors;
pub mod forms;
pub mod lexer;
pub mod number;
pub mod source;
pub mod string;
pub mod token;

use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Public entry point: read exactly one form from the input string. Errors on
/// EOF (empty input), trailing content, or any malformed form.
#[pyfunction]
#[pyo3(name = "read_string")]
pub fn read_string_py(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let mut src = source::Source::new(s);
    dispatch::skip_ws_and_comments(&mut src);
    if src.at_eof() {
        return Err(errors::make("EOF while reading", src.line(), src.column()));
    }
    let form = dispatch::read_one(&mut src, py)?;
    dispatch::skip_ws_and_comments(&mut src);
    if !src.at_eof() {
        return Err(errors::make(
            "Unexpected trailing content after form",
            src.line(),
            src.column(),
        ));
    }
    Ok(form)
}

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
    m.add_function(wrap_pyfunction!(read_string_py, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_number, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_string, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_char, m)?)?;
    Ok(())
}
