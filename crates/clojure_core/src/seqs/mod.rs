//! Seq types — Cons, LazySeq, VectorSeq, (future: ChunkedSeq, IteratorSeq).

pub mod cons;
pub mod lazy_seq;
pub mod vector_seq;

pub use cons::Cons;
pub use lazy_seq::LazySeq;
pub use vector_seq::VectorSeq;

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    cons::register(py, m)?;
    lazy_seq::register(py, m)?;
    vector_seq::register(py, m)?;
    Ok(())
}
