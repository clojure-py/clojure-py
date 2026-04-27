//! End-to-end tests for the `Counted` impl on `PyObject`. Runs under
//! `cargo test`, which links PyO3 with `auto-initialize` so a fresh
//! Python interpreter is created for each test process.

use clojure_py as _; // ensure inventory submissions in clojure_py link
use clojure_rt::{exception, init, rt, Value};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use pyo3::IntoPyObject;

#[test]
fn count_of_python_list_is_three() {
    init();
    Python::attach(|py| {
        let lst = PyList::new(py, [1i64, 2, 3]).unwrap();
        let v = Value::pyobject(lst.as_ptr() as *mut _);
        assert_eq!(rt::count(v).as_int(), Some(3));
    });
}

#[test]
fn count_of_python_string_is_unicode_length() {
    init();
    Python::attach(|py| {
        let s = PyString::new(py, "λclojure"); // 8 unicode codepoints
        let v = Value::pyobject(s.as_ptr() as *mut _);
        assert_eq!(rt::count(v).as_int(), Some(8));
    });
}

#[test]
fn count_of_python_int_returns_foreign_exception() {
    init();
    Python::attach(|py| {
        let n = 42i64.into_pyobject(py).unwrap();
        let v = Value::pyobject(n.as_ptr() as *mut _);
        let result = rt::count(v);

        assert!(result.is_exception(),
                "expected throwable Value, got tag={}", result.tag);
        assert_eq!(exception::kind(result), Some(exception::ExceptionKind::Foreign));

        let msg = exception::message(result).expect("exception payload missing");
        assert!(msg.contains("TypeError"),
                "exception message should name the originating Python TypeError, got: {msg}");
    });
}
