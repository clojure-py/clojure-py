//! ChunkedCons — `(chunk, rest)` pair where iteration is done chunk-at-a-time.
//!
//! Port of `clojure/lang/ChunkedCons.java`. Realizes as a Clojure ISeq: `first`
//! is `chunk.nth(0)`, `more` either drops the first from the chunk (still a
//! ChunkedCons) or seqifies `rest`. In addition it implements IChunkedSeq so
//! chunk-aware consumers like `concat` can forward whole chunks.

use crate::counted::Counted;
use crate::ichunk::IChunk;
use crate::ichunkedseq::IChunkedSeq;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::iseq::ISeq;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;
use crate::seqs::array_chunk::ArrayChunk;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "ChunkedCons", frozen)]
pub struct ChunkedCons {
    pub chunk: Py<ArrayChunk>,
    pub rest: PyObject, // another seq-like or nil
    pub meta: Option<PyObject>,
}

impl ChunkedCons {
    pub fn new(chunk: Py<ArrayChunk>, rest: PyObject) -> Self {
        Self { chunk, rest, meta: None }
    }

    fn chunk_count(&self, py: Python<'_>) -> usize {
        let c = self.chunk.bind(py).get();
        c.items.len().saturating_sub(c.offset)
    }
}

#[pymethods]
impl ChunkedCons {
    fn __len__(slf: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        crate::rt::count(py, slf.into_any())
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<crate::seqs::cons::ConsIter>> {
        Py::new(py, crate::seqs::cons::ConsIter { current: slf.into_any() })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        crate::seqs::cons::format_seq(py, slf.into_any())
    }

    #[getter(meta)]
    fn get_meta(&self, py: Python<'_>) -> PyObject {
        self.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }
}

#[implements(ISeq)]
impl ISeq for ChunkedCons {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let c = s.chunk.bind(py).get();
        if c.offset >= c.items.len() {
            return Ok(py.None());
        }
        Ok(c.items[c.offset].clone_ref(py))
    }

    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.chunk_count(py) > 1 {
            let dropped = <ArrayChunk as IChunk>::drop_first(s.chunk.clone_ref(py), py)?;
            // dropped is a Py<ArrayChunk>-typed PyObject
            let dropped_py = dropped.bind(py).cast::<ArrayChunk>()?.clone().unbind();
            let new = ChunkedCons::new(dropped_py, s.rest.clone_ref(py));
            return Ok(Py::new(py, new)?.into_any());
        }
        crate::rt::seq(py, s.rest.clone_ref(py))
    }

    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.chunk_count(py) > 1 {
            let dropped = <ArrayChunk as IChunk>::drop_first(s.chunk.clone_ref(py), py)?;
            let dropped_py = dropped.bind(py).cast::<ArrayChunk>()?.clone().unbind();
            let new = ChunkedCons::new(dropped_py, s.rest.clone_ref(py));
            return Ok(Py::new(py, new)?.into_any());
        }
        if s.rest.is_none(py) {
            return Ok(crate::collections::plist::empty_list(py).into_any());
        }
        crate::rt::seq(py, s.rest.clone_ref(py))
    }

    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let new = crate::seqs::cons::Cons::new(x, this.into_any());
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for ChunkedCons {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.chunk_count(py) == 0 {
            return crate::rt::seq(py, s.rest.clone_ref(py));
        }
        Ok(this.into_any())
    }
}

#[implements(Counted)]
impl Counted for ChunkedCons {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        let mut n: usize = 0;
        let mut cur: PyObject = this.into_any();
        loop {
            let s = crate::rt::seq(py, cur)?;
            if s.is_none(py) { break; }
            n += 1;
            cur = crate::rt::next_(py, s)?;
            if cur.is_none(py) { break; }
        }
        Ok(n)
    }
}

#[implements(IEquiv)]
impl IEquiv for ChunkedCons {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        if !crate::rt::is_sequential(py, &other) {
            return Ok(false);
        }
        crate::rt::sequential_equiv(py, this.into_any(), other)
    }
}

#[implements(IHashEq)]
impl IHashEq for ChunkedCons {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        Ok(crate::murmur3::hash_ordered_seq(py, this.into_any())? as i64)
    }
}

#[implements(IMeta)]
impl IMeta for ChunkedCons {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, ChunkedCons {
            chunk: s.chunk.clone_ref(py),
            rest: s.rest.clone_ref(py),
            meta: m,
        })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for ChunkedCons {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        <ChunkedCons as Counted>::count(this, py)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        <ChunkedCons as ISeq>::cons(this, py, x)
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(crate::collections::plist::empty_list(py).into_any())
    }
}

#[implements(Sequential)]
impl Sequential for ChunkedCons {}

#[implements(IChunkedSeq)]
impl IChunkedSeq for ChunkedCons {
    fn chunked_first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().chunk.clone_ref(py).into_any())
    }

    fn chunked_more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.rest.is_none(py) {
            return Ok(crate::collections::plist::empty_list(py).into_any());
        }
        let seqd = crate::rt::seq(py, s.rest.clone_ref(py))?;
        if seqd.is_none(py) {
            return Ok(crate::collections::plist::empty_list(py).into_any());
        }
        Ok(seqd)
    }

    fn chunked_next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        crate::rt::seq(py, s.rest.clone_ref(py))
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ChunkedCons>()?;
    Ok(())
}
