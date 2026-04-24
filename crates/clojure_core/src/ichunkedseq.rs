use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IChunkedSeq", extend_via_metadata = false)]
pub trait IChunkedSeq: Sized {
    /// Current chunk — an IChunk.
    fn chunked_first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    /// Rest as a (possibly empty) seq.
    fn chunked_more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    /// Rest as a seq, or nil when empty.
    fn chunked_next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
