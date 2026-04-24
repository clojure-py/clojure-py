//! Delay — cached single-evaluation thunk. `force` runs the thunk once and
//! memoizes the result; subsequent forces return the cached value.
//!
//! Unlike `LazySeq`, which allows concurrent re-entrance, `Delay` promises
//! exactly-once evaluation, so the mutex is held across the thunk call.

use crate::ideref::IDeref;
use clojure_core_macros::implements;
use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

enum DelayState {
    Unrealized(PyObject),     // the thunk
    Realized(PyObject),       // the cached value (possibly None)
    Failed(PyObject),         // the cached exception instance
}

#[pyclass(module = "clojure._core", name = "Delay", frozen)]
pub struct Delay {
    state: Mutex<DelayState>,
}

impl Delay {
    pub fn new(thunk: PyObject) -> Self {
        Self { state: Mutex::new(DelayState::Unrealized(thunk)) }
    }

    /// Force the delay: invoke the thunk if not yet realized, cache, and
    /// return the cached value. A thrown exception is cached too — subsequent
    /// forces re-throw the *same* exception instance (matches vanilla).
    pub fn force(&self, py: Python<'_>) -> PyResult<PyObject> {
        let mut g = self.state.lock();
        match &*g {
            DelayState::Realized(v) => Ok(v.clone_ref(py)),
            DelayState::Failed(e) => {
                Err(PyErr::from_value(e.clone_ref(py).into_bound(py)))
            }
            DelayState::Unrealized(thunk) => {
                let t = thunk.clone_ref(py);
                match crate::rt::invoke_n(py, t, &[]) {
                    Ok(v) => {
                        *g = DelayState::Realized(v.clone_ref(py));
                        Ok(v)
                    }
                    Err(e) => {
                        let val = e.value(py).clone().unbind().into_any();
                        *g = DelayState::Failed(val);
                        Err(e)
                    }
                }
            }
        }
    }

    pub fn is_realized(&self) -> bool {
        matches!(*self.state.lock(), DelayState::Realized(_) | DelayState::Failed(_))
    }
}

#[pymethods]
impl Delay {
    fn deref(slf: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        slf.bind(py).get().force(py)
    }

    fn realized(&self) -> bool {
        self.is_realized()
    }
}

#[implements(IDeref)]
impl IDeref for Delay {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        this.bind(py).get().force(py)
    }
}

#[pyfunction]
#[pyo3(name = "delay")]
pub fn py_delay(thunk: PyObject) -> Delay {
    Delay::new(thunk)
}

/// `(force x)` — if x is a Delay, force it; else return x.
#[pyfunction]
#[pyo3(name = "force")]
pub fn py_force(py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
    let b = x.bind(py);
    if let Ok(d) = b.cast::<Delay>() {
        return d.get().force(py);
    }
    Ok(x)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Delay>()?;
    m.add_function(wrap_pyfunction!(py_delay, m)?)?;
    m.add_function(wrap_pyfunction!(py_force, m)?)?;
    Ok(())
}
