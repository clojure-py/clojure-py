//! `Sorted` — the protocol mirror of `clojure.lang.Sorted`, implemented by
//! `PersistentTreeMap` and `PersistentTreeSet`.
//!
//! Distinguishing a sorted collection from any other seqable:
//! - `seq(ascending?)` — iterate in ascending (true) or descending (false) order
//! - `seq_from(key, ascending?)` — iterate from the first entry `>= key` (asc)
//!   or `<= key` (desc)
//! - `entry_key(entry)` — extract the comparison key from an entry (identity
//!   for sets; key of a MapEntry for maps)
//! - `comparator_of` — return the underlying comparator callable (may be a
//!   Clojure `compare` sentinel if using default ordering)

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Sorted", extend_via_metadata = false, emit_fn_primary = true)]
pub trait Sorted: Sized {
    fn sorted_seq(this: Py<Self>, py: Python<'_>, ascending: PyObject) -> PyResult<PyObject>;
    fn sorted_seq_from(
        this: Py<Self>,
        py: Python<'_>,
        key: PyObject,
        ascending: PyObject,
    ) -> PyResult<PyObject>;
    fn entry_key(this: Py<Self>, py: Python<'_>, entry: PyObject) -> PyResult<PyObject>;
    fn comparator_of(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
