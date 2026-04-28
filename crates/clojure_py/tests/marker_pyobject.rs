//! Marker-protocol bindings for Python ABCs:
//! - `collections.abc.Set`     → `IPersistentSet`  (for `(set? x)`)
//! - `collections.abc.Mapping` → `IPersistentMap`  (for `(map? x)`)
//!
//! Pure predicate affordances — mutating ops (disjoin, dissoc) stay
//! unimplemented; these registrations only let `satisfies?` answer
//! the way a Clojure user expects.

use clojure_rt::{drop_value, protocol};
use clojure_rt::protocols::persistent_map::IPersistentMap;
use clojure_rt::protocols::persistent_set::IPersistentSet;
use clojure_py::pyowned;
use pyo3::ffi as pyffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

#[test]
fn python_set_satisfies_persistent_set() {
    clojure_py::init();
    Python::attach(|py| {
        let s = unsafe { pyffi::PySet_New(std::ptr::null_mut()) };
        assert!(!s.is_null());
        let v = pyowned::owning(py, s);
        assert!(protocol::satisfies(&IPersistentSet::MARKER, v));
        drop_value(v);
    });
}

#[test]
fn python_frozenset_satisfies_persistent_set() {
    clojure_py::init();
    Python::attach(|py| {
        let s = unsafe { pyffi::PyFrozenSet_New(std::ptr::null_mut()) };
        assert!(!s.is_null());
        let v = pyowned::owning(py, s);
        assert!(protocol::satisfies(&IPersistentSet::MARKER, v));
        drop_value(v);
    });
}

#[test]
fn python_dict_satisfies_persistent_map() {
    clojure_py::init();
    Python::attach(|py| {
        let d = PyDict::new(py);
        let v = pyowned::owning(py, d.as_ptr() as *mut _);
        assert!(protocol::satisfies(&IPersistentMap::MARKER, v));
        drop_value(v);
    });
}

#[test]
fn python_list_does_not_satisfy_persistent_set_or_map() {
    clojure_py::init();
    Python::attach(|py| {
        let lst = PyList::new(py, [1i64, 2, 3]).unwrap();
        let v = pyowned::owning(py, lst.as_ptr() as *mut _);
        assert!(!protocol::satisfies(&IPersistentSet::MARKER, v));
        assert!(!protocol::satisfies(&IPersistentMap::MARKER, v));
        drop_value(v);
    });
}

#[test]
fn user_class_registered_to_set_abc_satisfies_persistent_set() {
    clojure_py::init();
    Python::attach(|py| {
        // Subclass abc.Set structurally — supplying the three
        // abstract methods lets ABCMeta accept the class.
        let globals = PyDict::new(py);
        py.run(
            std::ffi::CString::new(
                "from collections.abc import Set\n\
                 class MySet(Set):\n    \
                     def __init__(self): self._d = set()\n    \
                     def __contains__(self, x): return x in self._d\n    \
                     def __iter__(self): return iter(self._d)\n    \
                     def __len__(self): return len(self._d)\n"
            ).unwrap().as_c_str(),
            Some(&globals),
            None,
        ).unwrap();
        let cls = globals.get_item("MySet").unwrap().unwrap();
        let inst = cls.call0().unwrap();
        let v = pyowned::owning(py, inst.as_ptr() as *mut _);
        assert!(protocol::satisfies(&IPersistentSet::MARKER, v));
        drop_value(v);
    });
}
