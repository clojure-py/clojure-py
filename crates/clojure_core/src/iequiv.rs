use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IEquiv", extend_via_metadata = false)]
pub trait IEquiv: Sized {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}

use pyo3::types::{PyCFunction, PyDict, PyTuple};

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let iequiv_any = m.getattr("IEquiv")?;
    let iequiv_proto: &Bound<'_, crate::Protocol> = iequiv_any.downcast()?;

    let fallback = PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = args.py();
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.downcast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            // Register a generic equiv impl for target's PyType that does Python ==.
            let eq_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    let other = inner.get_item(1)?;
                    let eq_result = this.eq(other)?;
                    Ok(pyo3::types::PyBool::new(py, eq_result).to_owned().unbind().into_any())
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("equiv", &eq_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    iequiv_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
