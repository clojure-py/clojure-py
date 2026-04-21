use pyo3::create_exception;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;

create_exception!(clojure_core, ArityException, PyTypeError);
create_exception!(clojure_core, IllegalStateException, PyRuntimeError);
create_exception!(clojure_core, IllegalArgumentException, PyValueError);

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ArityException", py.get_type::<ArityException>())?;
    m.add("IllegalStateException", py.get_type::<IllegalStateException>())?;
    m.add("IllegalArgumentException", py.get_type::<IllegalArgumentException>())?;
    Ok(())
}
