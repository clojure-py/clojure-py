//! ChunkBuffer — mutable builder for an ArrayChunk.
//!
//! Port of `clojure/lang/ChunkBuffer.java`. `add` appends; `chunk` seals the
//! accumulated items into an ArrayChunk and resets the buffer so it can be
//! reused.

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

use crate::seqs::array_chunk::ArrayChunk;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "ChunkBuffer", frozen)]
pub struct ChunkBuffer {
    pub items: Mutex<Vec<PyObject>>,
    pub capacity: usize,
}

impl ChunkBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
        }
    }

    /// Rust-visible append (the `add` pymethod wraps this).
    pub fn push(&self, x: PyObject) -> PyResult<()> {
        let mut g = self.items.lock();
        if g.len() >= self.capacity {
            return Err(crate::exceptions::IllegalStateException::new_err(
                "ChunkBuffer overflow",
            ));
        }
        g.push(x);
        Ok(())
    }

    /// Rust-visible seal (the `chunk` pymethod wraps this).
    pub fn seal(&self, py: Python<'_>) -> PyResult<Py<ArrayChunk>> {
        let mut g = self.items.lock();
        let out: Vec<PyObject> = std::mem::take(&mut *g);
        let arc: Arc<[PyObject]> = Arc::from(out.into_boxed_slice());
        Py::new(py, ArrayChunk::new(arc, 0))
    }
}

#[pymethods]
impl ChunkBuffer {
    #[new]
    pub fn py_new(capacity: usize) -> Self {
        Self::new(capacity)
    }

    fn add(&self, x: PyObject) -> PyResult<()> { self.push(x) }

    fn chunk(&self, py: Python<'_>) -> PyResult<Py<ArrayChunk>> { self.seal(py) }

    fn count(&self) -> usize {
        self.items.lock().len()
    }

    fn __len__(&self) -> usize {
        self.count()
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ChunkBuffer>()?;
    Ok(())
}
