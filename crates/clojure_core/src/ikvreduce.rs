//! IKVReduce — key/value reducer for map-like collections. Backs
//! `clojure.core/reduce-kv`. No built-in fallback: only types that implement
//! it are reducible-by-kv.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IKVReduce", extend_via_metadata = false)]
pub trait IKVReduce: Sized {
    fn kv_reduce(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject>;
}
