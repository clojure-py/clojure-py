//! Thread-local binding stack for dynamic vars.
//!
//! Each OS thread has its own stack of **binding frames**. A frame is a
//! `PersistentArrayMap` while small (<8 entries) and auto-promotes to a
//! `PersistentHashMap` past that — matching Clojure's own `hash-map`
//! literal behavior. Keys are `Var` instances; equality and hashing are
//! identity-based on Vars, so PAM's linear scan and PHM's HAMT both
//! degenerate to pointer comparison.
//!
//! `push_thread_bindings(assoc)` builds a new frame by `assoc`ing the
//! given associative's entries on top of the current frame (or an empty
//! frame if the stack is empty) and pushes. `pop_thread_bindings` pops.
//! `Var.deref()` on a dynamic var consults the top frame first.
//!
//! Under free-threaded 3.14t, each Python thread is an OS thread, so the
//! `thread_local!` TLS is per-Python-thread exactly as we want.

use crate::collections::parraymap::PersistentArrayMap;
use crate::collections::phashmap::PersistentHashMap;
use crate::exceptions::IllegalStateException;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use std::cell::RefCell;

type PyObject = Py<PyAny>;

/// A binding frame — either a PersistentArrayMap (small) or a PersistentHashMap
/// (after auto-promotion at 8 entries). Stored type-erased as `Py<PyAny>`.
pub type Frame = Py<PyAny>;

thread_local! {
    pub(crate) static BINDING_STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Fresh empty frame — a zero-entry PersistentArrayMap.
pub fn empty_frame(py: Python<'_>) -> PyResult<Frame> {
    Ok(Py::new(py, PersistentArrayMap::new_empty())?.into_any())
}

/// Look up `key` in `frame`, returning `default` if absent. Dispatches by
/// the concrete frame type without going through the protocol table.
pub fn frame_val_at_default(
    frame: &Frame,
    py: Python<'_>,
    key: &PyObject,
    default: PyObject,
) -> PyResult<PyObject> {
    let b = frame.bind(py);
    if let Ok(pam) = b.downcast::<PersistentArrayMap>() {
        return pam.get().val_at_default_internal(py, key.clone_ref(py), default);
    }
    if let Ok(phm) = b.downcast::<PersistentHashMap>() {
        return phm.get().val_at_default_internal(py, key.clone_ref(py), default);
    }
    Ok(default)
}

/// Does `frame` contain `key`?
pub fn frame_contains(frame: &Frame, py: Python<'_>, key: &PyObject) -> PyResult<bool> {
    let b = frame.bind(py);
    if let Ok(pam) = b.downcast::<PersistentArrayMap>() {
        return pam.get().contains_key_internal(py, key.clone_ref(py));
    }
    if let Ok(phm) = b.downcast::<PersistentHashMap>() {
        return phm.get().contains_key_internal(py, key.clone_ref(py));
    }
    Ok(false)
}

/// Assoc a (key, val) pair on a frame, returning a (possibly promoted)
/// new frame. The PAM path auto-promotes to PHM at `HASHMAP_THRESHOLD`.
pub fn frame_assoc(
    frame: &Frame,
    py: Python<'_>,
    key: PyObject,
    val: PyObject,
) -> PyResult<Frame> {
    let b = frame.bind(py);
    if let Ok(pam) = b.downcast::<PersistentArrayMap>() {
        // PAM.assoc_internal already returns a type-erased PyObject
        // (either a PAM or a newly-promoted PHM).
        return pam.get().assoc_internal(py, key, val);
    }
    if let Ok(phm) = b.downcast::<PersistentHashMap>() {
        let new = phm.get().assoc_internal(py, key, val)?;
        return Ok(Py::new(py, new)?.into_any());
    }
    Err(IllegalStateException::new_err(
        "binding frame has unexpected type",
    ))
}

/// Iterate `frame`'s (key, val) entries into a Vec. Used for
/// `get_thread_bindings` and frame-merging helpers.
pub fn frame_entries(frame: &Frame, py: Python<'_>) -> PyResult<Vec<(PyObject, PyObject)>> {
    let b = frame.bind(py);
    if let Ok(pam) = b.downcast::<PersistentArrayMap>() {
        return Ok(pam.get().collect_entries(py));
    }
    if let Ok(phm) = b.downcast::<PersistentHashMap>() {
        return Ok(phm.get().collect_entries(py));
    }
    Ok(Vec::new())
}

/// Push a new binding frame. Accepts any associative — a Clojure map
/// (`hash-map`, `array-map`, `sorted-map`, …) or a Python `dict`. Walked
/// via `rt::seq` into MapEntries (dict gets a fast direct-iterator path).
/// The resulting frame inherits all bindings from the current top frame
/// and overlays the given pairs.
#[pyfunction]
pub fn push_thread_bindings(py: Python<'_>, assoc: PyObject) -> PyResult<()> {
    let top: Frame = BINDING_STACK
        .with(|s| s.borrow().last().map(|f| f.clone_ref(py)))
        .unwrap_or_else(|| empty_frame(py).unwrap());
    let mut new_frame = top;

    let b = assoc.bind(py);
    if let Ok(d) = b.downcast::<PyDict>() {
        for (k, v) in d.iter() {
            validate_dynamic_key(py, &k)?;
            new_frame = frame_assoc(&new_frame, py, k.unbind(), v.unbind())?;
        }
    } else {
        let mut cur = crate::rt::seq(py, assoc)?;
        while !cur.is_none(py) {
            let entry = crate::rt::first(py, cur.clone_ref(py))?;
            let e_b = entry.bind(py);
            let k = e_b.get_item(0)?;
            let v: PyObject = e_b.get_item(1)?.unbind();
            validate_dynamic_key(py, &k)?;
            new_frame = frame_assoc(&new_frame, py, k.unbind(), v)?;
            cur = crate::rt::next_(py, cur)?;
        }
    }

    BINDING_STACK.with(|s| s.borrow_mut().push(new_frame));
    Ok(())
}

/// JVM parity (Var.java:326-327): reject non-dynamic Vars at push time.
/// Non-Var keys are left alone — policy stays whatever the frame store allows.
fn validate_dynamic_key(py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Ok(v) = key.downcast::<crate::var::Var>() {
        if !v.get().dynamic.load(std::sync::atomic::Ordering::Acquire) {
            let repr: String = key
                .repr()
                .and_then(|r| r.extract())
                .unwrap_or_else(|_| "<Var>".to_string());
            return Err(IllegalStateException::new_err(format!(
                "Can't dynamically bind non-dynamic var: {}",
                repr
            )));
        }
    }
    Ok(())
}

#[pyfunction]
pub fn pop_thread_bindings() -> PyResult<()> {
    BINDING_STACK.with(|s| {
        s.borrow_mut().pop();
    });
    Ok(())
}

/// Return the top binding frame as a Clojure map (PAM or PHM). Empty PAM
/// if the stack is empty.
#[pyfunction]
pub fn get_thread_bindings(py: Python<'_>) -> PyResult<PyObject> {
    let frame = BINDING_STACK.with(|s| s.borrow().last().map(|f| f.clone_ref(py)));
    match frame {
        Some(f) => Ok(f),
        None => empty_frame(py),
    }
}

/// Look up `var_py` in the top frame. Returns `None` if no binding for this var.
pub(crate) fn lookup_binding(py: Python<'_>, var_py: &PyObject) -> Option<PyObject> {
    BINDING_STACK.with(|s| {
        let stack = s.borrow();
        let top = stack.last()?;
        // Use a unique sentinel so we can distinguish "not present" from
        // "present with value nil".
        let sentinel: PyObject = pyo3::types::PyList::empty(py).unbind().into_any();
        let result = frame_val_at_default(top, py, var_py, sentinel.clone_ref(py)).ok()?;
        if crate::rt::identical(py, result.clone_ref(py), sentinel) {
            None
        } else {
            Some(result)
        }
    })
}

/// Mutate the top frame's entry for `var_py`. Implemented as
/// "assoc-and-replace-top"; `set!` on dynamic vars is semantically an
/// update-in-place from the caller's perspective but internally produces
/// a fresh frame (cheap: PAM clone is O(n) with n<8, PHM path share).
pub(crate) fn set_binding(py: Python<'_>, var_py: &PyObject, val: PyObject) -> PyResult<()> {
    BINDING_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        let top = stack.last().ok_or_else(|| {
            IllegalStateException::new_err("Can't set!: no binding frame")
        })?;
        // Require the var to have an existing binding in the top frame.
        if !frame_contains(top, py, var_py)? {
            return Err(IllegalStateException::new_err(
                "Can't set!: var has no thread-local binding",
            ));
        }
        let new_top = frame_assoc(top, py, var_py.clone_ref(py), val)?;
        let last_idx = stack.len() - 1;
        stack[last_idx] = new_top;
        Ok(())
    })
}

// --- Full-stack snapshot (binding-conveyor-fn / agents) --------------------

/// A snapshot of the full thread-binding stack. Opaque to Clojure code;
/// consumed only by `reset_thread_binding_frame`. Mirrors JVM's
/// `Var$Frame`.
#[pyclass(module = "clojure._core", name = "BindingFrame", frozen)]
pub struct BindingFrame {
    pub(crate) frames: Vec<Frame>,
}

/// Snapshot the current thread's entire binding stack. Returned frame can
/// be installed on any thread via `reset_thread_binding_frame`.
#[pyfunction]
pub fn clone_thread_binding_frame(py: Python<'_>) -> BindingFrame {
    let frames = BINDING_STACK.with(|s| {
        s.borrow()
            .iter()
            .map(|f| f.clone_ref(py))
            .collect::<Vec<_>>()
    });
    BindingFrame { frames }
}

/// Replace the current thread's binding stack with the given frame. Drops
/// any currently-pushed frames. Used on agent worker threads to install
/// the caller's conveyed binding frame.
#[pyfunction]
pub fn reset_thread_binding_frame(py: Python<'_>, frame: Py<BindingFrame>) -> PyResult<()> {
    let frames: Vec<Frame> = frame
        .bind(py)
        .get()
        .frames
        .iter()
        .map(|f| f.clone_ref(py))
        .collect();
    BINDING_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        *stack = frames;
    });
    Ok(())
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BindingFrame>()?;
    m.add_function(wrap_pyfunction!(push_thread_bindings, m)?)?;
    m.add_function(wrap_pyfunction!(pop_thread_bindings, m)?)?;
    m.add_function(wrap_pyfunction!(get_thread_bindings, m)?)?;
    m.add_function(wrap_pyfunction!(clone_thread_binding_frame, m)?)?;
    m.add_function(wrap_pyfunction!(reset_thread_binding_frame, m)?)?;
    Ok(())
}
