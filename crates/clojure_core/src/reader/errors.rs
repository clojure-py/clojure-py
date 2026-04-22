//! ReaderError — subclass of IllegalArgumentException with line/col context.

use pyo3::create_exception;
use pyo3::prelude::*;

create_exception!(
    clojure_core,
    ReaderError,
    crate::exceptions::IllegalArgumentException
);

pub(crate) fn make(msg: impl AsRef<str>, line: u32, column: u32) -> PyErr {
    ReaderError::new_err(format!(
        "{} (at line {}, col {})",
        msg.as_ref(),
        line,
        column
    ))
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ReaderError", py.get_type::<ReaderError>())?;
    Ok(())
}
