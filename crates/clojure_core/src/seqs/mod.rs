//! Seq types — Cons, LazySeq, VectorSeq, Delay, ArrayChunk/ChunkBuffer/ChunkedCons.

pub mod array_chunk;
pub mod chunk_buffer;
pub mod chunked_cons;
pub mod cons;
pub mod delay;
pub mod lazy_seq;
pub mod vector_rseq;
pub mod vector_seq;

pub use array_chunk::ArrayChunk;
pub use chunk_buffer::ChunkBuffer;
pub use chunked_cons::ChunkedCons;
pub use cons::Cons;
pub use delay::Delay;
pub use lazy_seq::LazySeq;
pub use vector_rseq::VectorRSeq;
pub use vector_seq::VectorSeq;

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    cons::register(py, m)?;
    lazy_seq::register(py, m)?;
    vector_seq::register(py, m)?;
    vector_rseq::register(py, m)?;
    delay::register(py, m)?;
    array_chunk::register(py, m)?;
    chunk_buffer::register(py, m)?;
    chunked_cons::register(py, m)?;
    Ok(())
}
