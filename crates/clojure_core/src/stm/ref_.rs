//! `Ref` — Clojure's coordinated-state reference type, used with `sync`/`dosync`
//! transactions. Under the hood: an MVCC history ring + per-ref `RwLock` for
//! commit-time serialization. Reads outside a transaction are lock-free (take
//! the head of the history). Reads inside a transaction walk the history for
//! a TVal with `point <= read_point` (see `stm::txn::do_get`).
//!
//! Notes:
//! - `addWatch` / `setValidator` etc. are exposed as duck-typed `#[pymethods]`,
//!   mirroring `Atom`. There is no formal `IRef` protocol in this port.
//! - History growth policy matches vanilla: a ref grows its history ring on
//!   retry-faults (when a reader misses a point). Trimmed on commit to
//!   `max_history`.

use crate::exceptions::IllegalStateException;
use crate::ideref::IDeref;
use crate::imeta::IMeta;
use arc_swap::ArcSwap;
use clojure_core_macros::implements;
use parking_lot::{Mutex, RwLock};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

type PyObject = Py<PyAny>;

pub const DEFAULT_MIN_HISTORY: usize = 0;
pub const DEFAULT_MAX_HISTORY: usize = 10;

pub struct TVal {
    pub point: u64,
    pub val: PyObject,
}

impl TVal {
    pub fn clone_ref(&self, py: Python<'_>) -> Self {
        Self { point: self.point, val: self.val.clone_ref(py) }
    }
}

pub struct History {
    /// Newest-first ring. `entries[0]` is the most recent TVal.
    pub entries: VecDeque<TVal>,
}

impl History {
    pub fn new() -> Self {
        Self { entries: VecDeque::new() }
    }

    pub fn head(&self) -> Option<&TVal> {
        self.entries.front()
    }

    /// Push a new committed TVal to the front and trim to `max`.
    pub fn push(&mut self, tv: TVal, min: usize, max: usize) {
        self.entries.push_front(tv);
        let target = max.max(min).max(1);
        while self.entries.len() > target {
            self.entries.pop_back();
        }
    }

    /// Walk the ring for the newest TVal with `point <= read_point`.
    pub fn find_at(&self, read_point: u64) -> Option<&TVal> {
        self.entries.iter().find(|tv| tv.point <= read_point)
    }
}

#[pyclass(module = "clojure._core", name = "Ref", frozen)]
pub struct Ref {
    /// Stable identity for sort order in commit lock-acquisition.
    pub id: u64,

    /// Per-ref commit lock. Write path holds `write()`; `ensure` holds `read()`.
    /// Never held across Python callback invocation.
    pub rw: RwLock<()>,

    /// MVCC history ring. Guarded by `Mutex` rather than the outer `rw` so
    /// readers outside a transaction don't contend with committers beyond the
    /// critical history swap.
    pub history: Mutex<History>,

    /// Count of retry-faults on this ref (transactions that missed an
    /// old-enough TVal). Drives history growth during commit.
    pub faults: AtomicU64,

    /// User-configurable bounds. AtomicUsize for lock-free reads.
    pub min_history: AtomicUsize,
    pub max_history: AtomicUsize,

    /// Most recent commit point, for fast barge/age checks.
    pub last_commit: AtomicU64,

    /// IRef bits — same shape as Atom.
    pub meta: ArcSwap<Option<PyObject>>,
    pub validator: ArcSwap<Option<PyObject>>,
    pub watches: RwLock<Py<PyDict>>,
}

impl Ref {
    pub fn new(py: Python<'_>, initial: PyObject) -> Self {
        let mut hist = History::new();
        hist.push(
            TVal { point: 0, val: initial },
            DEFAULT_MIN_HISTORY,
            DEFAULT_MAX_HISTORY,
        );
        Self {
            id: crate::rt::next_id(),
            rw: RwLock::new(()),
            history: Mutex::new(hist),
            faults: AtomicU64::new(0),
            min_history: AtomicUsize::new(DEFAULT_MIN_HISTORY),
            max_history: AtomicUsize::new(DEFAULT_MAX_HISTORY),
            last_commit: AtomicU64::new(0),
            meta: ArcSwap::new(Arc::new(None)),
            validator: ArcSwap::new(Arc::new(None)),
            watches: RwLock::new(PyDict::new(py).unbind()),
        }
    }

    /// Validate a proposed value against the installed validator fn.
    pub fn validate(&self, py: Python<'_>, v: &PyObject) -> PyResult<()> {
        let validator = {
            let g = self.validator.load();
            let opt: &Option<PyObject> = &g;
            opt.as_ref().map(|o| o.clone_ref(py))
        };
        if let Some(vf) = validator {
            let r = vf.bind(py).call1((v.clone_ref(py),))?;
            if !r.is_truthy()? {
                return Err(crate::exceptions::IllegalArgumentException::new_err(
                    "Invalid reference value",
                ));
            }
        }
        Ok(())
    }

    /// Install a new validator fn (or `None`) from Rust. Duck-typed
    /// equivalent of the `#[pymethods]` `set_validator` entry.
    pub fn install_validator(&self, py: Python<'_>, validator: Option<PyObject>) -> PyResult<()> {
        if let Some(vf) = validator.as_ref() {
            let cur = {
                let h = self.history.lock();
                h.head().map(|tv| tv.val.clone_ref(py))
            };
            if let Some(cur) = cur {
                let r = vf.bind(py).call1((cur,))?;
                if !r.is_truthy()? {
                    return Err(crate::exceptions::IllegalArgumentException::new_err(
                        "Invalid reference state",
                    ));
                }
            }
        }
        self.validator.store(Arc::new(validator));
        Ok(())
    }

    /// Current history length — Rust-callable accessor matching `#[pymethods]`.
    pub fn hist_count(&self) -> usize {
        self.history.lock().entries.len()
    }

    pub fn get_min_history(&self) -> usize {
        self.min_history.load(Ordering::Relaxed)
    }

    pub fn get_max_history(&self) -> usize {
        self.max_history.load(Ordering::Relaxed)
    }

    /// Snapshot + fire watches outside the lock (same pattern as Atom).
    pub fn fire_watches(
        &self,
        py: Python<'_>,
        slf: &Py<Ref>,
        old: PyObject,
        new: PyObject,
    ) -> PyResult<()> {
        let watches_snapshot: Vec<(PyObject, PyObject)> = {
            let guard = self.watches.read();
            guard.bind(py).iter().map(|(k, v)| (k.unbind(), v.unbind())).collect()
        };
        for (k, f) in watches_snapshot {
            f.bind(py).call1((
                k,
                slf.clone_ref(py),
                old.clone_ref(py),
                new.clone_ref(py),
            ))?;
        }
        Ok(())
    }
}

#[pymethods]
impl Ref {
    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let this = slf.bind(py).get();
        let head_val = {
            let h = this.history.lock();
            h.head().map(|tv| tv.val.clone_ref(py))
        };
        match head_val {
            Some(v) => {
                let s = v.bind(py).repr()?.extract::<String>()?;
                Ok(format!("#<Ref {}>", s))
            }
            None => Ok(String::from("#<Ref unbound>")),
        }
    }

    #[getter(meta)]
    fn get_meta(&self, py: Python<'_>) -> PyObject {
        let g = self.meta.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }

    fn set_validator(&self, py: Python<'_>, validator: Option<PyObject>) -> PyResult<()> {
        // If the current value fails the new validator, reject it.
        if let Some(vf) = validator.as_ref() {
            let cur = {
                let h = self.history.lock();
                h.head().map(|tv| tv.val.clone_ref(py))
            };
            if let Some(cur) = cur {
                let r = vf.bind(py).call1((cur,))?;
                if !r.is_truthy()? {
                    return Err(crate::exceptions::IllegalArgumentException::new_err(
                        "Invalid reference state",
                    ));
                }
            }
        }
        self.validator.store(Arc::new(validator));
        Ok(())
    }

    fn get_validator(&self, py: Python<'_>) -> Option<PyObject> {
        let g = self.validator.load();
        let opt: &Option<PyObject> = &g;
        opt.as_ref().map(|o| o.clone_ref(py))
    }

    fn add_watch(&self, py: Python<'_>, key: PyObject, f: PyObject) -> PyResult<()> {
        let guard = self.watches.read();
        guard.bind(py).set_item(key, f)?;
        Ok(())
    }

    fn remove_watch(&self, py: Python<'_>, key: PyObject) -> PyResult<()> {
        let guard = self.watches.read();
        guard.bind(py).del_item(key)?;
        Ok(())
    }

    /// Current history length — exposes `ref-history-count`.
    fn history_count(&self) -> usize {
        self.history.lock().entries.len()
    }

    #[getter(min_history)]
    fn min_history(&self) -> usize {
        self.min_history.load(Ordering::Relaxed)
    }

    fn set_min_history(slf: Py<Self>, py: Python<'_>, n: usize) -> Py<Self> {
        slf.bind(py).get().min_history.store(n, Ordering::Relaxed);
        slf
    }

    #[getter(max_history)]
    fn max_history(&self) -> usize {
        self.max_history.load(Ordering::Relaxed)
    }

    fn set_max_history(slf: Py<Self>, py: Python<'_>, n: usize) -> Py<Self> {
        slf.bind(py).get().max_history.store(n, Ordering::Relaxed);
        slf
    }
}

#[implements(IDeref)]
impl IDeref for Ref {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        // In a transaction — route through the MVCC read path.
        if let Some(txn) = crate::stm::txn::current() {
            return txn.do_get(py, &this);
        }
        // Outside — just return the head.
        let this_ref = this.bind(py).get();
        let h = this_ref.history.lock();
        match h.head() {
            Some(tv) => Ok(tv.val.clone_ref(py)),
            None => Err(IllegalStateException::new_err("Ref is unbound")),
        }
    }
}

#[implements(IMeta)]
impl IMeta for Ref {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let g = this.bind(py).get().meta.load();
        let opt: &Option<PyObject> = &g;
        Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        // Ref is a reference type; match Atom: store meta and return self.
        let r = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        r.meta.store(Arc::new(m));
        Ok(this.into_any())
    }
}

/// Python-side constructor: `clojure._core.ref(initial)`.
#[pyfunction]
#[pyo3(name = "ref")]
pub fn py_ref(py: Python<'_>, initial: PyObject) -> Ref {
    Ref::new(py, initial)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Ref>()?;
    m.add_function(wrap_pyfunction!(py_ref, m)?)?;
    Ok(())
}
