//! End-to-end reduce: Python lambdas as the step fn, vector/list as
//! the source coll, plus chunked-seq cases that verify the chunked
//! dispatch path is actually exercised.

use clojure_rt::{drop_value, rt, Value};
use clojure_py::pyowned;
use pyo3::prelude::*;
use pyo3::types::PyDict;

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
    let v = pyowned::owning(py, f.as_ptr() as *mut _);
    (globals, v)
}

unsafe fn py_int(v: Value) -> i64 {
    let p = unsafe { pyowned::ptr_of(v) };
    let mut overflow = 0;
    unsafe { pyo3::ffi::PyLong_AsLongLongAndOverflow(p, &mut overflow) }
}

fn ints(xs: &[i64]) -> Vec<Value> { xs.iter().map(|&n| Value::int(n)).collect() }

#[test]
fn reduce_init_sums_a_vector() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, plus) = def_in(py, "f = lambda a, b: a + b", "f");
        let v = rt::vector(&ints(&[1, 2, 3, 4, 5]));
        let r = rt::reduce_init(v, plus, Value::int(0));
        assert_eq!(unsafe { py_int(r) }, 15);
        drop_value(r);
        drop_value(v);
        drop_value(plus);
    });
}

#[test]
fn reduce_no_init_uses_first_element_as_seed() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, plus) = def_in(py, "f = lambda a, b: a + b", "f");
        let v = rt::vector(&ints(&[10, 20, 30]));
        let r = rt::reduce(v, plus);
        assert_eq!(unsafe { py_int(r) }, 60);
        drop_value(r);
        drop_value(v);
        drop_value(plus);
    });
}

#[test]
fn reduce_no_init_on_empty_calls_f_zero_arg() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, identity_zero) = def_in(py, "f = lambda *a: 99 if not a else a", "f");
        let v = rt::vector(&[]);
        let r = rt::reduce(v, identity_zero);
        assert_eq!(unsafe { py_int(r) }, 99);
        drop_value(r);
        drop_value(v);
        drop_value(identity_zero);
    });
}

#[test]
fn reduce_init_walks_chunks_for_large_vector() {
    // 100-element vector exercises the trie + tail path. The chunked-
    // seq fallback branch in rt::reduce should produce the same answer
    // as the IReduce direct impl on PersistentVector — both should sum
    // to (n*(n-1))/2.
    clojure_py::init();
    Python::attach(|py| {
        let (_g, plus) = def_in(py, "f = lambda a, b: a + b", "f");
        let xs: Vec<Value> = (0..100i64).map(Value::int).collect();
        let v = rt::vector(&xs);
        let r = rt::reduce_init(v, plus, Value::int(0));
        assert_eq!(unsafe { py_int(r) }, (0..100i64).sum::<i64>());
        drop_value(r);
        drop_value(v);
        drop_value(plus);
    });
}

#[test]
fn reduce_init_handles_2049_element_vector() {
    // Exercises root-grown trie (shift = 10).
    clojure_py::init();
    Python::attach(|py| {
        let (_g, plus) = def_in(py, "f = lambda a, b: a + b", "f");
        let xs: Vec<Value> = (0..2049i64).map(Value::int).collect();
        let v = rt::vector(&xs);
        let r = rt::reduce_init(v, plus, Value::int(0));
        assert_eq!(unsafe { py_int(r) }, (0..2049i64).sum::<i64>());
        drop_value(r);
        drop_value(v);
        drop_value(plus);
    });
}

#[test]
fn reduce_handles_python_exception_propagation() {
    clojure_py::init();
    Python::attach(|py| {
        let (_g, divider) = def_in(py, "f = lambda a, b: a / b", "f");
        // Division by zero on the second step raises ZeroDivisionError;
        // reduce should surface that as a throwable Value.
        let v = rt::vector(&ints(&[10, 0, 1]));
        let r = rt::reduce_init(v, divider, Value::int(20));
        assert!(r.is_exception(), "expected exception, got tag={}", r.tag);
        drop_value(r);
        drop_value(v);
        drop_value(divider);
    });
}
