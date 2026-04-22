//! Thread-local binding stack for dynamic vars.
//!
//! Each OS thread has its own stack. `push_thread_bindings(map)` creates a
//! new frame that is `(top-frame merge map)` and pushes it. `pop_thread_bindings`
//! pops. `Var.deref()` on a dynamic var consults the top frame first.
//!
//! Under free-threaded 3.14t, each Python thread is an OS thread, so the
//! `thread_local!` TLS is per-Python-thread exactly as we want.

use crate::exceptions::IllegalStateException;
use crate::binding_pmap::PMap;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use std::cell::RefCell;

type PyObject = Py<PyAny>;

thread_local! {
    pub(crate) static BINDING_STACK: RefCell<Vec<PMap>> = const { RefCell::new(Vec::new()) };
}

#[pyfunction]
pub fn push_thread_bindings(py: Python<'_>, map: Bound<'_, PyDict>) -> PyResult<()> {
    let top = BINDING_STACK.with(|s| s.borrow().last().cloned().unwrap_or_default());
    let mut new_frame = top;
    for (k, v) in map.iter() {
        let k_obj: PyObject = k.unbind();
        let v_obj: PyObject = v.unbind();
        new_frame = new_frame.assoc(py, &k_obj, v_obj);
    }
    BINDING_STACK.with(|s| s.borrow_mut().push(new_frame));
    Ok(())
}

#[pyfunction]
pub fn pop_thread_bindings() -> PyResult<()> {
    BINDING_STACK.with(|s| {
        s.borrow_mut().pop();
    });
    Ok(())
}

/// Look up `var_py` in the top frame. Returns `None` if no binding for this var.
pub(crate) fn lookup_binding(py: Python<'_>, var_py: &PyObject) -> Option<PyObject> {
    BINDING_STACK.with(|s| {
        let stack = s.borrow();
        stack
            .last()
            .and_then(|frame| frame.get(var_py).map(|v| v.clone_ref(py)))
    })
}

/// Mutate the current (top) frame's entry for `var_py`. Error if no frame
/// exists or the var has no entry in the current frame.
pub(crate) fn set_binding(py: Python<'_>, var_py: &PyObject, val: PyObject) -> PyResult<()> {
    BINDING_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        let top = stack.last_mut().ok_or_else(|| {
            IllegalStateException::new_err("Can't set!: no binding frame")
        })?;
        if !top.update_in_place(py, var_py, val) {
            return Err(IllegalStateException::new_err(
                "Can't set!: var has no thread-local binding",
            ));
        }
        Ok(())
    })
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(push_thread_bindings, m)?)?;
    m.add_function(wrap_pyfunction!(pop_thread_bindings, m)?)?;
    Ok(())
}
