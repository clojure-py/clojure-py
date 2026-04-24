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

/// Read exactly one form from the input and return `(form, consumed_bytes)`.
/// Unlike `read_string_py`, leaves non-whitespace trailing content
/// un-consumed so the caller can resume — the Clojure-layer `(read)` uses
/// this to preserve pushback across calls and accumulate multi-line forms.
/// Trailing whitespace / comments after the form ARE consumed, so the
/// caller won't treat e.g. a lingering `\n` as pending input. Raises the
/// standard `ReaderError` ("EOF while reading …") on incomplete input so
/// the caller can detect and accumulate more.
#[pyfunction]
#[pyo3(name = "read_string_prefix")]
pub fn read_string_prefix_py(py: Python<'_>, s: &str) -> PyResult<(PyObject, usize)> {
    let mut src = source::Source::new(s);
    dispatch::skip_ws_and_comments(&mut src);
    if src.at_eof() {
        return Err(errors::make("EOF while reading", src.line(), src.column()));
    }
    let form = dispatch::read_one(&mut src, py)?;
    dispatch::skip_ws_and_comments(&mut src);
    Ok((form, src.offset()))
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
    m.add_function(wrap_pyfunction!(read_string_prefix_py, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_number, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_string, m)?)?;
    m.add_function(wrap_pyfunction!(test_parse_char, m)?)?;
    Ok(())
}
