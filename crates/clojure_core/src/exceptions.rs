use pyo3::create_exception;
use pyo3::exceptions::{PyAssertionError, PyBaseException, PyException, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;

create_exception!(clojure_core, ArityException, PyTypeError);
create_exception!(clojure_core, IllegalStateException, PyRuntimeError);
create_exception!(clojure_core, IllegalArgumentException, PyValueError);
// Raised by (assert ...) and by :pre / :post condition failures in fn/defn.
create_exception!(clojure_core, AssertionError, PyAssertionError);
// `ex-info` exception type. Carries a `.data` attribute (Clojure map) and
// optional `.__cause__` (Python's standard cause chain). Subclasses
// `Exception` so `(catch Exception e ...)` matches.
create_exception!(clojure_core, ExceptionInfo, PyException);

// RetryEx is the STM-internal retry signal. Inherits `PyBaseException` (not
// `PyException`) so a `(try ... (catch Exception e ...))` inside a `dosync`
// body cannot accidentally swallow it — matching vanilla's `RetryEx extends
// Error` arrangement. Not re-exported at the module top-level; only
// `stm::txn` raises and catches it.
create_exception!(clojure_core, RetryEx, PyBaseException);

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ArityException", py.get_type::<ArityException>())?;
    m.add("IllegalStateException", py.get_type::<IllegalStateException>())?;
    m.add("IllegalArgumentException", py.get_type::<IllegalArgumentException>())?;
    m.add("AssertionError", py.get_type::<AssertionError>())?;
    m.add("ExceptionInfo", py.get_type::<ExceptionInfo>())?;
    Ok(())
}
