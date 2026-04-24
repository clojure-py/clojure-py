use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentSet", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IPersistentSet: Sized {
    fn disjoin(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
