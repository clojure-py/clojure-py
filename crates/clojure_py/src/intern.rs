//! Lazy interning of Python classes as `clojure_rt` `TypeId`s, plus
//! per-class inheritance walks (MRO + registered ABCs).
//!
//! On first encounter of a Python class at a dispatch site, the class
//! is registered as a dynamic type in `clojure_rt::type_registry` and
//! the resulting `TypeId` is cached in `PY_TYPE_TABLE`. Subsequent
//! dispatches on the same class hit the IC at full speed, like any
//! Clojure-native type.
//!
//! Inheritance: when a class is interned, the resolver walks every
//! registered ABC and copies impl entries from any ABC the class is a
//! subclass of (per `PyObject_IsSubclass`, which respects ABCMeta's
//! `__subclasshook__`). Result: `extend Counted to Sized` automatically
//! covers `list`, `dict`, `str`, and any user class with `__len__`.

use std::alloc::Layout;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use clojure_rt::dispatch::foreign;
use clojure_rt::dispatch::perfect_hash::PerTypeTable;
use clojure_rt::header::Header;
use clojure_rt::type_registry;
use clojure_rt::value::TypeId;
use clojure_rt::TYPE_PYOBJECT;
use pyo3::ffi as pyffi;
use pyo3::types::{PyType, PyTypeMethods};
use pyo3::{Bound, PyAny, Python};

/// `*mut PyTypeObject` (stored as `usize` because raw pointers aren't
/// `Send`/`Sync`) → `clojure_rt` `TypeId`.
static PY_TYPE_TABLE: LazyLock<RwLock<HashMap<usize, TypeId>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Python ABCs (or any class meant to act as inheritance metadata),
/// stored as `(py_type_ptr_as_usize, clojure_rt_typeid)`. Walked when
/// interning a new Python class to determine inherited impls.
static REGISTERED_ABCS: LazyLock<RwLock<Vec<(usize, TypeId)>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

unsafe fn primitive_destruct(_: *mut Header) {
    unreachable!("Python class TypeIds are inline (no heap header to destruct)");
}

/// Install the resolver that maps `Value(TYPE_PYOBJECT)` payloads to
/// per-Python-class `TypeId`s. Call once during `clojure_py` init,
/// after `clojure_rt::init()`.
pub fn install_foreign_resolver() {
    foreign::register_foreign_resolver(TYPE_PYOBJECT, resolver_for_pyobject);
}

/// Foreign-type resolver for `TYPE_PYOBJECT`. Reads `Py_TYPE(payload)`
/// and interns it on first encounter; cache-hit fast path otherwise.
unsafe fn resolver_for_pyobject(payload: u64) -> TypeId {
    let obj_ptr = payload as *mut pyffi::PyObject;
    let py_type_ptr = unsafe { (*obj_ptr).ob_type } as *mut pyffi::PyObject;

    if let Some(&tid) = PY_TYPE_TABLE.read().unwrap().get(&(py_type_ptr as usize)) {
        return tid;
    }
    Python::attach(|py| intern_with_inheritance(py, py_type_ptr))
}

/// Register a Python class as a dynamic clojure_rt type and install
/// inherited impls into its per-type table. GIL-required.
fn intern_with_inheritance(py: Python<'_>, py_type: *mut pyffi::PyObject) -> TypeId {
    let key = py_type as usize;

    // Re-check under write lock (race vs. concurrent interning of the
    // same class).
    let tid = {
        let mut table = PY_TYPE_TABLE.write().unwrap();
        if let Some(&tid) = table.get(&key) {
            return tid;
        }
        let name = python_class_name(py, py_type);
        let tid = type_registry::register_dynamic_type(
            name,
            Layout::from_size_align(0, 1).unwrap(),
            primitive_destruct,
        );
        table.insert(key, tid);
        tid
    };

    populate_inherited_impls(py, py_type, tid);
    tid
}

/// Walk the registered-ABC list; for each ABC the class is a subclass
/// of, copy its impl entries into the new class's per-type table.
/// First-match-wins per `method_id` (registered-ABC list order is the
/// tiebreaker).
fn populate_inherited_impls(
    py: Python<'_>,
    py_type: *mut pyffi::PyObject,
    new_tid: TypeId,
) {
    let _ = py; // GIL token — held implicitly by FFI calls below.

    let mut entries: Vec<(u32, *const ())> = Vec::new();
    let abcs = REGISTERED_ABCS.read().unwrap();

    for &(abc_ptr, abc_tid) in abcs.iter() {
        // PyObject_IsSubclass respects __subclasshook__, which is what
        // makes ABCs structural for built-ins like list/dict/str.
        let r = unsafe {
            pyffi::PyObject_IsSubclass(py_type, abc_ptr as *mut pyffi::PyObject)
        };
        if r != 1 {
            // 0 = not subclass; -1 = error (treat as not subclass; in
            // practice means we passed something that isn't a class).
            continue;
        }
        let Some(abc_meta) = type_registry::try_get(abc_tid) else { continue };
        let table = abc_meta.table.load();
        for slot in table.slots.iter() {
            if slot.method_id == 0 {
                continue;
            }
            // First-match wins — only insert if no prior ABC supplied
            // this method.
            if !entries.iter().any(|(mid, _)| *mid == slot.method_id) {
                entries.push((slot.method_id, slot.fn_ptr));
            }
        }
    }

    if entries.is_empty() {
        return;
    }

    let new_meta = type_registry::get(new_tid);
    let new_table = Arc::new(PerTypeTable::build(&entries));
    new_meta.table.store(new_table);
}

/// Register an ABC (or any class acting as inheritance metadata) so
/// that future per-class interning walks inherit from it. Returns the
/// `TypeId` minted (or previously minted) for the ABC. GIL-required.
pub fn register_abc(py: Python<'_>, abc: &Bound<'_, PyAny>) -> TypeId {
    let abc_ptr = abc.as_ptr();
    let key = abc_ptr as usize;

    // Already-interned fast path: just make sure it's also in the ABC
    // list.
    if let Some(&tid) = PY_TYPE_TABLE.read().unwrap().get(&key) {
        let mut abcs = REGISTERED_ABCS.write().unwrap();
        if !abcs.iter().any(|(p, _)| *p == key) {
            abcs.push((key, tid));
        }
        return tid;
    }

    let mut table = PY_TYPE_TABLE.write().unwrap();
    if let Some(&tid) = table.get(&key) {
        let mut abcs = REGISTERED_ABCS.write().unwrap();
        if !abcs.iter().any(|(p, _)| *p == key) {
            abcs.push((key, tid));
        }
        return tid;
    }
    let name = python_class_name(py, abc_ptr);
    let tid = type_registry::register_dynamic_type(
        name,
        Layout::from_size_align(0, 1).unwrap(),
        primitive_destruct,
    );
    table.insert(key, tid);

    let mut abcs = REGISTERED_ABCS.write().unwrap();
    abcs.push((key, tid));

    tid
}

/// Extract a 'static class name for a Python class. Leaks the string
/// — Python classes are essentially immortal in CPython, so the
/// per-process leak is bounded by class-count.
fn python_class_name(py: Python<'_>, py_type: *mut pyffi::PyObject) -> &'static str {
    let bound: Bound<'_, PyAny> = unsafe { Bound::from_borrowed_ptr(py, py_type) };
    let raw = bound
        .cast::<PyType>()
        .ok()
        .and_then(|c| c.name().ok())
        .map(|s| s.to_string());
    let owned = match raw {
        Some(s) => format!("py:{s}"),
        None    => "py:<unnamed>".to_string(),
    };
    Box::leak(owned.into_boxed_str())
}
