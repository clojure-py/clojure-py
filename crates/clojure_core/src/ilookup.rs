//! ILookup — the protocol behind `(get m k)` / `(get m k default)`.
//!
//! Our own collections implement it directly via `#[implements(ILookup)]`.
//! For arbitrary Python types that support `__getitem__`, a built-in fallback
//! installed at module init registers a generic wrapper — so `(get dict :k)`
//! works uniformly with `(get our-pmap :k)` through a single dispatch path.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCFunction, PyDict, PyTuple};

type PyObject = Py<PyAny>;

/// Clojure's canonical lookup protocol.
///
/// `val_at(coll, key, not_found)` returns the value at `key`, or `not_found`
/// if absent. The single-method (with not_found) shape matches Clojure-JVM's
/// `IPersistentMap.valAt(key, notFound)`; the two-arg Clojure form `(get m k)`
/// is spelled `val_at(coll, k, nil)` at the Rust level.
#[protocol(name = "clojure.core/ILookup", extend_via_metadata = false)]
pub trait ILookup: Sized {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject>;
}

/// Install a fallback that, for any target with `__getitem__`, registers
/// a generic wrapper: val_at(target, k, not_found) = target[k] on hit,
/// not_found on KeyError/IndexError.
pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ilookup_any = m.getattr("ILookup")?;
    let ilookup_proto: &Bound<'_, crate::Protocol> = ilookup_any.cast()?;

    let fallback = PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = args.py();
            // args = (protocol, method_key, target)
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.cast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            // Build the val_at wrapper: takes (self, k, not_found), tries
            // self[k] when the target has __getitem__; otherwise returns
            // not_found. Matches vanilla's `(get non-lookupable k)` → nil.
            let val_at_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner_args: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let _py = inner_args.py();
                    let self_obj = inner_args.get_item(0)?;
                    let k = inner_args.get_item(1)?;
                    let not_found = inner_args.get_item(2)?;
                    if self_obj.getattr("__getitem__").is_err() {
                        return Ok(not_found.unbind());
                    }
                    match self_obj.get_item(&k) {
                        Ok(v) => Ok(v.unbind()),
                        Err(_) => Ok(not_found.unbind()),
                    }
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("val_at", &val_at_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    // Protocol::set_fallback is not pub-to-Rust — dispatch through Python.
    ilookup_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
