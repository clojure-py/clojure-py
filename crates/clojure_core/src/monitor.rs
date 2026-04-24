//! Hybrid monitor runtime. Vanilla Clojure's `monitor-enter` / `monitor-exit`
//! rely on the JVM giving every object an intrinsic monitor. Python has no
//! such thing. Two paths:
//!
//! 1. **ContextManager fast path** — if the value implements the
//!    `__enter__` / `__exit__` protocol (e.g. `threading.Lock`,
//!    `threading.RLock`, or one of our own pyclasses we opt in later),
//!    `monitor_enter` calls `__enter__()` and `monitor_exit` calls
//!    `__exit__(None, None, None)`. This is the natural Python mapping.
//!
//! 2. **Registry fallback** — for arbitrary values with no CM protocol
//!    (vectors, atoms, user objects), we maintain a process-global
//!    `WeakKeyDictionary[object, threading.RLock]` so each target value
//!    owns a dedicated reentrant lock for its lifetime. Non-weakrefable
//!    values (ints, tuples, strings, frozensets) fall through to an
//!    `id`-keyed plain dict; entries there leak until process exit, which
//!    is acceptable given those values are typically interned.
//!
//! Reentrancy is required for `locking` semantics: the same thread must
//! be allowed to re-enter the monitor it already holds. `threading.RLock`
//! provides this.

use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PyTuple};

type PyObject = Py<PyAny>;

struct Registry {
    /// Python-level `weakref.WeakKeyDictionary()` — keyed by the object,
    /// value is the `threading.RLock`.
    weak_dict: PyObject,
    /// Python-level `dict()` — keyed by `id(obj)` (as an int), for values
    /// that can't take a weak reference.
    id_dict: PyObject,
    /// Cached `threading.RLock` class for fresh-lock creation.
    rlock_cls: PyObject,
}

static REGISTRY: OnceCell<Mutex<Registry>> = OnceCell::new();

fn ensure_registry(py: Python<'_>) -> PyResult<&'static Mutex<Registry>> {
    if let Some(r) = REGISTRY.get() {
        return Ok(r);
    }
    let weakref = py.import("weakref")?;
    let wkd = weakref.getattr("WeakKeyDictionary")?.call0()?.unbind();
    let id_dict = PyDict::new(py).unbind();
    let threading = py.import("threading")?;
    let rlock_cls = threading.getattr("RLock")?.unbind();
    let reg = Registry {
        weak_dict: wkd,
        id_dict: id_dict.into_any(),
        rlock_cls,
    };
    let _ = REGISTRY.set(Mutex::new(reg));
    Ok(REGISTRY.get().unwrap())
}

fn has_context_manager(x: &Bound<'_, PyAny>) -> bool {
    x.hasattr("__enter__").unwrap_or(false) && x.hasattr("__exit__").unwrap_or(false)
}

fn is_weakrefable(py: Python<'_>, x: &Bound<'_, PyAny>) -> bool {
    // weakref.proxy raises TypeError on non-weakrefable objects; use a cheap
    // probe via `weakref.ref` in a try/except.
    let Ok(weakref) = py.import("weakref") else {
        return false;
    };
    let Ok(ref_cls) = weakref.getattr("ref") else {
        return false;
    };
    ref_cls.call1((x.clone(),)).is_ok()
}

/// Get (or lazily create) the reentrant lock associated with `x`.
fn get_or_create_lock(py: Python<'_>, x: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    let reg_cell = ensure_registry(py)?;
    let reg = reg_cell.lock();
    let weak_dict_b = reg.weak_dict.bind(py);
    let id_dict_b = reg.id_dict.bind(py);

    if is_weakrefable(py, x) {
        // Try lookup first.
        if let Ok(existing) = weak_dict_b.call_method1("get", (x.clone(),)) {
            if !existing.is_none() {
                return Ok(existing.unbind());
            }
        }
        let lock = reg.rlock_cls.bind(py).call0()?.unbind();
        weak_dict_b.set_item(x.clone(), &lock)?;
        return Ok(lock);
    }
    // Fall back to id-keyed dict.
    let id_key = x.as_ptr() as usize;
    let id_key_py = id_key.into_pyobject(py)?;
    if let Ok(existing) = id_dict_b.get_item(&id_key_py) {
        return Ok(existing.unbind());
    }
    let lock = reg.rlock_cls.bind(py).call0()?.unbind();
    id_dict_b.set_item(id_key_py, &lock)?;
    Ok(lock)
}

/// `monitor-enter` — acquire a monitor on `x`.
pub fn monitor_enter(py: Python<'_>, x: PyObject) -> PyResult<()> {
    let b = x.bind(py);
    if has_context_manager(&b) {
        b.call_method0("__enter__")?;
        return Ok(());
    }
    let lock = get_or_create_lock(py, &b)?;
    lock.bind(py).call_method0("acquire")?;
    Ok(())
}

/// `monitor-exit` — release a monitor on `x`.
pub fn monitor_exit(py: Python<'_>, x: PyObject) -> PyResult<()> {
    let b = x.bind(py);
    if has_context_manager(&b) {
        let none = py.None();
        let args = PyTuple::new(py, &[none.clone_ref(py), none.clone_ref(py), none])?;
        b.call_method1("__exit__", args)?;
        return Ok(());
    }
    let lock = get_or_create_lock(py, &b)?;
    lock.bind(py).call_method0("release")?;
    Ok(())
}

pub(crate) fn register(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
