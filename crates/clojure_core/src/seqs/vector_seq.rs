//! VectorSeq — simple index-walking seq over PersistentVector. Phase 12A's
//! placeholder for ChunkedSeq; ChunkedSeq's 32-at-a-time optimization lands
//! later.

use crate::collections::pvector::PersistentVector;
use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::iseq::ISeq;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;
use clojure_core_macros::implements;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "VectorSeq", frozen)]
pub struct VectorSeq {
    pub vec: Py<PersistentVector>,
    pub i: u32,
    pub meta: RwLock<Option<PyObject>>,
}

#[pymethods]
impl VectorSeq {
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
}

#[implements(ISeq)]
impl ISeq for VectorSeq {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        s.vec.bind(py).get().nth_internal_pub(py, s.i as usize)
    }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let next_i = s.i + 1;
        if next_i >= s.vec.bind(py).get().cnt {
            return Ok(py.None());
        }
        let vs = VectorSeq {
            vec: s.vec.clone_ref(py),
            i: next_i,
            meta: RwLock::new(None),
        };
        Ok(Py::new(py, vs)?.into_any())
    }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let n = <VectorSeq as ISeq>::next(this.clone_ref(py), py)?;
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
impl ISeqable for VectorSeq {
    fn seq(this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.into_any())
    }
}

#[implements(Counted)]
impl Counted for VectorSeq {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        let s = this.bind(py).get();
        Ok((s.vec.bind(py).get().cnt - s.i) as usize)
    }
}

#[implements(IEquiv)]
impl IEquiv for VectorSeq {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        // Walk both.
        let mut ap: PyObject = this.into_any();
        let mut bp: PyObject = other;
        loop {
            let sa = crate::rt::seq(py, ap.clone_ref(py))?;
            let sb = crate::rt::seq(py, bp.clone_ref(py))?;
            if sa.is_none(py) && sb.is_none(py) { return Ok(true); }
            if sa.is_none(py) || sb.is_none(py) { return Ok(false); }
            let ha = crate::rt::first(py, sa.clone_ref(py))?;
            let hb = crate::rt::first(py, sb.clone_ref(py))?;
            if !crate::rt::equiv(py, ha, hb)? { return Ok(false); }
            ap = crate::rt::next_(py, sa)?;
            bp = crate::rt::next_(py, sb)?;
        }
    }
}

#[implements(IHashEq)]
impl IHashEq for VectorSeq {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        let mut h: i64 = 1;
        let mut cur: PyObject = this.into_any();
        loop {
            let s = crate::rt::seq(py, cur)?;
            if s.is_none(py) { break; }
            let head = crate::rt::first(py, s.clone_ref(py))?;
            let eh = crate::rt::hash_eq(py, head)?;
            h = h.wrapping_mul(31).wrapping_add(eh);
            cur = crate::rt::next_(py, s)?;
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for VectorSeq {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, VectorSeq {
            vec: s.vec.clone_ref(py),
            i: s.i,
            meta: RwLock::new(m),
        })?.into_any())
    }
}

#[implements(Sequential)]
impl Sequential for VectorSeq {}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<VectorSeq>()?;
    Ok(())
}
