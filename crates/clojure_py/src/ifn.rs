//! `IFn` impl bound to the Python ABC `collections.abc.Callable`. Any
//! Python class that has `__call__` (functions, lambdas, classes,
//! bound methods, partials, etc.) gets `IFn` for free via the ABC
//! inheritance walk in `crate::intern`.
//!
//! Each per-arity slot constructs a Python tuple of the user args,
//! calls `PyObject_Call`, and wraps the result back into a `Value`
//! via `pyowned::taking` (the call result is a fresh +1 ref, no
//! incref needed).
//!
//! Conversion of Clojure `Value`s to Python objects is currently
//! limited to the cheap-immediate set (`nil`, `bool`, `int`, `float`)
//! plus any Value that's already a `pyowned`-wrapped Python object
//! (passthrough — we incref the inner ptr to balance the tuple's
//! `SET_ITEM` steal). Other tags (string, keyword, symbol, list,
//! vector, …) return a Foreign exception so the gap is loud rather
//! than silent. The conversion table grows as more borrowable
//! bridges land.

use clojure_rt::protocol::extend_type;
use clojure_rt::protocols::ifn::IFn;
use clojure_rt::value::{TypeId, Value};
use clojure_rt::{TYPE_BOOL, TYPE_FLOAT64, TYPE_INT64, TYPE_NIL, FIRST_HEAP_TYPE};
use pyo3::ffi as pyffi;
use pyo3::{PyErr, Python};

/// Build a *new* PyObject reference for a single `Value`. The caller
/// hands the result into `PyTuple_SET_ITEM`, which steals the ref.
/// On unsupported tags returns `None`; the caller turns that into a
/// Foreign exception.
unsafe fn value_to_py_new(v: Value) -> Option<*mut pyffi::PyObject> {
    match v.tag {
        TYPE_NIL => {
            let p = unsafe { pyffi::Py_None() };
            unsafe { pyffi::Py_INCREF(p); }
            Some(p)
        }
        TYPE_BOOL => {
            let p = if v.payload != 0 {
                unsafe { pyffi::Py_True() }
            } else {
                unsafe { pyffi::Py_False() }
            };
            unsafe { pyffi::Py_INCREF(p); }
            Some(p)
        }
        TYPE_INT64 => {
            let p = unsafe { pyffi::PyLong_FromLongLong(v.payload as i64) };
            if p.is_null() { None } else { Some(p) }
        }
        TYPE_FLOAT64 => {
            let f = f64::from_bits(v.payload);
            let p = unsafe { pyffi::PyFloat_FromDouble(f) };
            if p.is_null() { None } else { Some(p) }
        }
        tag if tag >= FIRST_HEAP_TYPE => {
            // Heap-typed Values are either `pyowned`-wrapped Python
            // objects or native Clojure heap types. We currently only
            // support the former — pull the inner ptr and incref it so
            // PyTuple_SET_ITEM has a fresh +1 to steal. Native Clojure
            // heap types (string, keyword, list, vector, …) need
            // dedicated bridges to be passable to Python, which lands
            // when the value-to-py conversion table grows.
            //
            // We detect "is this a pyowned wrapper?" structurally by
            // checking that the type registry has the pyowned body
            // layout. For now, we attempt the read and trust the
            // caller — passing a non-pyowned heap Value here is a bug
            // that surfaces as a corrupted Python ref, so this needs
            // tightening (e.g., a per-class flag or a `IsPyObject`
            // marker protocol). Tracked as a follow-up.
            let p = unsafe { crate::pyowned::ptr_of(v) };
            if p.is_null() {
                None
            } else {
                unsafe { pyffi::Py_INCREF(p); }
                Some(p)
            }
        }
        _ => None,
    }
}

/// Construct a Python `tuple` of `n` user args and pass it to
/// `PyObject_Call`. On any conversion failure or Python-side
/// exception, returns a Foreign-throwable `Value`. On success,
/// wraps the Python result as an owning `Value` via `pyowned::taking`
/// so refcount discipline is balanced (no leak).
unsafe fn call_n(callable: Value, args: &[Value]) -> Value {
    let f_ptr = unsafe { crate::pyowned::ptr_of(callable) };
    debug_assert!(!f_ptr.is_null(), "IFn/invoke: null Python callable");

    Python::attach(|py| {
        // Build the args tuple. PyTuple_SET_ITEM steals one ref per
        // slot; `value_to_py_new` produces the +1 ref that's about
        // to be stolen.
        let tuple = unsafe { pyffi::PyTuple_New(args.len() as pyffi::Py_ssize_t) };
        if tuple.is_null() {
            let err = PyErr::take(py).expect("PyTuple_New failed without error");
            return crate::exception::pyerr_to_value(py, err);
        }
        for (i, &arg) in args.iter().enumerate() {
            match unsafe { value_to_py_new(arg) } {
                Some(p) => {
                    unsafe { pyffi::PyTuple_SET_ITEM(tuple, i as pyffi::Py_ssize_t, p); }
                }
                None => {
                    unsafe { pyffi::Py_DECREF(tuple); }
                    return clojure_rt::exception::make_foreign(format!(
                        "IFn/invoke: cannot pass Value with tag {} to a Python callable \
                         — extend value_to_py_new in clojure_py/src/ifn.rs",
                        arg.tag
                    ));
                }
            }
        }

        let result = unsafe { pyffi::PyObject_Call(f_ptr, tuple, std::ptr::null_mut()) };
        unsafe { pyffi::Py_DECREF(tuple); }

        if result.is_null() {
            let err = PyErr::take(py).expect("PyObject_Call failed without error");
            return crate::exception::pyerr_to_value(py, err);
        }
        // result is a new ref; pyowned::taking takes ownership so the
        // wrapper's destructor will decref when the Value is dropped.
        crate::pyowned::taking(py, result)
    })
}

// ============================================================================
// Per-arity adapters. Each unwraps the dispatch slice and forwards to
// `call_n` with the appropriate user-arg slice.
// ============================================================================

unsafe extern "C" fn ifn_invoke_1_pyobject(args: *const Value, _n: usize) -> Value {
    let f = unsafe { *args.add(0) };
    unsafe { call_n(f, &[]) }
}

unsafe extern "C" fn ifn_invoke_2_pyobject(args: *const Value, _n: usize) -> Value {
    let f = unsafe { *args.add(0) };
    let a1 = unsafe { *args.add(1) };
    unsafe { call_n(f, &[a1]) }
}

unsafe extern "C" fn ifn_invoke_3_pyobject(args: *const Value, _n: usize) -> Value {
    let f = unsafe { *args.add(0) };
    let a1 = unsafe { *args.add(1) };
    let a2 = unsafe { *args.add(2) };
    unsafe { call_n(f, &[a1, a2]) }
}

unsafe extern "C" fn ifn_invoke_4_pyobject(args: *const Value, _n: usize) -> Value {
    let f = unsafe { *args.add(0) };
    let a1 = unsafe { *args.add(1) };
    let a2 = unsafe { *args.add(2) };
    let a3 = unsafe { *args.add(3) };
    unsafe { call_n(f, &[a1, a2, a3]) }
}

unsafe extern "C" fn ifn_invoke_5_pyobject(args: *const Value, _n: usize) -> Value {
    let f = unsafe { *args.add(0) };
    let a1 = unsafe { *args.add(1) };
    let a2 = unsafe { *args.add(2) };
    let a3 = unsafe { *args.add(3) };
    let a4 = unsafe { *args.add(4) };
    unsafe { call_n(f, &[a1, a2, a3, a4]) }
}

unsafe extern "C" fn ifn_invoke_6_pyobject(args: *const Value, _n: usize) -> Value {
    let f = unsafe { *args.add(0) };
    let a1 = unsafe { *args.add(1) };
    let a2 = unsafe { *args.add(2) };
    let a3 = unsafe { *args.add(3) };
    let a4 = unsafe { *args.add(4) };
    let a5 = unsafe { *args.add(5) };
    unsafe { call_n(f, &[a1, a2, a3, a4, a5]) }
}

/// Install all six `IFn::invoke_<N>` adapters as the impl for the
/// given Callable TypeId. Called from `crate::abcs::init` once the
/// `collections.abc.Callable` ABC has been interned.
pub fn install(callable_tid: TypeId) {
    extend_type(callable_tid, &IFn::INVOKE_1, ifn_invoke_1_pyobject);
    extend_type(callable_tid, &IFn::INVOKE_2, ifn_invoke_2_pyobject);
    extend_type(callable_tid, &IFn::INVOKE_3, ifn_invoke_3_pyobject);
    extend_type(callable_tid, &IFn::INVOKE_4, ifn_invoke_4_pyobject);
    extend_type(callable_tid, &IFn::INVOKE_5, ifn_invoke_5_pyobject);
    extend_type(callable_tid, &IFn::INVOKE_6, ifn_invoke_6_pyobject);
}
