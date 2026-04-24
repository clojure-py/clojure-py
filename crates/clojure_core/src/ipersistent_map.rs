use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentMap", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IPersistentMap: Sized {
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
    fn without(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
