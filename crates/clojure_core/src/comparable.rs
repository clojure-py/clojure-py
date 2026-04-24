//! Comparable — three-way comparison backing `clojure.core/compare`.
//!
//! Returns a negative number, zero, or a positive number when `this` is
//! logically less than, equal to, or greater than `other`. Nil sorts first;
//! numbers and strings use Python's ordering; mismatched types fail the same
//! way Python's `<` does.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyCFunction, PyDict, PyFloat, PyInt, PyString, PyTuple};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Comparable", extend_via_metadata = false, emit_fn_primary = true)]
pub trait Comparable: Sized {
    fn compare_to(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<i64>;
}

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let proto_any = m.getattr("Comparable")?;
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

            let cmp_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>|
                    -> PyResult<Py<PyAny>>
                {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    let other = inner.get_item(1)?;
                    let r = compare_builtin(py, &this, &other)?;
                    Ok(r.into_pyobject(py)?.unbind().into_any())
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("compare_to", &cmp_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty.clone(), impls)?;
            if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "Comparable", "compare_to") {
                let mut fns = crate::protocol_fn::InvokeFns::empty();
                fns.invoke1 = Some(compare_builtin_thunk as crate::protocol_fn::InvokeFn1);
                pfn.bind(py).get().extend_with_native(ty, fns);
            }

            Ok(py.None())
        },
    )?;

    proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}

/// Typed thunk for Comparable/compare_to fallback.
fn compare_builtin_thunk(
    py: Python<'_>,
    target: &Py<PyAny>,
    other: Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    let r = compare_builtin(py, target.bind(py), other.bind(py))?;
    Ok(r.into_pyobject(py)?.unbind().into_any())
}

/// Built-in fallback: nil sorts before anything; bools/ints/floats compare
/// numerically; strings lex; otherwise use Python `<`/`>` and error on
/// incomparable types.
fn compare_builtin(py: Python<'_>, this: &Bound<'_, PyAny>, other: &Bound<'_, PyAny>) -> PyResult<i64> {
    let this_nil = this.is_none();
    let other_nil = other.is_none();
    if this_nil && other_nil { return Ok(0); }
    if this_nil { return Ok(-1); }
    if other_nil { return Ok(1); }

    // Numbers (bool is a subclass of int in Python, so this covers it too).
    let this_num = this.cast::<PyInt>().is_ok() || this.cast::<PyFloat>().is_ok();
    let other_num = other.cast::<PyInt>().is_ok() || other.cast::<PyFloat>().is_ok();
    if this_num && other_num {
        if this.lt(other)? { return Ok(-1); }
        if this.gt(other)? { return Ok(1); }
        return Ok(0);
    }

    // Strings
    if this.cast::<PyString>().is_ok() && other.cast::<PyString>().is_ok() {
        if this.lt(other)? { return Ok(-1); }
        if this.gt(other)? { return Ok(1); }
        return Ok(0);
    }

    // Booleans reach here if mixed with non-numeric; fall through to generic.
    let _ = PyBool::new(py, false);

    // Generic Python ordering — raises TypeError if incomparable.
    match this.lt(other) {
        Ok(true) => Ok(-1),
        Ok(false) => match this.gt(other) {
            Ok(true) => Ok(1),
            Ok(false) => Ok(0),
            Err(e) => Err(crate::exceptions::IllegalArgumentException::new_err(
                format!("compare: incomparable values: {}", e),
            )),
        },
        Err(e) => Err(crate::exceptions::IllegalArgumentException::new_err(
            format!("compare: incomparable values: {}", e),
        )),
    }
}
