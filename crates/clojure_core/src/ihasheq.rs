use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyCFunction, PyDict, PyFloat, PyInt, PyTuple};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IHashEq", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IHashEq: Sized {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64>;
}

fn make_i64(py: Python<'_>, v: i64) -> PyResult<Py<PyAny>> {
    Ok(v.into_pyobject(py)?.unbind().into_any())
}

/// Default hasheq: Python `__hash__`. Used for opaque user types.
fn py_hash_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let h: isize = target.bind(py).hash()?;
    make_i64(py, h as i64)
}

/// `bool` hash: matches `Boolean.hashCode` on the JVM (1231 / 1237).
/// Distinct from any int/float hash so `1` and `true` don't collide.
fn py_hash_bool_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let b: bool = target.bind(py).extract()?;
    make_i64(py, if b { 1231 } else { 1237 })
}

/// `int` (non-bool) hash: `Murmur3.hashLong(value)` for values in i64 range.
/// For arbitrary-precision ints outside i64 range we fall back to Python's
/// hash; vanilla's `hasheqFrom` does the analogous thing for BigInteger
/// values outside Long range (`x.hashCode()`).
fn py_hash_int_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let bound = target.bind(py);
    let h: i32 = match bound.extract::<i64>() {
        Ok(v) => crate::murmur3::hash_long(v),
        Err(_) => bound.hash()? as i32,
    };
    make_i64(py, h as i64)
}

/// `float` hash: matches `Double.hashCode` on the JVM, with `-0.0` normalized
/// to `0.0` per `Numbers.hasheq`. Distinct from int hash for the same value
/// (e.g. `1` vs `1.0`).
fn py_hash_float_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let bound = target.bind(py);
    let v: f64 = bound.extract()?;
    let v = if v == 0.0 { 0.0 } else { v }; // collapse -0.0 → 0.0
    let bits = v.to_bits();
    let h = ((bits ^ (bits >> 32)) as u32) as i32;
    make_i64(py, h as i64)
}

/// `Char` hash: matches JVM `Character.hashCode()` — returns the codepoint
/// directly, not Murmur3'd. This intentionally diverges from int hash so
/// `(hash \a)` ≠ `(hash 97)` to match vanilla.
fn py_hash_char_thunk(py: Python<'_>, target: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let bound = target.bind(py);
    let c = bound.cast::<crate::char::Char>()?;
    make_i64(py, c.get().value as i64)
}

fn wrapper_for(
    py: Python<'_>,
    thunk: fn(Python<'_>, &Py<PyAny>) -> PyResult<Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    let f = PyCFunction::new_closure(
        py,
        None,
        None,
        move |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = inner.py();
            let this = inner.get_item(0)?.unbind();
            thunk(py, &this)
        },
    )?;
    Ok(f.unbind().into_any())
}

/// Eagerly install hash impl for a specific Python type. Same MRO concern
/// as IEquiv: bool ⊂ int means bool needs its own exact-type entry, else
/// the legacy mirror falls through to int's impl.
fn install_for_type(
    py: Python<'_>,
    proto: &Bound<'_, crate::Protocol>,
    ty: Bound<'_, pyo3::types::PyType>,
    thunk: fn(Python<'_>, &Py<PyAny>) -> PyResult<Py<PyAny>>,
) -> PyResult<()> {
    let wrapper = wrapper_for(py, thunk)?;
    let impls = PyDict::new(py);
    impls.set_item("hash_eq", &wrapper)?;
    proto.get().extend_type(py, ty.clone(), impls)?;
    if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "IHashEq", "hash_eq") {
        let mut fns = crate::protocol_fn::InvokeFns::empty();
        fns.invoke0 = Some(thunk as crate::protocol_fn::InvokeFn0);
        pfn.bind(py).get().extend_with_native(ty, fns);
    }
    Ok(())
}

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ihasheq_any = m.getattr("IHashEq")?;
    let ihasheq_proto: &Bound<'_, crate::Protocol> = ihasheq_any.cast()?;

    install_for_type(py, ihasheq_proto, py.get_type::<PyBool>(), py_hash_bool_thunk)?;
    install_for_type(py, ihasheq_proto, py.get_type::<PyInt>(), py_hash_int_thunk)?;
    install_for_type(py, ihasheq_proto, py.get_type::<PyFloat>(), py_hash_float_thunk)?;
    install_for_type(py, ihasheq_proto, py.get_type::<crate::char::Char>(), py_hash_char_thunk)?;

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

            let wrapper = wrapper_for(py, py_hash_thunk)?;
            let impls = PyDict::new(py);
            impls.set_item("hash_eq", &wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty.clone(), impls)?;
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
