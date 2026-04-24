//! ArrayChunk — a fixed slice of a PyObject vector with a running offset.
//!
//! Port of `clojure/lang/ArrayChunk.java`. `drop-first` advances the offset
//! rather than allocating a new array. `Arc<[PyObject]>` makes chunk sharing
//! across drop-first chains cheap.

use crate::counted::Counted;
use crate::ichunk::IChunk;
use crate::indexed::Indexed;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "ArrayChunk", frozen)]
pub struct ArrayChunk {
    pub items: Arc<[PyObject]>,
    pub offset: usize,
}

impl ArrayChunk {
    pub fn new(items: Arc<[PyObject]>, offset: usize) -> Self {
        Self { items, offset }
    }
}

#[pymethods]
impl ArrayChunk {
    fn __len__(&self) -> usize {
        self.items.len().saturating_sub(self.offset)
    }

    #[getter(offset)]
    fn get_offset(&self) -> usize { self.offset }
}

#[implements(Counted)]
impl Counted for ArrayChunk {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        let b = this.bind(py).get();
        Ok(b.items.len().saturating_sub(b.offset))
    }
}

#[implements(Indexed)]
impl Indexed for ArrayChunk {
    fn nth(this: Py<Self>, py: Python<'_>, i: PyObject) -> PyResult<PyObject> {
        let b = this.bind(py).get();
        let i_usize: usize = i.bind(py).extract()?;
        let idx = b.offset + i_usize;
        if idx >= b.items.len() {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "ArrayChunk index {i_usize} out of bounds (count={})",
                b.items.len() - b.offset
            )));
        }
        Ok(b.items[idx].clone_ref(py))
    }

    fn nth_or_default(this: Py<Self>, py: Python<'_>, i: PyObject, default: PyObject) -> PyResult<PyObject> {
        let b = this.bind(py).get();
        let i_usize: usize = match i.bind(py).extract::<usize>() {
            Ok(n) => n,
            Err(_) => return Ok(default),
        };
        let idx = b.offset + i_usize;
        if idx >= b.items.len() {
            return Ok(default);
        }
        Ok(b.items[idx].clone_ref(py))
    }
}

#[implements(IChunk)]
impl IChunk for ArrayChunk {
    fn drop_first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let b = this.bind(py).get();
        if b.offset >= b.items.len() {
            return Err(crate::exceptions::IllegalStateException::new_err(
                "drop-first of empty chunk",
            ));
        }
        let next = ArrayChunk {
            items: b.items.clone(),
            offset: b.offset + 1,
        };
        Ok(Py::new(py, next)?.into_any())
    }

    fn chunk_reduce(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let b = this.bind(py).get();
        let mut acc = init;
        for i in b.offset..b.items.len() {
            let x = b.items[i].clone_ref(py);
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, x])?;
            // Propagate Reduced upward so the caller can bail. We don't
            // unwrap here — unwrapping is the outermost reducer's job.
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(acc);
            }
        }
        Ok(acc)
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ArrayChunk>()?;
    Ok(())
}
