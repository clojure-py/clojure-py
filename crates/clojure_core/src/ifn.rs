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

#[protocol(name = "clojure.core/IFn", extend_via_metadata = false, emit_fn_primary = true)]
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

// Typed thunks for the generic "call target as a Python callable" fallback.
// One per arity (invoke0..invoke20) + variadic. All do the same thing: call
// the target with the supplied args as a PyTuple. Macro generates them to
// avoid repetition.
macro_rules! define_call_thunk {
    ($name:ident, $($arg:ident),*) => {
        fn $name(
            py: Python<'_>,
            target: &Py<PyAny>,
            $($arg: Py<PyAny>,)*
        ) -> PyResult<Py<PyAny>> {
            let items: [Py<PyAny>; 0 $(+ { let _ = stringify!($arg); 1 })*] = [$($arg,)*];
            let rest = pyo3::types::PyTuple::new(py, &items)?;
            Ok(target.bind(py).call1(rest)?.unbind())
        }
    };
}

fn call_thunk_0(
    py: Python<'_>,
    target: &Py<PyAny>,
) -> PyResult<Py<PyAny>> {
    Ok(target.bind(py).call0()?.unbind())
}
define_call_thunk!(call_thunk_1,  a);
define_call_thunk!(call_thunk_2,  a, b);
define_call_thunk!(call_thunk_3,  a, b, c);
define_call_thunk!(call_thunk_4,  a, b, c, d);
define_call_thunk!(call_thunk_5,  a, b, c, d, e);
define_call_thunk!(call_thunk_6,  a, b, c, d, e, f);
define_call_thunk!(call_thunk_7,  a, b, c, d, e, f, g);
define_call_thunk!(call_thunk_8,  a, b, c, d, e, f, g, h);
define_call_thunk!(call_thunk_9,  a, b, c, d, e, f, g, h, i);
define_call_thunk!(call_thunk_10, a, b, c, d, e, f, g, h, i, j);
define_call_thunk!(call_thunk_11, a, b, c, d, e, f, g, h, i, j, k);
define_call_thunk!(call_thunk_12, a, b, c, d, e, f, g, h, i, j, k, l);
define_call_thunk!(call_thunk_13, a, b, c, d, e, f, g, h, i, j, k, l, mm);
define_call_thunk!(call_thunk_14, a, b, c, d, e, f, g, h, i, j, k, l, mm, n);
define_call_thunk!(call_thunk_15, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o);
define_call_thunk!(call_thunk_16, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p);
define_call_thunk!(call_thunk_17, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q);
define_call_thunk!(call_thunk_18, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r);
define_call_thunk!(call_thunk_19, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r, s);
define_call_thunk!(call_thunk_20, a, b, c, d, e, f, g, h, i, j, k, l, mm, n, o, p, q, r, s, t);

/// Variadic: take a Vec of args, wrap in tuple, call.
fn call_thunk_variadic(
    py: Python<'_>,
    target: &Py<PyAny>,
    rest: Vec<Py<PyAny>>,
) -> PyResult<Py<PyAny>> {
    let tup = pyo3::types::PyTuple::new(py, &rest)?;
    Ok(target.bind(py).call1(tup)?.unbind())
}

/// Install the generic "call target as a Python callable" fallback into
/// every IFn invokeN ProtocolFn plus the variadic one. For types without
/// a direct #[implements(IFn)] impl (e.g. plain Python callables), this
/// fills in the dispatch table on first hit with typed thunks that just
/// forward to CPython's `call1`.
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
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.cast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            if !target.is_callable() {
                return Ok(py.None());
            }

            let ty = target.get_type();
            let proto_ref = proto.get();

            // Old path: merge with existing (preserves real impls), add
            // generic Python-call wrapper for missing arities.
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
            let impls = PyDict::new(py);
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
            proto_ref.extend_type(py, ty.clone(), impls)?;

            // New path: install typed thunks in each arity ProtocolFn's
            // cache so subsequent calls take the fast path. Only populate
            // arities that aren't already covered by a direct
            // #[implements(IFn)] impl on this type.
            //
            // Macro helper: lookup ProtocolFn by method name, build InvokeFns
            // with the matching field set, call extend_with_native.
            macro_rules! install_arity {
                ($method:literal, $field:ident, $thunk:ident, $ty_alias:ident) => {{
                    if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, "IFn", $method) {
                        let pfn_ref = pfn.bind(py).get();
                        // Only install if this type isn't already in the cache
                        // with a direct impl for this arity.
                        let exact_key = crate::protocol::CacheKey::for_py_type(&ty);
                        let already_has = pfn_ref.cache.get(&exact_key)
                            .map(|e| e.value().$field.is_some())
                            .unwrap_or(false);
                        if !already_has {
                            let mut fns = crate::protocol_fn::InvokeFns::empty();
                            fns.$field = Some($thunk as crate::protocol_fn::$ty_alias);
                            pfn_ref.extend_with_native(ty.clone(), fns);
                        }
                    }
                }};
            }
            install_arity!("invoke0",  invoke0,  call_thunk_0,  InvokeFn0);
            install_arity!("invoke1",  invoke1,  call_thunk_1,  InvokeFn1);
            install_arity!("invoke2",  invoke2,  call_thunk_2,  InvokeFn2);
            install_arity!("invoke3",  invoke3,  call_thunk_3,  InvokeFn3);
            install_arity!("invoke4",  invoke4,  call_thunk_4,  InvokeFn4);
            install_arity!("invoke5",  invoke5,  call_thunk_5,  InvokeFn5);
            install_arity!("invoke6",  invoke6,  call_thunk_6,  InvokeFn6);
            install_arity!("invoke7",  invoke7,  call_thunk_7,  InvokeFn7);
            install_arity!("invoke8",  invoke8,  call_thunk_8,  InvokeFn8);
            install_arity!("invoke9",  invoke9,  call_thunk_9,  InvokeFn9);
            install_arity!("invoke10", invoke10, call_thunk_10, InvokeFn10);
            install_arity!("invoke11", invoke11, call_thunk_11, InvokeFn11);
            install_arity!("invoke12", invoke12, call_thunk_12, InvokeFn12);
            install_arity!("invoke13", invoke13, call_thunk_13, InvokeFn13);
            install_arity!("invoke14", invoke14, call_thunk_14, InvokeFn14);
            install_arity!("invoke15", invoke15, call_thunk_15, InvokeFn15);
            install_arity!("invoke16", invoke16, call_thunk_16, InvokeFn16);
            install_arity!("invoke17", invoke17, call_thunk_17, InvokeFn17);
            install_arity!("invoke18", invoke18, call_thunk_18, InvokeFn18);
            install_arity!("invoke19", invoke19, call_thunk_19, InvokeFn19);
            install_arity!("invoke20", invoke20, call_thunk_20, InvokeFn20);
            install_arity!("invoke_variadic", invoke_variadic, call_thunk_variadic, InvokeFnVariadic);

            Ok(py.None())
        },
    )?;

    ifn_proto.call_method1("set_fallback", (fallback,))?;
    Ok(())
}
