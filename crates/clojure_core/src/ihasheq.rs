use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IHashEq", extend_via_metadata = false)]
pub trait IHashEq: Sized {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64>;
}

use pyo3::types::{PyCFunction, PyDict, PyTuple};

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ihasheq_any = m.getattr("IHashEq")?;
    let ihasheq_proto: &Bound<'_, crate::Protocol> = ihasheq_any.downcast()?;

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

            let hash_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    let h: isize = this.hash()?;
                    Ok((h as i64).into_pyobject(py)?.unbind().into_any())
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("hash_eq", &hash_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    ihasheq_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
