//! IFn — the protocol implemented by anything callable from Clojure.
//!
//! Declared with 22 arities (`invoke0`..`invoke20`, `invoke_variadic`) matching
//! JVM Clojure's IFn. Every method has a default body that raises an
//! ArityException with the arity and the Rust type name; implementers override
//! only the arities they actually handle. The `#[implements(IFn)]` macro only
//! registers the explicitly-overridden methods with the dispatch cache, so
//! callers of an un-overridden arity receive a clean "No implementation of
//! method: invokeN of protocol: …" error from the dispatch layer.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

/// Short name for the implementing type, used in arity-error messages. We
/// grab the last `::` segment of `std::any::type_name::<T>()` so
/// `clojure_core::keyword::Keyword` reports as just `Keyword`.
fn type_short_name<T>() -> &'static str {
    let full = std::any::type_name::<T>();
    full.rsplit("::").next().unwrap_or(full)
}

fn arity_err<T>(n: usize) -> PyResult<PyObject> {
    Err(crate::exceptions::ArityException::new_err(format!(
        "Wrong number of args ({}) passed to: {}",
        n,
        type_short_name::<T>()
    )))
}

#[protocol(name = "clojure.core/IFn", extend_via_metadata = false)]
pub trait IFn: Sized {
    fn invoke0(_this: Py<Self>, _py: Python<'_>) -> PyResult<PyObject> { arity_err::<Self>(0) }
    fn invoke1(_this: Py<Self>, _py: Python<'_>, _a0: PyObject) -> PyResult<PyObject> { arity_err::<Self>(1) }
    fn invoke2(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject) -> PyResult<PyObject> { arity_err::<Self>(2) }
    fn invoke3(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject) -> PyResult<PyObject> { arity_err::<Self>(3) }
    fn invoke4(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject) -> PyResult<PyObject> { arity_err::<Self>(4) }
    fn invoke5(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject) -> PyResult<PyObject> { arity_err::<Self>(5) }
    fn invoke6(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject) -> PyResult<PyObject> { arity_err::<Self>(6) }
    fn invoke7(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject) -> PyResult<PyObject> { arity_err::<Self>(7) }
    fn invoke8(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject) -> PyResult<PyObject> { arity_err::<Self>(8) }
    fn invoke9(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject) -> PyResult<PyObject> { arity_err::<Self>(9) }
    fn invoke10(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject) -> PyResult<PyObject> { arity_err::<Self>(10) }
    fn invoke11(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject) -> PyResult<PyObject> { arity_err::<Self>(11) }
    fn invoke12(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject) -> PyResult<PyObject> { arity_err::<Self>(12) }
    fn invoke13(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject) -> PyResult<PyObject> { arity_err::<Self>(13) }
    fn invoke14(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject) -> PyResult<PyObject> { arity_err::<Self>(14) }
    fn invoke15(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject) -> PyResult<PyObject> { arity_err::<Self>(15) }
    fn invoke16(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject) -> PyResult<PyObject> { arity_err::<Self>(16) }
    fn invoke17(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject) -> PyResult<PyObject> { arity_err::<Self>(17) }
    fn invoke18(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject) -> PyResult<PyObject> { arity_err::<Self>(18) }
    fn invoke19(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject) -> PyResult<PyObject> { arity_err::<Self>(19) }
    fn invoke20(_this: Py<Self>, _py: Python<'_>, _a0: PyObject, _a1: PyObject, _a2: PyObject, _a3: PyObject, _a4: PyObject, _a5: PyObject, _a6: PyObject, _a7: PyObject, _a8: PyObject, _a9: PyObject, _a10: PyObject, _a11: PyObject, _a12: PyObject, _a13: PyObject, _a14: PyObject, _a15: PyObject, _a16: PyObject, _a17: PyObject, _a18: PyObject, _a19: PyObject) -> PyResult<PyObject> { arity_err::<Self>(20) }
    fn invoke_variadic(_this: Py<Self>, _py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err(format!(
            "Wrong number of args ({}) passed to: {}",
            args.len(),
            type_short_name::<Self>()
        )))
    }
}

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pyo3::types::{PyCFunction, PyDict};

    let ifn_any = m.getattr("IFn")?;
    let ifn_proto: &Bound<'_, crate::Protocol> = ifn_any.cast()?;

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

            if !target.is_callable() {
                return Ok(py.None());
            }

            // Build the generic invoke_variadic impl: it receives (self, *a) and calls self(*a).
            let inv = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner_args: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner_args.py();
                    let self_obj = inner_args.get_item(0)?;
                    let n = inner_args.len();
                    let mut rest: Vec<Py<PyAny>> = Vec::with_capacity(n.saturating_sub(1));
                    for i in 1..n {
                        rest.push(inner_args.get_item(i)?.unbind());
                    }
                    let rest_tup = PyTuple::new(py, &rest)?;
                    Ok(self_obj.call1(rest_tup)?.unbind())
                },
            )?;

            // Build the new impls dict, MERGING with any existing extension:
            // preserve whatever invoke* methods the type already has (e.g. from
            // a `#[implements(IFn)]` on a PyO3 class like `Keyword`), and only
            // install the Python-call fallback for arities that are missing.
            // Without this merge, the first time a bad-arity call hits a
            // partially-implemented IFn type (e.g. `(:kw)` on Keyword with no
            // `invoke0`), the fallback would clobber the type's real invoke1 /
            // invoke2 impls with generic Python-call wrappers — breaking later
            // correct calls like `(:key record)` which rely on those native
            // impls.
            let ty = target.get_type();
            let impls = PyDict::new(py);
            let proto_ref = proto.get();
            let existing = proto_ref
                .cache
                .lookup(crate::protocol::CacheKey::for_py_type(&ty));
            if let Some(table) = existing {
                for (k, v) in table.impls.iter() {
                    impls.set_item(k.as_ref(), v.clone_ref(py))?;
                }
            }
            for key in [
                "invoke0", "invoke1", "invoke2", "invoke3", "invoke4", "invoke5",
                "invoke6", "invoke7", "invoke8", "invoke9", "invoke10", "invoke11",
                "invoke12", "invoke13", "invoke14", "invoke15", "invoke16", "invoke17",
                "invoke18", "invoke19", "invoke20", "invoke_variadic",
            ] {
                if !impls.contains(key)? {
                    impls.set_item(key, &inv)?;
                }
            }

            proto_ref.extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    ifn_proto.call_method1("set_fallback", (fallback,))?;
    Ok(())
}
