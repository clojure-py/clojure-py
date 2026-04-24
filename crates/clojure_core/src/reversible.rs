use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// `clojure.core/Reversible` — return a reverse seq. `rseq` is O(1) for
/// indexed collections (vectors) that already know their length.
#[protocol(name = "clojure.core/Reversible", extend_via_metadata = false, emit_fn_primary = true)]
pub trait Reversible: Sized {
    fn rseq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
