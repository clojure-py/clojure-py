use pyo3::create_exception;
use pyo3::prelude::*;

create_exception!(
    clojure_core,
    EvalError,
    crate::exceptions::IllegalArgumentException
);

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("EvalError", py.get_type::<EvalError>())?;
    Ok(())
}

pub fn err(msg: impl AsRef<str>) -> PyErr {
    EvalError::new_err(msg.as_ref().to_string())
}
