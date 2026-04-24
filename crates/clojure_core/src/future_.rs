//! `Future` and `Promise` — deferred-value reference types.
//!
//! `Future` wraps a 0-arg Clojure callable and dispatches it on the same
//! thread pool used by `send-off` (`agent::future_pool()`). The result (or
//! exception) is parked in the future's state slot; `deref` blocks until
//! ready.
//!
//! `Promise` is a one-shot box without backing computation: `(promise)`
//! returns an unrealized promise, `(deliver p v)` fulfills it (idempotent),
//! `(deref p)` blocks until delivered.
//!
//! Both implement `IDeref` and `IPending` (`realized?`).

use crate::ideref::IDeref;
use crate::ipending::IPending;
use clojure_core_macros::implements;
use parking_lot::{Condvar, Mutex};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

// ============================================================================
// Future
// ============================================================================

enum FutureState {
    Pending,
    Done(PyObject),
    Failed(PyObject), // the exception object
    Cancelled,
}

struct FutureInner {
    state: Mutex<FutureState>,
    cond: Condvar,
}

#[pyclass(module = "clojure._core", name = "Future", frozen)]
pub struct Future {
    inner: Arc<FutureInner>,
}

impl Future {
    /// Rust-callable equivalents of the pymethods (those are Python-only
    /// when not declared `pub`).
    pub fn is_done(&self) -> bool {
        !matches!(*self.inner.state.lock(), FutureState::Pending)
    }
    pub fn is_cancelled(&self) -> bool {
        matches!(*self.inner.state.lock(), FutureState::Cancelled)
    }
    pub fn try_cancel(&self) -> bool {
        let mut st = self.inner.state.lock();
        if matches!(*st, FutureState::Pending) {
            *st = FutureState::Cancelled;
            self.inner.cond.notify_all();
            true
        } else {
            false
        }
    }

    /// Construct a Future that will run `f` on the future pool.
    /// `f` must be callable with no arguments.
    pub fn spawn(py: Python<'_>, f: PyObject) -> PyResult<Self> {
        let inner = Arc::new(FutureInner {
            state: Mutex::new(FutureState::Pending),
            cond: Condvar::new(),
        });
        let inner_for_worker = inner.clone();
        let f_for_worker = f;
        crate::agent::future_pool().execute(Box::new(move || {
            // Skip work if already cancelled.
            {
                let st = inner_for_worker.state.lock();
                if matches!(*st, FutureState::Cancelled) {
                    inner_for_worker.cond.notify_all();
                    return;
                }
            }
            // Invoke the callable under GIL; trap any error.
            let final_state = Python::attach(|py| match crate::rt::invoke_n(py, f_for_worker, &[]) {
                Ok(v) => FutureState::Done(v),
                Err(e) => FutureState::Failed(e.into_value(py).into_any()),
            });
            // Don't overwrite a Cancelled state set after dispatch but before
            // we got here.
            let mut st = inner_for_worker.state.lock();
            if matches!(*st, FutureState::Cancelled) {
                inner_for_worker.cond.notify_all();
                return;
            }
            *st = final_state;
            inner_for_worker.cond.notify_all();
        }));
        let _ = py;
        Ok(Future { inner })
    }
}

#[pymethods]
impl Future {
    /// True iff the future has reached any terminal state.
    #[getter]
    fn done(&self) -> bool {
        !matches!(*self.inner.state.lock(), FutureState::Pending)
    }

    /// True iff the future was cancelled before it ran (or its result discarded).
    #[getter]
    fn cancelled(&self) -> bool {
        matches!(*self.inner.state.lock(), FutureState::Cancelled)
    }

    /// Mark cancelled. The work itself isn't actually interrupted — Python
    /// has no cooperative-thread-cancel — but a subsequent `deref` raises,
    /// `cancelled?` returns true, and any result that arrives is discarded.
    /// Returns true if the cancellation took effect (i.e. we were Pending),
    /// false otherwise.
    fn cancel(&self) -> bool {
        let mut st = self.inner.state.lock();
        if matches!(*st, FutureState::Pending) {
            *st = FutureState::Cancelled;
            self.inner.cond.notify_all();
            true
        } else {
            false
        }
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> String {
        let s = slf.bind(py).get();
        match &*s.inner.state.lock() {
            FutureState::Pending => "#<Future :pending>".to_string(),
            FutureState::Done(_) => "#<Future :done>".to_string(),
            FutureState::Failed(_) => "#<Future :failed>".to_string(),
            FutureState::Cancelled => "#<Future :cancelled>".to_string(),
        }
    }
}

#[implements(IDeref)]
impl IDeref for Future {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let inner = this.bind(py).get().inner.clone();
        // Block (GIL released) until the worker reaches a terminal state.
        py.detach(|| {
            let mut st = inner.state.lock();
            while matches!(*st, FutureState::Pending) {
                inner.cond.wait(&mut st);
            }
        });
        // Re-acquire the lock under GIL and act on the now-frozen state.
        let st = inner.state.lock();
        match &*st {
            FutureState::Done(v) => Ok(v.clone_ref(py)),
            FutureState::Failed(e) => {
                Err(PyErr::from_value(e.bind(py).clone()))
            }
            FutureState::Cancelled => Err(
                crate::exceptions::IllegalStateException::new_err("Future was cancelled"),
            ),
            FutureState::Pending => unreachable!(),
        }
    }
}

#[implements(IPending)]
impl IPending for Future {
    fn is_realized(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let realized = !matches!(*s.inner.state.lock(), FutureState::Pending);
        Ok(pyo3::types::PyBool::new(py, realized).to_owned().unbind().into_any())
    }
}

// ============================================================================
// Promise
// ============================================================================

struct PromiseInner {
    state: Mutex<Option<PyObject>>,
    cond: Condvar,
}

#[pyclass(module = "clojure._core", name = "Promise", frozen)]
pub struct Promise {
    inner: Arc<PromiseInner>,
}

impl Promise {
    /// Public constructor — pymethods don't expose `new` outside Python.
    pub fn create() -> Self {
        Self {
            inner: Arc::new(PromiseInner {
                state: Mutex::new(None),
                cond: Condvar::new(),
            }),
        }
    }

    /// Public deliver — pymethods version is Python-only.
    pub fn try_deliver(slf: Py<Self>, py: Python<'_>, v: PyObject) -> PyObject {
        let inner = slf.bind(py).get().inner.clone();
        let mut st = inner.state.lock();
        if st.is_some() {
            return py.None();
        }
        *st = Some(v);
        inner.cond.notify_all();
        slf.into_any()
    }
}

#[pymethods]
impl Promise {
    #[new]
    fn py_new() -> Self {
        Self::create()
    }

    fn deliver(slf: Py<Self>, py: Python<'_>, v: PyObject) -> PyObject {
        Self::try_deliver(slf, py, v)
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> String {
        let s = slf.bind(py).get();
        if s.inner.state.lock().is_some() {
            "#<Promise :delivered>".to_string()
        } else {
            "#<Promise :pending>".to_string()
        }
    }
}

#[implements(IDeref)]
impl IDeref for Promise {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let inner = this.bind(py).get().inner.clone();
        // Wait (GIL released). Promise state is monotonic — once Some, never
        // back to None — so we can re-lock under GIL afterwards safely.
        py.detach(|| {
            let mut st = inner.state.lock();
            while st.is_none() {
                inner.cond.wait(&mut st);
            }
        });
        let st = inner.state.lock();
        Ok(st.as_ref().expect("delivered").clone_ref(py))
    }
}

#[implements(IPending)]
impl IPending for Promise {
    fn is_realized(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let realized = s.inner.state.lock().is_some();
        Ok(pyo3::types::PyBool::new(py, realized).to_owned().unbind().into_any())
    }
}

// ============================================================================
// Module registration
// ============================================================================

#[pyfunction]
#[pyo3(name = "future_call")]
pub fn future_call(py: Python<'_>, f: PyObject) -> PyResult<Future> {
    Future::spawn(py, f)
}

#[pyfunction]
#[pyo3(name = "promise")]
pub fn promise() -> Promise {
    Promise::create()
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Future>()?;
    m.add_class::<Promise>()?;
    m.add_function(wrap_pyfunction!(future_call, m)?)?;
    m.add_function(wrap_pyfunction!(promise, m)?)?;
    Ok(())
}
