//! `ICounted` impl bound to the Python ABC `collections.abc.Sized`.
//!
//! Direct FFI: one `PyObject_Length` C call, no `Bound` construction,
//! no incref/decref pair. The borrowed semantics of
//! `Value(TYPE_PYOBJECT)` make this safe — the calling Python frame
//! owns the underlying ref for the dispatch's duration.
//!
//! Inheritance is handled by `crate::intern`: when a Python class is
//! first encountered, the resolver walks registered ABCs and copies
//! Sized's impls into the new class's per-type table. Result: `list`,
//! `dict`, `str`, and any user class with `__len__` — all dispatch
//! through this single function.

use clojure_rt::protocol::extend_type;
use clojure_rt::protocols::counted::ICounted;
use clojure_rt::value::{TypeId, Value};
use pyo3::ffi as pyffi;
use pyo3::{PyErr, Python};

unsafe extern "C" fn counted_count_via_pyobject_length(
    args: *const Value,
    _n: usize,
) -> Value {
    let this = unsafe { *args };
    let ptr = this.payload as *mut pyffi::PyObject;
    debug_assert!(!ptr.is_null(), "ICounted/count: null Python object pointer");

    Python::attach(|py| {
        let n = unsafe { pyffi::PyObject_Length(ptr) };
        if n < 0 {
            // PyObject_Length set the Python error indicator. Convert
            // to a throwable Foreign Value.
            let err = PyErr::take(py).expect("PyObject_Length set error indicator");
            return crate::exception::pyerr_to_value(py, err);
        }
        Value::int(n as i64)
    })
}

/// Install `ICounted/count` as the impl for the given Sized TypeId.
/// Called from `crate::abcs::init` once Sized has been interned.
pub fn install(sized_tid: TypeId) {
    extend_type(sized_tid, &ICounted::COUNT_1, counted_count_via_pyobject_length);
}
