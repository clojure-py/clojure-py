use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IEditableCollection", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IEditableCollection: Sized {
    fn as_transient(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
