//! End-to-end: Rust calls Python callables through `IFn`. Mirrors the
//! `counted_pyobject.rs` style — a fresh Python interpreter per test
//! (PyO3 `auto-initialize`), `clojure_py::init()` once, then exercise
//! each arity slot.

use clojure_rt::{exception, protocol, rt, Value};
use clojure_rt::protocols::ifn::IFn;
use pyo3::ffi as pyffi;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict};

/// Run a Python snippet that defines bindings in a fresh dict, return
/// the named binding as a borrowed PyObject pointer wrapped in a
/// `Value::pyobject`. The PyDict outlives the returned Value via the
/// `globals_keep_alive` slot the caller chooses to retain.
fn def_in<'py>(py: Python<'py>, src: &str, name: &str)
    -> (Bound<'py, PyDict>, Value)
{
    let globals = PyDict::new(py);
    py.run(
        std::ffi::CString::new(src).unwrap().as_c_str(),
        Some(&globals),
        None,
    ).unwrap();
    let f = globals.get_item(name).unwrap().unwrap();
    let v = Value::pyobject(f.as_ptr() as *mut _);
    (globals, v)
}

/// Pull an `i64` back out of a Python-int Value. Useful for verifying
/// the return of a Python call without round-tripping through more
/// Clojure protocols than we've ported.
unsafe fn py_int(v: Value) -> i64 {
    let p = v.payload as *mut pyffi::PyObject;
    let mut overflow = 0;
    unsafe { pyffi::PyLong_AsLongLongAndOverflow(p, &mut overflow) }
}

#[test]
fn invoke_zero_arg_lambda() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda: 42", "f");
        let r = rt::invoke(f, &[]);
        assert_eq!(unsafe { py_int(r) }, 42);
    });
}

#[test]
fn invoke_one_arg_lambda() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda x: x + 1", "f");
        let r = rt::invoke(f, &[Value::int(7)]);
        assert_eq!(unsafe { py_int(r) }, 8);
    });
}

#[test]
fn invoke_two_arg_lambda() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda a, b: a * b", "f");
        let r = rt::invoke(f, &[Value::int(6), Value::int(7)]);
        assert_eq!(unsafe { py_int(r) }, 42);
    });
}

#[test]
fn invoke_three_arg_lambda() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda a, b, c: a + b * c", "f");
        let r = rt::invoke(f, &[Value::int(1), Value::int(2), Value::int(3)]);
        assert_eq!(unsafe { py_int(r) }, 7);
    });
}

#[test]
fn invoke_passes_through_pyobject_args() {
    // Verifies that an arg already shaped as `Value::pyobject` is
    // handed straight to the Python callable without conversion —
    // i.e. Python identity is preserved for opaque objects.
    clojure_py::init();
    Python::attach(|py| {
        let (_g, identity) = def_in(py, "f = lambda x: x", "f");
        let (_g2, opaque) = def_in(py, "obj = object()", "obj");
        let r = rt::invoke(identity, &[opaque]);
        // r should point at the same object as opaque.
        assert_eq!(r.payload, opaque.payload, "identity preserved");
    });
}

#[test]
fn invoke_propagates_python_exception() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda: 1/0", "f");
        let r = rt::invoke(f, &[]);
        assert!(r.is_exception(), "expected throwable, got tag={}", r.tag);
        let msg = exception::message(r).expect("payload");
        assert!(
            msg.contains("ZeroDivisionError"),
            "expected ZeroDivisionError in {msg}"
        );
        clojure_rt::drop_value(r);
    });
}

#[test]
fn invoke_passes_nil_as_python_none() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda x: x is None", "f");
        let r = rt::invoke(f, &[Value::NIL]);
        let p = r.payload as *mut pyffi::PyObject;
        assert_eq!(p, unsafe { pyffi::Py_True() });
    });
}

#[test]
fn invoke_passes_bool() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda b: not b", "f");
        let r_true  = rt::invoke(f, &[Value::TRUE]);
        let r_false = rt::invoke(f, &[Value::FALSE]);
        assert_eq!(r_true.payload  as *mut pyffi::PyObject, unsafe { pyffi::Py_False() });
        assert_eq!(r_false.payload as *mut pyffi::PyObject, unsafe { pyffi::Py_True()  });
    });
}

#[test]
fn invoke_passes_float() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda x: x * 2.0", "f");
        let r = rt::invoke(f, &[Value::float(1.5)]);
        let p = r.payload as *mut pyffi::PyObject;
        let got = unsafe { pyffi::PyFloat_AsDouble(p) };
        assert!((got - 3.0).abs() < 1e-9, "expected 3.0, got {got}");
    });
}

#[test]
fn satisfies_ifn_for_lambda_is_true() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, f) = def_in(py, "f = lambda: 1", "f");
        assert!(protocol::satisfies(&IFn::INVOKE_1, f));
    });
}

#[test]
fn satisfies_ifn_for_python_int_is_false() {
    clojure_py::init();
    Python::attach(|py| {
        let n = 42i64.into_pyobject(py).unwrap();
        let v = Value::pyobject(n.as_ptr() as *mut _);
        assert!(!protocol::satisfies(&IFn::INVOKE_1, v));
    });
}

#[test]
fn invoke_class_constructor_acts_as_callable() {
    // Python classes are also callable — ABCMeta's __subclasshook__
    // on Callable accepts anything with __call__, which classes have.
    clojure_py::init();
    Python::attach(|py| {
        let (_g, ctor) = def_in(
            py,
            "class C:\n    def __init__(self): self.x = 5\n",
            "C",
        );
        let r = rt::invoke(ctor, &[]);
        // r is now a `C()` instance — verify by getattr 'x' through Python.
        let p = r.payload as *mut pyffi::PyObject;
        let bound: Bound<'_, pyo3::types::PyAny> =
            unsafe { Bound::from_borrowed_ptr(py, p) };
        let x: i64 = bound.getattr("x").unwrap().extract().unwrap();
        assert_eq!(x, 5);
    });
}
