use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientSet", extend_via_metadata = false, emit_fn_primary = true)]
pub trait ITransientSet: Sized {
    fn disj_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
