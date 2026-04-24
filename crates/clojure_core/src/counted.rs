use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCFunction, PyDict, PyTuple};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Counted", extend_via_metadata = false, emit_fn_primary = true)]
pub trait Counted: Sized {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
}

/// Default `count` via Python's `__len__`. Covers str, list, tuple, dict,
/// set, and any user-defined object that implements `__len__`. Types that
/// don't have `__len__` still fail at dispatch time.
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
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
