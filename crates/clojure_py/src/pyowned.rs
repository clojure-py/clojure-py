//! Heap-allocated owning wrapper for a Python `*mut PyObject`. All
//! Python `Value`s in the runtime point at one of these — the per-class
//! `TypeId` (registered in `intern.rs`) tags the wrapper, the wrapper's
//! body holds the borrowed PyObject pointer, and our normal RCImmix
//! refcount manages the wrapper's lifetime. When the wrapper's
//! refcount hits zero, `pyowned_destruct` runs `Py_DECREF` on the
//! contained PyObject.
//!
//! The two construction APIs make ownership explicit at the FFI
//! boundary:
//!
//! - `owning(py, ptr)` — caller is "borrowing" the input from somewhere
//!   (a `#[pymodule]` arg, a borrowed PyO3 `Bound`, etc.); we incref
//!   to take our own owning ref.
//! - `taking(py, ptr)` — caller already holds an owning +1 (e.g., the
//!   return of `PyObject_Call`); we just record it. No incref.
//!
//! After construction, `dup`/`drop_value` on the resulting `Value`
//! flow through the normal `dup_heap`/`drop_heap` path. No special
//! casing on the rc hot path; non-Python callers pay nothing for this
//! existing.

use std::alloc::Layout;

use clojure_rt::header::Header;
use clojure_rt::value::Value;
use pyo3::ffi as pyffi;
use pyo3::Python;

/// Wrapper body: a single owned Python object pointer. The destructor
/// (`pyowned_destruct`) is what actually balances the +1 ref.
#[repr(C)]
pub struct PyOwnedBody {
    pub ptr: *mut pyffi::PyObject,
}

/// Layout of the wrapper body — used by `intern.rs` when minting per-
/// Python-class `TypeId`s so RCImmix knows how much space to allocate
/// for each Python Value.
pub fn body_layout() -> Layout {
    Layout::new::<PyOwnedBody>()
}

/// Destructor invoked by RCImmix when a Python Value's refcount hits
/// zero. Decrements the underlying Python ref. Free-threaded Python
/// 3.14 makes `Py_DECREF` thread-safe without GIL acquisition; on
/// non-no-GIL builds this would be unsound.
pub unsafe fn pyowned_destruct(h: *mut Header) {
    let body = unsafe { (h.add(1)) as *mut PyOwnedBody };
    let ptr = unsafe { (*body).ptr };
    if !ptr.is_null() {
        unsafe { pyffi::Py_DECREF(ptr); }
    }
}

/// Wrap a *borrowed* PyObject pointer as an owning Clojure `Value`.
/// We incref before recording so the wrapper holds its own reference,
/// independent of whatever owns the caller's borrow.
///
/// Looks up the Python class to resolve/mint a per-class `TypeId`;
/// allocations for that class go through RCImmix tagged with that
/// TypeId, so dispatch works the same as for any heap-typed Value.
pub fn owning(py: Python<'_>, ptr: *mut pyffi::PyObject) -> Value {
    debug_assert!(!ptr.is_null(), "pyowned::owning: null PyObject");
    unsafe { pyffi::Py_INCREF(ptr); }
    unsafe { wrap_unchecked(py, ptr) }
}

/// Wrap an *owned* PyObject pointer (caller already holds +1, e.g.
/// from `PyObject_Call`). The wrapper takes ownership without
/// increfing; its destructor balances with `Py_DECREF`.
pub fn taking(py: Python<'_>, ptr: *mut pyffi::PyObject) -> Value {
    debug_assert!(!ptr.is_null(), "pyowned::taking: null PyObject");
    unsafe { wrap_unchecked(py, ptr) }
}

/// Allocate a wrapper, store the pointer in its body, return the
/// tagged `Value`. Called by `owning` and `taking` after they've
/// reconciled the incref discipline.
unsafe fn wrap_unchecked(py: Python<'_>, ptr: *mut pyffi::PyObject) -> Value {
    let py_type = unsafe { (*ptr).ob_type } as *mut pyffi::PyObject;
    let tid = crate::intern::tid_for_pyclass(py, py_type);
    unsafe {
        let h = clojure_rt::gc::rcimmix::RCIMMIX.alloc_inline(body_layout(), tid);
        let body = h.add(1) as *mut PyOwnedBody;
        std::ptr::write(body, PyOwnedBody { ptr });
        Value::from_heap(h)
    }
}

/// Extract the `*mut PyObject` from a Value previously constructed
/// via `owning`/`taking`. Used by IFn / Counted / etc. impl bodies
/// that need to call into the Python C API.
///
/// # Safety
/// `v` must be a Value of a per-Python-class TypeId (i.e. the result
/// of `owning` or `taking`). Calling on any other Value tag is UB.
#[inline]
pub unsafe fn ptr_of(v: Value) -> *mut pyffi::PyObject {
    let h = v.as_heap().expect("pyowned::ptr_of: not a heap Value");
    let body = unsafe { (h.add(1)) as *const PyOwnedBody };
    unsafe { (*body).ptr }
}
