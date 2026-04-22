//! LazySeq — thunk-cached lazy sequence. First access realizes by calling
//! the thunk (a 0-arity IFn) and caching the result.

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

enum LazySeqState {
    Unrealized(PyObject),       // the thunk
    Realized(Option<PyObject>), // None = empty seq; Some(s) = non-empty seq
}

#[pyclass(module = "clojure._core", name = "LazySeq", frozen)]
pub struct LazySeq {
    state: RwLock<LazySeqState>,
    meta: RwLock<Option<PyObject>>,
}

impl LazySeq {
    fn realize(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        // Fast path: already realized (double-check under read lock).
        {
            let g = self.state.read();
            if let LazySeqState::Realized(v) = &*g {
                return Ok(v.as_ref().map(|o| o.clone_ref(py)));
            }
        }
        // Slow path: acquire write lock, re-check, clone thunk, then drop lock.
        let thunk = {
            let g = self.state.write();
            match &*g {
                LazySeqState::Realized(v) => {
                    return Ok(v.as_ref().map(|o| o.clone_ref(py)));
                }
                LazySeqState::Unrealized(t) => t.clone_ref(py),
            }
            // `g` dropped here — we release the lock before calling the
            // thunk so thunk bodies may re-enter this LazySeq via seq on a
            // nested LazySeq without deadlocking.
        };
        let raw_result = crate::rt::invoke_n(py, thunk, &[])?;
        // Unwrap nested LazySeqs iteratively to prevent deep recursion.
        let mut cur = raw_result;
        loop {
            let cur_b = cur.bind(py);
            if let Ok(nested) = cur_b.downcast::<LazySeq>() {
                cur = match nested.get().realize(py)? {
                    Some(s) => s,
                    None => {
                        let mut g2 = self.state.write();
                        *g2 = LazySeqState::Realized(None);
                        return Ok(None);
                    }
                };
                continue;
            }
            break;
        }
        // cur is now a non-LazySeq; seq-it to get an ISeq (or None).
        let s = crate::rt::seq(py, cur)?;
        let result = if s.is_none(py) { None } else { Some(s) };
        let mut g2 = self.state.write();
        *g2 = LazySeqState::Realized(result.as_ref().map(|o| o.clone_ref(py)));
        Ok(result)
    }
}

#[pymethods]
impl LazySeq {
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

    #[getter(meta)]
    fn get_meta(&self, py: Python<'_>) -> PyObject {
        self.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }
}

#[implements(ISeq)]
impl ISeq for LazySeq {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get().realize(py)?;
        match s {
            Some(seq) => crate::rt::first(py, seq),
            None => Ok(py.None()),
        }
    }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get().realize(py)?;
        match s {
            Some(seq) => crate::rt::next_(py, seq),
            None => Ok(py.None()),
        }
    }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get().realize(py)?;
        match s {
            Some(seq) => crate::rt::rest(py, seq),
            None => Ok(crate::collections::plist::empty_list(py).into_any()),
        }
    }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let new = crate::seqs::cons::Cons::new(x, this.into_any());
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for LazySeq {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get().realize(py)?;
        Ok(s.unwrap_or_else(|| py.None()))
    }
}

#[implements(Counted)]
impl Counted for LazySeq {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        // Walk. O(n) + realize all.
        let s = this.bind(py).get().realize(py)?;
        match s {
            Some(seq) => crate::rt::count(py, seq),
            None => Ok(0),
        }
    }
}

#[implements(IEquiv)]
impl IEquiv for LazySeq {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        // Same structural-walk approach as Cons.
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
impl IHashEq for LazySeq {
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
impl IMeta for LazySeq {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        // Copy the current state (realized or not).
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        let new_state = match &*s.state.read() {
            LazySeqState::Unrealized(t) => LazySeqState::Unrealized(t.clone_ref(py)),
            LazySeqState::Realized(v) => LazySeqState::Realized(v.as_ref().map(|o| o.clone_ref(py))),
        };
        Ok(Py::new(py, LazySeq {
            state: RwLock::new(new_state),
            meta: RwLock::new(m),
        })?.into_any())
    }
}

#[implements(Sequential)]
impl Sequential for LazySeq {}

#[pyfunction]
#[pyo3(name = "lazy_seq")]
pub fn py_lazy_seq(thunk: PyObject) -> LazySeq {
    LazySeq {
        state: RwLock::new(LazySeqState::Unrealized(thunk)),
        meta: RwLock::new(None),
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<LazySeq>()?;
    m.add_function(wrap_pyfunction!(py_lazy_seq, m)?)?;
    Ok(())
}
