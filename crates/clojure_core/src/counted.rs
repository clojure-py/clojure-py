use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCFunction, PyDict, PyTuple};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Counted", extend_via_metadata = false, emit_fn_primary = true)]
pub trait Counted: Sized {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
}

/// Default `count` via Python's `__len__`. Typed fn-pointer thunk — gets
/// installed into both the old Protocol's cache and the new ProtocolFn's
/// typed table on first hit for a given type.
fn len_based_count_thunk(
    py: Python<'_>,
    target: &Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    let n: usize = target.bind(py).len()?;
    Ok((n as i64).into_pyobject(py)?.unbind().into_any())
}

/// Default `count` via Python's `__len__`. Covers str, list, tuple, dict,
/// set, and any user-defined object that implements `__len__`. Types that
/// don't have `__len__` still fail at dispatch time.
///
/// On first dispatch hit for a given type, populates BOTH the old Protocol
/// cache (for legacy dispatch paths) AND the new ProtocolFn's typed cache
/// (so subsequent calls take the typed fast path).
pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let proto_any = m.getattr("Counted")?;
    let proto: &Bound<'_, crate::Protocol> = proto_any.cast()?;

    let fallback = PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = args.py();
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.cast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            let count_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    let n: usize = this.len()?;
                    Ok((n as i64).into_pyobject(py)?.unbind().into_any())
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("count", &count_wrapper)?;
            let ty = target.get_type();
            // Install in the old Protocol (still needed for a bit longer).
            proto.get().extend_type(py, ty.clone(), impls)?;
            // Install in the new ProtocolFn — lets subsequent calls take
            // the typed fast path instead of the fall-through.
            if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "Counted", "count") {
                let mut fns = crate::protocol_fn::InvokeFns::empty();
                fns.invoke0 = Some(len_based_count_thunk as crate::protocol_fn::InvokeFn0);
                pfn.bind(py).get().extend_with_native(ty, fns);
            }

            Ok(py.None())
        },
    )?;

    proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
