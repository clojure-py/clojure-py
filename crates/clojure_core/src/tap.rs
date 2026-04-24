//! Tap system for `(add-tap …)` / `(remove-tap …)` / `(tap> …)`.
//!
//! Vanilla maintains a global set of tap-fns and a bounded async queue
//! that decouples producers from slow consumers. We keep a global list of
//! tap-fns and fire each synchronously on the calling thread when `tap>`
//! is invoked. Sufficient for REPL inspection — async semantics can be
//! layered in later if a real consumer needs the queue.

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

static TAPS: Mutex<Vec<PyObject>> = Mutex::new(Vec::new());

pub fn add(_py: Python<'_>, f: PyObject) {
    TAPS.lock().push(f);
}

/// Remove first occurrence of `f` (by Python identity).
pub fn remove(py: Python<'_>, f: PyObject) {
    let mut taps = TAPS.lock();
    if let Some(ix) = taps.iter().position(|t| crate::rt::identical(py, t.clone_ref(py), f.clone_ref(py))) {
        taps.remove(ix);
    }
}

/// Fire every tap-fn with the value. Returns true if any fired (i.e. there
/// was at least one tap registered). Errors from individual taps are
/// swallowed — vanilla's queue would similarly drop them on the floor.
pub fn fire(py: Python<'_>, v: PyObject) -> PyResult<bool> {
    let snapshot: Vec<PyObject> = {
        let taps = TAPS.lock();
        taps.iter().map(|t| t.clone_ref(py)).collect()
    };
    if snapshot.is_empty() {
        return Ok(false);
    }
    for t in snapshot {
        let _ = crate::rt::invoke_n(py, t, &[v.clone_ref(py)]);
    }
    Ok(true)
}
