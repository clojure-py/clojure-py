use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCFunction, PyDict, PyModule, PyString, PyTuple};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ISeqable", extend_via_metadata = false, emit_fn_primary = true)]
pub trait ISeqable: Sized {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}

/// Pre-registered seq impl for `str`: yields a seq of `Char` values, matching
/// vanilla's `(seq "abc")` → `(\a \b \c)`.
fn py_seq_string_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let s = target.bind(py).cast::<PyString>()?.to_str()?;
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return Ok(py.None());
    }
    let mut acc: Py<PyAny> = crate::collections::plist::empty_list(py).into_any();
    for c in chars.into_iter().rev() {
        let item: Py<PyAny> = Py::new(py, crate::char::Char::new(c))?.into_any();
        acc = crate::rt::conj(py, acc, item)?;
    }
    Ok(acc)
}

/// Fallback for builtin Python iterables that aren't otherwise ISeqable.
/// The fallback installs an extend_type entry on first dispatch, so repeat
/// calls hit the cache.
pub(crate) fn install_builtin_fallback(
    py: Python<'_>,
    m: &Bound<'_, PyModule>,
) -> PyResult<()> {
    let proto_any = m.getattr("ISeqable")?;
    let proto: &Bound<'_, crate::Protocol> = proto_any.cast()?;

    // Pre-register `str` so seq-on-string yields Chars (matches vanilla
    // `(seq "abc")` → `(\a \b \c)`).
    let str_wrapper = PyCFunction::new_closure(
        py,
        None,
        None,
        |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = inner.py();
            let this = inner.get_item(0)?.unbind();
            py_seq_string_thunk(py, &this)
        },
    )?;
    let str_impls = PyDict::new(py);
    str_impls.set_item("seq", &str_wrapper)?;
    proto.get().extend_type(py, py.get_type::<PyString>(), str_impls)?;
    if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "ISeqable", "seq") {
        let mut fns = crate::protocol_fn::InvokeFns::empty();
        fns.invoke0 = Some(py_seq_string_thunk as crate::protocol_fn::InvokeFn0);
        pfn.bind(py).get().extend_with_native(py.get_type::<PyString>(), fns);
    }

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

            // Build a seq via Python iteration — then convert to a PList.
            let seq_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    // Collect items into a Vec, then build a plist from them.
                    let mut items: Vec<Py<PyAny>> = Vec::new();
                    let iter = this.try_iter()?;
                    for v in iter {
                        items.push(v?.unbind());
                    }
                    if items.is_empty() {
                        return Ok(py.None());
                    }
                    // Build a PList by consing in reverse.
                    let mut acc: Py<PyAny> =
                        crate::collections::plist::empty_list(py).into_any();
                    for v in items.into_iter().rev() {
                        acc = crate::rt::conj(py, acc, v)?;
                    }
                    Ok(acc)
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("seq", &seq_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty.clone(), impls)?;
            if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "ISeqable", "seq") {
                let mut fns = crate::protocol_fn::InvokeFns::empty();
                fns.invoke0 = Some(iter_seq_thunk as crate::protocol_fn::InvokeFn0);
                pfn.bind(py).get().extend_with_native(ty, fns);
            }

            Ok(py.None())
        },
    )?;

    proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}

/// Typed thunk for the iterator-based seq fallback.
fn iter_seq_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let this = target.bind(py);
    let mut items: Vec<Py<PyAny>> = Vec::new();
    let iter = this.try_iter()?;
    for v in iter {
        items.push(v?.unbind());
    }
    if items.is_empty() {
        return Ok(py.None());
    }
    let mut acc: Py<PyAny> = crate::collections::plist::empty_list(py).into_any();
    for v in items.into_iter().rev() {
        acc = crate::rt::conj(py, acc, v)?;
    }
    Ok(acc)
}
