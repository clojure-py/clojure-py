use clojure_core_macros::protocol;
use once_cell::sync::OnceCell;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyCFunction, PyDict, PyFloat, PyInt, PyTuple, PyType};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IEquiv", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IEquiv: Sized {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}

/// Categorize a value for vanilla-Clojure-equivalent equiv. Mirrors the JVM
/// `Util.equiv` + `Numbers.equal`/`category` rules: `bool` is its own thing
/// (Boolean is not a Number on the JVM); Long, Double, Ratio, BigDecimal are
/// distinct numeric categories that never compare equal under `=`.
#[derive(Copy, Clone, PartialEq)]
enum Cat { Bool, Int, Float, Ratio, Decimal }

/// `fractions.Fraction` and `decimal.Decimal` class refs. Populated once at
/// `install_builtin_fallback` time so `classify` doesn't re-import per call.
static FRACTION_CLS: OnceCell<Py<PyType>> = OnceCell::new();
static DECIMAL_CLS:  OnceCell<Py<PyType>> = OnceCell::new();

#[inline]
fn classify(v: &Bound<'_, PyAny>) -> Option<Cat> {
    // Order matters: bool subclasses int in Python, so check it first.
    if v.is_instance_of::<PyBool>()  { return Some(Cat::Bool); }
    if v.is_instance_of::<PyInt>()   { return Some(Cat::Int); }
    if v.is_instance_of::<PyFloat>() { return Some(Cat::Float); }
    if let Some(fr) = FRACTION_CLS.get() {
        if v.is_instance(fr.bind(v.py())).unwrap_or(false) {
            return Some(Cat::Ratio);
        }
    }
    if let Some(dc) = DECIMAL_CLS.get() {
        if v.is_instance(dc.bind(v.py())).unwrap_or(false) {
            return Some(Cat::Decimal);
        }
    }
    None
}

/// Default equiv: Python `==`. Used for opaque user types where Python's
/// equality is the only thing we know about them.
fn py_eq_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let eq_result = target.bind(py).eq(other.bind(py))?;
    Ok(PyBool::new(py, eq_result).to_owned().unbind().into_any())
}

/// `bool` equiv: only equal to other booleans. Without this, `(= 1 true)`
/// returns true because Python's `True == 1`.
fn py_eq_bool_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let other_b = other.bind(py);
    let result = if other_b.is_instance_of::<PyBool>() {
        target.bind(py).eq(other_b)?
    } else { false };
    Ok(PyBool::new(py, result).to_owned().unbind().into_any())
}

/// `int` (non-bool) equiv: matches other ints (Python `==`) but never
/// matches `bool` or `float` — vanilla treats Long and Double as distinct
/// numeric categories, so `(= 1 1.0) → false`.
fn py_eq_int_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let other_b = other.bind(py);
    let result = match classify(other_b) {
        Some(Cat::Int) => target.bind(py).eq(other_b)?,
        Some(Cat::Bool) | Some(Cat::Float) | Some(Cat::Ratio) | Some(Cat::Decimal) => false,
        None => target.bind(py).eq(other_b)?,
    };
    Ok(PyBool::new(py, result).to_owned().unbind().into_any())
}

/// `float` equiv: never matches `bool` or `int`; matches other floats.
fn py_eq_float_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let other_b = other.bind(py);
    let result = match classify(other_b) {
        Some(Cat::Float) => target.bind(py).eq(other_b)?,
        Some(Cat::Bool) | Some(Cat::Int) | Some(Cat::Ratio) | Some(Cat::Decimal) => false,
        None => target.bind(py).eq(other_b)?,
    };
    Ok(PyBool::new(py, result).to_owned().unbind().into_any())
}

/// `Char` equiv: matches only other `Char` instances by value. Vanilla
/// Character.equals compares Character to Character only; here we ensure
/// `(= \a "a")` and `(= \a 97)` both return false.
fn py_eq_char_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let result = match other.bind(py).cast::<crate::char::Char>() {
        Ok(o) => target.bind(py).cast::<crate::char::Char>()
            .map(|t| t.get().value == o.get().value)
            .unwrap_or(false),
        Err(_) => false,
    };
    Ok(PyBool::new(py, result).to_owned().unbind().into_any())
}

/// `Ratio` (fractions.Fraction) equiv: matches other Ratios via Python `==`;
/// false against Bool/Int/Float/Decimal.
fn py_eq_ratio_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let other_b = other.bind(py);
    let result = match classify(other_b) {
        Some(Cat::Ratio) => target.bind(py).eq(other_b)?,
        Some(_)          => false,
        None             => target.bind(py).eq(other_b)?,
    };
    Ok(PyBool::new(py, result).to_owned().unbind().into_any())
}

/// `Decimal` equiv: matches other Decimals via Python `==`; false against
/// Bool/Int/Float/Ratio.
fn py_eq_decimal_thunk(py: Python<'_>, target: &Py<PyAny>, other: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let other_b = other.bind(py);
    let result = match classify(other_b) {
        Some(Cat::Decimal) => target.bind(py).eq(other_b)?,
        Some(_)            => false,
        None               => target.bind(py).eq(other_b)?,
    };
    Ok(PyBool::new(py, result).to_owned().unbind().into_any())
}

/// Build a `PyCFunction` closure that calls `thunk` for IEquiv/equiv.
fn wrapper_for(
    py: Python<'_>,
    thunk: fn(Python<'_>, &Py<PyAny>, Py<PyAny>) -> PyResult<Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    let f = PyCFunction::new_closure(
        py,
        None,
        None,
        move |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = inner.py();
            let this = inner.get_item(0)?.unbind();
            let other = inner.get_item(1)?.unbind();
            thunk(py, &this, other)
        },
    )?;
    Ok(f.unbind().into_any())
}

/// Eagerly install equiv impl for a specific Python type. Required for
/// `bool`/`int`/`float` because Python's `bool ⊂ int` MRO causes the
/// legacy-mirror lookup to find int's impl when given a bool — pre-registering
/// each type with its exact-type key short-circuits that.
fn install_for_type(
    py: Python<'_>,
    proto: &Bound<'_, crate::Protocol>,
    ty: Bound<'_, pyo3::types::PyType>,
    thunk: fn(Python<'_>, &Py<PyAny>, Py<PyAny>) -> PyResult<Py<PyAny>>,
) -> PyResult<()> {
    let wrapper = wrapper_for(py, thunk)?;
    let impls = PyDict::new(py);
    impls.set_item("equiv", &wrapper)?;
    proto.get().extend_type(py, ty.clone(), impls)?;
    if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "IEquiv", "equiv") {
        let mut fns = crate::protocol_fn::InvokeFns::empty();
        fns.invoke1 = Some(thunk as crate::protocol_fn::InvokeFn1);
        pfn.bind(py).get().extend_with_native(ty, fns);
    }
    Ok(())
}

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let iequiv_any = m.getattr("IEquiv")?;
    let iequiv_proto: &Bound<'_, crate::Protocol> = iequiv_any.cast()?;

    // Pre-register Python primitive types with vanilla-Clojure semantics.
    install_for_type(py, iequiv_proto, py.get_type::<PyBool>(), py_eq_bool_thunk)?;
    install_for_type(py, iequiv_proto, py.get_type::<PyInt>(), py_eq_int_thunk)?;
    install_for_type(py, iequiv_proto, py.get_type::<PyFloat>(), py_eq_float_thunk)?;
    install_for_type(py, iequiv_proto, py.get_type::<crate::char::Char>(), py_eq_char_thunk)?;

    // Cache class refs and register Ratio + Decimal as their own equiv categories.
    let fractions = py.import("fractions")?;
    let frac_cls: Bound<'_, PyType> = fractions.getattr("Fraction")?.downcast_into::<PyType>()?;
    let _ = FRACTION_CLS.set(frac_cls.clone().unbind());

    let decimal = py.import("decimal")?;
    let dec_cls: Bound<'_, PyType> = decimal.getattr("Decimal")?.downcast_into::<PyType>()?;
    let _ = DECIMAL_CLS.set(dec_cls.clone().unbind());

    install_for_type(py, iequiv_proto, frac_cls, py_eq_ratio_thunk)?;
    install_for_type(py, iequiv_proto, dec_cls,  py_eq_decimal_thunk)?;

    // For everything else (user types, etc.), fall back to Python `==`.
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

            let wrapper = wrapper_for(py, py_eq_thunk)?;
            let impls = PyDict::new(py);
            impls.set_item("equiv", &wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty.clone(), impls)?;
            if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "IEquiv", "equiv") {
                let mut fns = crate::protocol_fn::InvokeFns::empty();
                fns.invoke1 = Some(py_eq_thunk as crate::protocol_fn::InvokeFn1);
                pfn.bind(py).get().extend_with_native(ty, fns);
            }

            Ok(py.None())
        },
    )?;

    iequiv_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
