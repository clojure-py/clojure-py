use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// IChunk exposes only chunk-specific operations; `count` and `nth` are
/// covered by the shared `Counted` and `Indexed` protocols respectively, so
/// we don't redeclare them here (doing so would shadow those protocols'
/// module-level bindings).
#[protocol(name = "clojure.core/IChunk", extend_via_metadata = false)]
pub trait IChunk: Sized {
    fn drop_first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    /// Fold this chunk's elements with `f`, threading `init`. Used by the
    /// chunked-seq fast path inside `CollReduce` / `reduce1`.
    fn chunk_reduce(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject>;
}
