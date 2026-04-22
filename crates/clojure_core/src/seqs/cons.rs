//! Cons — basic non-lazy cons cell: (first, more, meta).

use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::iseq::ISeq;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;
use clojure_core_macros::implements;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Cons", frozen)]
pub struct Cons {
    pub first: PyObject,
    pub more: PyObject,   // another seq-like or nil
    pub meta: RwLock<Option<PyObject>>,
}

impl Cons {
    pub fn new(first: PyObject, more: PyObject) -> Self {
        Self { first, more, meta: RwLock::new(None) }
    }
}

#[pymethods]
impl Cons {
    #[new]
    pub fn py_new(first: PyObject, more: PyObject) -> Self {
        Self::new(first, more)
    }

    #[getter(first)]
    fn get_first(&self, py: Python<'_>) -> PyObject { self.first.clone_ref(py) }
    #[getter(more)]
    fn get_more(&self, py: Python<'_>) -> PyObject { self.more.clone_ref(py) }

    fn __len__(slf: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        crate::rt::count(py, slf.into_any())
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<ConsIter>> {
        Py::new(py, ConsIter { current: slf.into_any() })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        // Walk the seq and print as (a b c ...)
        let mut parts: Vec<String> = Vec::new();
        let mut cur: PyObject = slf.into_any();
        loop {
            let s = crate::rt::seq(py, cur.clone_ref(py))?;
            if s.is_none(py) { break; }
            let head = crate::rt::first(py, s.clone_ref(py))?;
            parts.push(head.bind(py).repr()?.extract::<String>()?);
            cur = crate::rt::next_(py, s)?;
            if cur.is_none(py) { break; }
        }
        Ok(format!("({})", parts.join(" ")))
    }

    #[getter(meta)]
    fn get_meta(&self, py: Python<'_>) -> PyObject {
        self.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<Cons>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(py, Cons {
            first: self.first.clone_ref(py),
            more: self.more.clone_ref(py),
            meta: RwLock::new(m),
        })
    }
}

#[pyclass(module = "clojure._core", name = "ConsIter")]
pub struct ConsIter {
    pub current: PyObject,
}

#[pymethods]
impl ConsIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let s = crate::rt::seq(py, self.current.clone_ref(py))?;
        if s.is_none(py) {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        let head = crate::rt::first(py, s.clone_ref(py))?;
        self.current = crate::rt::next_(py, s)?;
        Ok(head)
    }
}

#[implements(ISeq)]
impl ISeq for Cons {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().first.clone_ref(py))
    }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        crate::rt::seq(py, s.more.clone_ref(py))
    }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().more.clone_ref(py))
    }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        // Prepend x: new Cons{first: x, more: this}.
        let new = Cons::new(x, this.into_any());
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for Cons {
    fn seq(this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        // A Cons IS a seq.
        Ok(this.into_any())
    }
}

#[implements(Counted)]
impl Counted for Cons {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        // Cons is O(n) count — walk.
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
impl IEquiv for Cons {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        // Same-type only for now.
        let other_b = other.bind(py);
        if other_b.downcast::<Cons>().is_err() && other_b.downcast::<crate::collections::plist::PersistentList>().is_err() {
            return Ok(false);
        }
        // Walk both seqs pairwise.
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
            if ap.is_none(py) && bp.is_none(py) { return Ok(true); }
        }
    }
}

#[implements(IHashEq)]
impl IHashEq for Cons {
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
            if cur.is_none(py) { break; }
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for Cons {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, Cons {
            first: s.first.clone_ref(py),
            more: s.more.clone_ref(py),
            meta: RwLock::new(m),
        })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for Cons {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        <Cons as Counted>::count(this, py)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        <Cons as ISeq>::cons(this, py, x)
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(crate::collections::plist::empty_list(py).into_any())
    }
}

#[implements(Sequential)]
impl Sequential for Cons {}

#[pyfunction]
#[pyo3(name = "cons")]
pub fn py_cons(py: Python<'_>, first: PyObject, more: PyObject) -> PyResult<Py<Cons>> {
    Py::new(py, Cons::new(first, more))
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Cons>()?;
    m.add_class::<ConsIter>()?;
    m.add_function(wrap_pyfunction!(py_cons, m)?)?;
    Ok(())
}
