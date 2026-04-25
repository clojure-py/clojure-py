//! VectorRSeq — lazy reverse seq over a PersistentVector. Mirrors the JVM
//! `APersistentVector.RSeq`: O(1) to construct, O(1) per step, walks backward
//! via `nth`.

use crate::collections::pvector::PersistentVector;
use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::iseq::ISeq;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "VectorRSeq", frozen)]
pub struct VectorRSeq {
    pub vec: Py<PersistentVector>,
    /// Current index — always < cnt. Invariant: a VectorRSeq is non-empty,
    /// so `i` is a valid index at construction time.
    pub i: u32,
    pub meta: Option<PyObject>,
}

#[pymethods]
impl VectorRSeq {
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
}

#[implements(ISeq)]
impl ISeq for VectorRSeq {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        s.vec.bind(py).get().nth_internal_pub(py, s.i as usize)
    }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.i == 0 {
            return Ok(py.None());
        }
        let rs = VectorRSeq {
            vec: s.vec.clone_ref(py),
            i: s.i - 1,
            meta: None,
        };
        Ok(Py::new(py, rs)?.into_any())
    }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let n = <VectorRSeq as ISeq>::next(this.clone_ref(py), py)?;
        if n.is_none(py) {
            Ok(crate::collections::plist::empty_list(py).into_any())
        } else {
            Ok(n)
        }
    }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let new = crate::seqs::cons::Cons::new(x, this.into_any());
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for VectorRSeq {
    fn seq(this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.into_any())
    }
}

#[implements(Counted)]
impl Counted for VectorRSeq {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        // i is the current head's index; remaining count = i + 1.
        Ok((this.bind(py).get().i + 1) as usize)
    }
}

#[implements(IEquiv)]
impl IEquiv for VectorRSeq {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        if !crate::rt::is_sequential(py, &other) {
            return Ok(false);
        }
        crate::rt::sequential_equiv(py, this.into_any(), other)
    }
}

#[implements(IHashEq)]
impl IHashEq for VectorRSeq {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        Ok(crate::murmur3::hash_ordered_seq(py, this.into_any())? as i64)
    }
}

#[implements(IMeta)]
impl IMeta for VectorRSeq {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, VectorRSeq {
            vec: s.vec.clone_ref(py),
            i: s.i,
            meta: m,
        })?.into_any())
    }
}

#[implements(Sequential)]
impl Sequential for VectorRSeq {}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<VectorRSeq>()?;
    Ok(())
}
