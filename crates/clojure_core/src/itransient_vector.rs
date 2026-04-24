use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientVector", extend_via_metadata = false, emit_fn_primary = true)]
pub trait ITransientVector: Sized {
    fn pop_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
