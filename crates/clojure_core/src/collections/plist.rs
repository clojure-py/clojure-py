//! PersistentList — cons-cell linked list. EmptyList is a module-init singleton.

use crate::counted::Counted;
use crate::exceptions::IllegalStateException;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_list::IPersistentList;
use crate::ipersistent_stack::IPersistentStack;
use crate::iseq::ISeq;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;
use clojure_core_macros::implements;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

// --- EmptyList ---

#[pyclass(module = "clojure._core", name = "EmptyList", frozen)]
pub struct EmptyList {
    meta: RwLock<Option<PyObject>>,
}

static EMPTY_LIST: OnceCell<Py<EmptyList>> = OnceCell::new();

pub fn empty_list(py: Python<'_>) -> Py<EmptyList> {
    EMPTY_LIST.get().expect("plist::register not called").clone_ref(py)
}

#[pymethods]
impl EmptyList {
    fn __len__(&self) -> usize { 0 }
    fn __bool__(&self) -> bool { false }
    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<EmptyListIter>> {
        let _ = slf;
        Py::new(py, EmptyListIter)
    }
    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }
    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }
    fn __repr__(&self) -> String { "()".to_string() }
    fn __str__(&self) -> String { "()".to_string() }

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<EmptyList>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(py, EmptyList { meta: RwLock::new(m) })
    }
}

#[pyclass(module = "clojure._core", name = "EmptyListIter")]
pub struct EmptyListIter;

#[pymethods]
impl EmptyListIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> { slf }
    fn __next__(&self) -> PyResult<PyObject> {
        Err(pyo3::exceptions::PyStopIteration::new_err(()))
    }
}

#[implements(ISeq)]
impl ISeq for EmptyList {
    fn first(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
    fn next(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
    fn more(this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> { Ok(this.into_any()) }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let new = PersistentList {
            head: x,
            tail: this.into_any(),
            count: 1,
            meta: RwLock::new(None),
        };
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for EmptyList {
    fn seq(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
}

#[implements(Counted)]
impl Counted for EmptyList {
    fn count(_this: Py<Self>, _py: Python<'_>) -> PyResult<usize> { Ok(0) }
}

#[implements(IEquiv)]
impl IEquiv for EmptyList {
    fn equiv(_this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let b = other.bind(py);
        // Only another EmptyList is equal (PersistentList invariant: count >= 1).
        Ok(b.downcast::<EmptyList>().is_ok())
    }
}

#[implements(IHashEq)]
impl IHashEq for EmptyList {
    fn hash_eq(_this: Py<Self>, _py: Python<'_>) -> PyResult<i64> { Ok(1) }
}

#[implements(IMeta)]
impl IMeta for EmptyList {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(_this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, EmptyList { meta: RwLock::new(m) })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for EmptyList {
    fn count(_this: Py<Self>, _py: Python<'_>) -> PyResult<usize> { Ok(0) }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        <EmptyList as ISeq>::cons(this, py, x)
    }
    fn empty(this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> { Ok(this.into_any()) }
}

#[implements(IPersistentList)]
impl IPersistentList for EmptyList {}

#[implements(IPersistentStack)]
impl IPersistentStack for EmptyList {
    fn peek(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
    fn pop(_this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Err(IllegalStateException::new_err("Can't pop empty list"))
    }
}

#[implements(Sequential)]
impl Sequential for EmptyList {}

// --- PersistentList ---

#[pyclass(module = "clojure._core", name = "PersistentList", frozen)]
pub struct PersistentList {
    pub head: PyObject,
    pub tail: PyObject,  // PersistentList | EmptyList
    pub count: u32,
    pub meta: RwLock<Option<PyObject>>,
}

#[pymethods]
impl PersistentList {
    #[getter] fn first(&self, py: Python<'_>) -> PyObject { self.head.clone_ref(py) }
    #[getter] fn rest(&self, py: Python<'_>) -> PyObject { self.tail.clone_ref(py) }

    fn __len__(&self) -> usize { self.count as usize }
    fn __bool__(&self) -> bool { true }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentListIter>> {
        Py::new(py, PersistentListIter { current: slf.into_any() })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let mut parts: Vec<String> = Vec::new();
        let mut cur: PyObject = slf.into_any();
        loop {
            let b = cur.bind(py);
            if b.downcast::<EmptyList>().is_ok() { break; }
            if let Ok(pl) = b.downcast::<PersistentList>() {
                let r = pl.get().head.bind(py).repr()?.extract::<String>()?;
                parts.push(r);
                cur = pl.get().tail.clone_ref(py);
                continue;
            }
            break;
        }
        Ok(format!("({})", parts.join(" ")))
    }
    fn __str__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> { Self::__repr__(slf, py) }

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<PersistentList>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(py, PersistentList {
            head: self.head.clone_ref(py),
            tail: self.tail.clone_ref(py),
            count: self.count,
            meta: RwLock::new(m),
        })
    }
}

#[pyclass(module = "clojure._core", name = "PersistentListIter")]
pub struct PersistentListIter {
    current: PyObject,
}

#[pymethods]
impl PersistentListIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let b = self.current.bind(py);
        if b.downcast::<EmptyList>().is_ok() {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        if let Ok(pl) = b.downcast::<PersistentList>() {
            let h = pl.get().head.clone_ref(py);
            self.current = pl.get().tail.clone_ref(py);
            return Ok(h);
        }
        Err(pyo3::exceptions::PyStopIteration::new_err(()))
    }
}

#[implements(ISeq)]
impl ISeq for PersistentList {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().head.clone_ref(py))
    }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let b = s.tail.bind(py);
        if b.downcast::<EmptyList>().is_ok() { return Ok(py.None()); }
        Ok(s.tail.clone_ref(py))
    }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().tail.clone_ref(py))
    }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let count = this.bind(py).get().count + 1;
        let new = PersistentList {
            head: x,
            tail: this.into_any(),  // self-as-tail; structural sharing
            count,
            meta: RwLock::new(None),
        };
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for PersistentList {
    fn seq(this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.into_any())  // non-empty list IS a seq
    }
}

#[implements(Counted)]
impl Counted for PersistentList {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentList {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let other_b = other.bind(py);
        // Only compare same type (cross-type sequential equality is deferred).
        let Ok(other_pl) = other_b.downcast::<PersistentList>() else {
            return Ok(false);
        };
        let a = this.bind(py).get();
        let b = other_pl.get();
        if a.count != b.count { return Ok(false); }

        // Walk both lists pairwise.
        let mut ap: PyObject = this.clone_ref(py).into_any();
        let mut bp: PyObject = other.clone_ref(py);
        loop {
            let ab = ap.bind(py);
            let bb = bp.bind(py);
            let a_empty = ab.downcast::<EmptyList>().is_ok();
            let b_empty = bb.downcast::<EmptyList>().is_ok();
            if a_empty && b_empty { return Ok(true); }
            if a_empty != b_empty { return Ok(false); }
            let apl = ab.downcast::<PersistentList>()?;
            let bpl = bb.downcast::<PersistentList>()?;
            let a_head = apl.get().head.clone_ref(py);
            let b_head = bpl.get().head.clone_ref(py);
            if !crate::rt::equiv(py, a_head, b_head)? { return Ok(false); }
            ap = apl.get().tail.clone_ref(py);
            bp = bpl.get().tail.clone_ref(py);
        }
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentList {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // Simple structural hash: fold each element's hash_eq.
        // Clojure's actual formula is more involved — matching bit-for-bit is deferred
        // per spec §10 non-goal. What matters here: (equal) => (same hash).
        let mut h: i64 = 1;
        let mut cur: PyObject = this.into_any();
        loop {
            let b = cur.bind(py);
            if b.downcast::<EmptyList>().is_ok() { break; }
            if let Ok(pl) = b.downcast::<PersistentList>() {
                let head = pl.get().head.clone_ref(py);
                let eh = crate::rt::hash_eq(py, head)?;
                h = h.wrapping_mul(31).wrapping_add(eh);
                cur = pl.get().tail.clone_ref(py);
                continue;
            }
            break;
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentList {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, PersistentList {
            head: s.head.clone_ref(py),
            tail: s.tail.clone_ref(py),
            count: s.count,
            meta: RwLock::new(m),
        })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentList {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        <PersistentList as ISeq>::cons(this, py, x)
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(empty_list(py).into_any())
    }
}

#[implements(IPersistentList)]
impl IPersistentList for PersistentList {}

#[implements(IPersistentStack)]
impl IPersistentStack for PersistentList {
    fn peek(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().head.clone_ref(py))
    }
    fn pop(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().tail.clone_ref(py))
    }
}

#[implements(Sequential)]
impl Sequential for PersistentList {}

// --- Python-facing constructor ---

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn list_(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
    if args.is_empty() {
        return Ok(empty_list(py).into_any());
    }
    // Build right-to-left.
    let mut tail: PyObject = empty_list(py).into_any();
    let mut count: u32 = 0;
    for i in (0..args.len()).rev() {
        let item = args.get_item(i)?.unbind();
        count += 1;
        let node = PersistentList {
            head: item,
            tail,
            count,
            meta: RwLock::new(None),
        };
        tail = Py::new(py, node)?.into_any();
    }
    Ok(tail)
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EmptyList>()?;
    m.add_class::<EmptyListIter>()?;
    m.add_class::<PersistentList>()?;
    m.add_class::<PersistentListIter>()?;
    m.add_function(wrap_pyfunction!(list_, m)?)?;

    let el = Py::new(py, EmptyList { meta: RwLock::new(None) })?;
    let _ = EMPTY_LIST.set(el);
    Ok(())
}
