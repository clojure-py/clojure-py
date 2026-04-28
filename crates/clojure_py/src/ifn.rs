//! `IFn` impl bound to the Python ABC `collections.abc.Callable`. Any
//! Python class that has `__call__` (functions, lambdas, classes,
//! bound methods, partials, etc.) gets `IFn` for free via the ABC
//! inheritance walk in `crate::intern`.
//!
//! Each per-arity slot constructs a Python tuple of the user args,
//! calls `PyObject_Call`, and wraps the result back into a `Value`.
//! Conversion of Clojure `Value`s to Python objects is currently
//! limited to the cheap-immediate set (`nil`, `bool`, `int`, `float`,
//! `pyobject`); other tags return a Foreign exception so the gap is
//! loud rather than silently producing garbage. The conversion table
//! grows as we add more Python-borrowable types (string, char, etc.).
//!
//! **Refcount caveat.** `Value::pyobject(...)` documents borrowed
//! semantics: the Value does not incref/decref the pointed-to
//! PyObject. Results returned from `PyObject_Call` are *new* refs,
//! so we presently leak them — every cross-language call leaks one
//! Python ref. This is the same gap noted in `value.rs`: "the owning
//! variant arrives once a protocol port needs PyObject storage."
//! Tests in this slice are short-lived processes; the leak is
//! bounded. Fixing it is its own substrate slice.

use clojure_rt::protocol::extend_type;
use clojure_rt::protocols::ifn::IFn;
use clojure_rt::value::{TypeId, Value};
use clojure_rt::{TYPE_BOOL, TYPE_FLOAT64, TYPE_INT64, TYPE_NIL, TYPE_PYOBJECT};
use pyo3::ffi as pyffi;
use pyo3::{PyErr, Python};

/// Build a borrowed-PyObject for a single `Value`. On unsupported
/// tags returns `None`; the caller turns this into a Foreign
/// exception. The returned pointer's lifetime tracks the underlying
/// Value's, so callers must wrap it in the call's tuple before any
/// further conversion that might invalidate the source.
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
        TYPE_PYOBJECT => {
            let p = v.payload as *mut pyffi::PyObject;
            // We're handing this into a tuple via `PyTuple_SET_ITEM`,
            // which steals one ref. Bump first so the Value's
            // (borrowed) reference isn't consumed.
            unsafe { pyffi::Py_INCREF(p); }
            Some(p)
        }
        _ => None,
    }
}

/// Construct a Python `tuple` of `n` user args and pass it to
/// `PyObject_Call`. On any conversion failure or Python-side
/// exception, returns a Foreign-throwable `Value`. On success,
/// wraps the Python result as `Value::pyobject` (which leaks one
/// ref — see module-level note).
unsafe fn call_n(callable: Value, args: &[Value]) -> Value {
    let f_ptr = callable.payload as *mut pyffi::PyObject;
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
        // result is a new ref; we hand it out as a borrowed-semantics
        // Value::pyobject and leak this ref (see module note).
        Value::pyobject(result as *mut _)
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

