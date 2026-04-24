use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IHashEq", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IHashEq: Sized {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64>;
}

use pyo3::types::{PyCFunction, PyDict, PyTuple};

/// Typed thunk for the __hash__ fallback. Reachable as a fn pointer so
/// ProtocolFn's typed cache can call it directly.
fn py_hash_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let h: isize = target.bind(py).hash()?;
    Ok((h as i64).into_pyobject(py)?.unbind().into_any())
}

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ihasheq_any = m.getattr("IHashEq")?;
    let ihasheq_proto: &Bound<'_, crate::Protocol> = ihasheq_any.cast()?;

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
            proto.get().extend_type(py, ty.clone(), impls)?;
            // Also populate the typed ProtocolFn cache so subsequent calls
            // take the fast path directly.
            if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "IHashEq", "hash_eq") {
                let mut fns = crate::protocol_fn::InvokeFns::empty();
                fns.invoke0 = Some(py_hash_thunk as crate::protocol_fn::InvokeFn0);
                pfn.bind(py).get().extend_with_native(ty, fns);
            }

            Ok(py.None())
        },
    )?;

    ihasheq_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
