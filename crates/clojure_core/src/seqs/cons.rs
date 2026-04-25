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
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Cons", frozen)]
pub struct Cons {
    pub first: PyObject,
    pub more: PyObject,   // another seq-like or nil
    pub meta: Option<PyObject>,
}

impl Cons {
    pub fn new(first: PyObject, more: PyObject) -> Self {
        Self { first, more, meta: None }
    }
}

/// Iterative Drop to avoid Rust-stack overflow on long
/// `LazySeq → Cons → LazySeq → …` chains (count crashes ~14k without this).
/// CPython exposes `Py_TRASHCAN_BEGIN`/`END` macros for this purpose, but
/// pyo3-ffi marks them as private and doesn't wrap them, so we roll our own:
///
///   * `DRAIN_ACTIVE` is true iff a drain loop is in progress on this thread.
///     Nested Drops see it as true and only push.
///   * `DRAIN_STACK` holds the objects still to be dropped.
///
/// Both `Cons::drop` and `LazySeq::drop` use this — chain links are pushed
/// instead of auto-dropped inline, and the outermost Drop iteratively
/// drains. Inner Drops run in normal (flat) stack depth.
thread_local! {
    static DRAIN_STACK: std::cell::RefCell<Vec<PyObject>> = const {
        std::cell::RefCell::new(Vec::new())
    };
    static DRAIN_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn defer_drop(obj: PyObject) {
    DRAIN_STACK.with(|s| s.borrow_mut().push(obj));
}

pub(crate) fn run_outer_drain() {
    if DRAIN_ACTIVE.with(|a| a.get()) {
        return;
    }
    DRAIN_ACTIVE.with(|a| a.set(true));
    loop {
        let next = DRAIN_STACK.with(|s| s.borrow_mut().pop());
        match next {
            Some(obj) => drop(obj),
            None => break,
        }
    }
    DRAIN_ACTIVE.with(|a| a.set(false));
}

impl Drop for Cons {
    fn drop(&mut self) {
        Python::attach(|py| {
            let more = std::mem::replace(&mut self.more, py.None());
            defer_drop(more);
            run_outer_drain();
        });
    }
}


#[pymethods]
impl Cons {
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
        format_seq(py, slf.into_any())
    }
}

/// Walk a seq and print as `(a b c …)`. Shared between Cons, VectorSeq,
/// VectorRSeq, and any other ISeq that wants Clojure-style printing.
pub fn format_seq(py: Python<'_>, start: PyObject) -> PyResult<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur: PyObject = start;
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
        // Cross-type sequential equality per Clojure: a Cons is equal to any
        // Sequential collection with the same elements in the same order.
        if !crate::rt::is_sequential(py, &other) {
            return Ok(false);
        }
        crate::rt::sequential_equiv(py, this.into_any(), other)
    }
}

#[implements(IHashEq)]
impl IHashEq for Cons {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // Vanilla Cons.hasheq = Murmur3.hashOrdered.
        Ok(crate::murmur3::hash_ordered_seq(py, this.into_any())? as i64)
    }
}

#[implements(IMeta)]
impl IMeta for Cons {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, Cons {
            first: s.first.clone_ref(py),
            more: s.more.clone_ref(py),
            meta: m,
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
