//! End-to-end tests for the per-Python-class Counted dispatch (ABCs as
//! inheritance metadata). Runs under `cargo test`, which links PyO3
//! with `auto-initialize` so a fresh Python interpreter is created for
//! each test process.

use clojure_rt::{drop_value, exception, protocol, rt};
use clojure_rt::protocols::counted::ICounted;
use clojure_py::pyowned;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};
use pyo3::IntoPyObject;

#[test]
fn count_of_python_list_is_three() {
    clojure_py::init();
    Python::attach(|py| {
        let lst = PyList::new(py, [1i64, 2, 3]).unwrap();
        let v = pyowned::owning(py, lst.as_ptr() as *mut _);
        assert_eq!(rt::count(v).as_int(), Some(3));
        drop_value(v);
    });
}

#[test]
fn count_of_python_string_is_unicode_length() {
    clojure_py::init();
    Python::attach(|py| {
        let s = PyString::new(py, "λclojure"); // 8 unicode codepoints
        let v = pyowned::owning(py, s.as_ptr() as *mut _);
        assert_eq!(rt::count(v).as_int(), Some(8));
        drop_value(v);
    });
}

#[test]
fn count_of_python_dict_is_pair_count() {
    clojure_py::init();
    Python::attach(|py| {
        let d = PyDict::new(py);
        d.set_item("a", 1).unwrap();
        d.set_item("b", 2).unwrap();
        let v = pyowned::owning(py, d.as_ptr() as *mut _);
        assert_eq!(rt::count(v).as_int(), Some(2));
        drop_value(v);
    });
}

#[test]
fn count_of_python_int_returns_no_impl_exception() {
    clojure_py::init();
    Python::attach(|py| {
        let n = 42i64.into_pyobject(py).unwrap();
        let v = pyowned::owning(py, n.as_ptr() as *mut _);
        let result = rt::count(v);

        assert!(result.is_exception(),
                "expected throwable Value, got tag={}", result.tag);
        // int doesn't subclass Sized; the resolver finds no impl, so
        // dispatch falls through to resolution_failure → NoProtocolImpl,
        // not a Foreign-from-TypeError leak.
        assert_eq!(
            exception::kind(result),
            Some(exception::ExceptionKind::NoProtocolImpl),
            "expected NoProtocolImpl; got {:?}",
            exception::kind(result),
        );
        let msg = exception::message(result).expect("exception payload missing");
        assert!(msg.contains("ICounted"), "message should name ICounted, got: {msg}");
        assert!(msg.contains("py:int"), "message should name py:int, got: {msg}");
        drop_value(result);
        drop_value(v);
    });
}

#[test]
fn satisfies_counted_for_python_list_is_true() {
    clojure_py::init();
    Python::attach(|py| {
        let lst = PyList::new(py, [1i64, 2, 3]).unwrap();
        let v = pyowned::owning(py, lst.as_ptr() as *mut _);
        assert!(protocol::satisfies(&ICounted::COUNT_1, v));
        drop_value(v);
    });
}

#[test]
fn satisfies_counted_for_python_int_is_false() {
    clojure_py::init();
    Python::attach(|py| {
        let n = 42i64.into_pyobject(py).unwrap();
        let v = pyowned::owning(py, n.as_ptr() as *mut _);
        assert!(!protocol::satisfies(&ICounted::COUNT_1, v));
        drop_value(v);
    });
}

#[test]
fn count_of_user_class_with_dunder_len_works_structurally() {
    clojure_py::init();
    Python::attach(|py| {
        // Define a class with __len__ via Python directly. ABCMeta's
        // __subclasshook__ on Sized makes any class with __len__ pass
        // isinstance(_, Sized), so the resolver inherits the impl.
        let globals = PyDict::new(py);
        py.run(
            std::ffi::CString::new(
                "class Custom:\n    def __len__(self): return 7\n"
            ).unwrap().as_c_str(),
            Some(&globals),
            None,
        ).unwrap();
        let cls = globals.get_item("Custom").unwrap().unwrap();
        let inst = cls.call0().unwrap();
        let v = pyowned::owning(py, inst.as_ptr() as *mut _);
        assert_eq!(rt::count(v).as_int(), Some(7));
        drop_value(v);
    });
}
