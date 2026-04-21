use crate::exceptions::IllegalArgumentException;
use crate::protocol::{CacheKey, Protocol};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple, PyType};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Dispatch a protocol method.
///
/// Algorithm (spec §4.2):
/// 1. Exact PyType lookup in the cache.
/// 2. MRO walk (excluding exact type); on hit, promote the entry for the exact type.
/// 3. If `via_metadata`, consult `__clj_meta__` on the target.
/// 4. If a fallback is registered, call it once, then re-run steps 1-3
///    with a "fallback already consulted" flag.
/// 5. Otherwise raise `IllegalArgumentException`.
pub fn dispatch(
    py: Python<'_>,
    protocol_py: &Py<Protocol>,
    method_key: &Arc<str>,
    target: PyObject,
    args: Bound<'_, PyTuple>,
) -> PyResult<PyObject> {
    let protocol = protocol_py.bind(py).get();
    let target_bound = target.bind(py);
    let ty = target_bound.get_type();
    let exact_key = CacheKey::for_py_type(&ty);

    // First pass — no fallback consulted yet.
    if let Some(result) = try_resolve(py, protocol, method_key, &target, &args, &ty, exact_key)? {
        return Ok(result);
    }

    // Fallback consultation.
    let fb_opt = protocol.fallback.read().as_ref().map(|o| o.clone_ref(py));
    if let Some(fb) = fb_opt {
        let fb_args = (
            protocol_py.clone_ref(py),
            method_key.as_ref().to_string(),
            target.clone_ref(py),
        );
        let _: Bound<'_, PyAny> = fb.bind(py).call1(fb_args)?;
        // Retry without re-consulting the fallback.
        if let Some(result) = try_resolve(py, protocol, method_key, &target, &args, &ty, exact_key)? {
            return Ok(result);
        }
    }

    Err(IllegalArgumentException::new_err(format!(
        "No implementation of method: {} of protocol: {} found for class: {}",
        method_key,
        protocol.name.bind(py).get().__repr__(),
        ty.qualname()?.to_string()
    )))
}

/// Runs steps 1-3 of dispatch. Returns `Some(result)` on hit, `None` on miss.
fn try_resolve(
    py: Python<'_>,
    protocol: &Protocol,
    method_key: &Arc<str>,
    target: &PyObject,
    args: &Bound<'_, PyTuple>,
    ty: &Bound<'_, PyType>,
    exact_key: CacheKey,
) -> PyResult<Option<PyObject>> {
    // Step 1: Exact type.
    if let Some(table) = protocol.cache.lookup(exact_key) {
        if let Some(impl_fn) = table.impls.get(method_key) {
            return Ok(Some(call_impl(py, impl_fn, target, args)?));
        }
    }

    // Step 2: MRO walk (skip index 0 = exact type).
    let mro = ty.getattr("__mro__")?;
    let mro_tuple: Bound<'_, PyTuple> = mro.downcast_into()?;
    for parent in mro_tuple.iter().skip(1) {
        let parent_ty: Bound<'_, PyType> = parent.downcast_into()?;
        let pk = CacheKey::for_py_type(&parent_ty);
        if let Some(table) = protocol.cache.lookup(pk) {
            if let Some(impl_fn) = table.impls.get(method_key) {
                // Promote to exact-type cache.
                protocol.cache.entries.insert(exact_key, Arc::clone(&table));
                return Ok(Some(call_impl(py, impl_fn, target, args)?));
            }
        }
    }

    // Step 3: extend-via-metadata (opt-in per protocol).
    if protocol.via_metadata {
        if let Ok(meta) = target.bind(py).getattr("__clj_meta__") {
            if let Ok(meta_dict) = meta.downcast::<PyDict>() {
                if let Some(impl_fn) = meta_dict.get_item(method_key.as_ref())? {
                    return Ok(Some(call_impl_any(py, &impl_fn, target, args)?));
                }
            }
        }
    }

    Ok(None)
}

fn call_impl(
    py: Python<'_>,
    impl_fn: &PyObject,
    target: &PyObject,
    args: &Bound<'_, PyTuple>,
) -> PyResult<PyObject> {
    let mut call_args: Vec<Py<PyAny>> = Vec::with_capacity(args.len() + 1);
    call_args.push(target.clone_ref(py));
    for a in args.iter() {
        call_args.push(a.unbind());
    }
    let tup = PyTuple::new(py, &call_args)?;
    Ok(impl_fn.bind(py).call1(tup)?.unbind())
}

fn call_impl_any(
    py: Python<'_>,
    impl_fn: &Bound<'_, PyAny>,
    target: &PyObject,
    args: &Bound<'_, PyTuple>,
) -> PyResult<PyObject> {
    let mut call_args: Vec<Py<PyAny>> = Vec::with_capacity(args.len() + 1);
    call_args.push(target.clone_ref(py));
    for a in args.iter() {
        call_args.push(a.unbind());
    }
    let tup = PyTuple::new(py, &call_args)?;
    Ok(impl_fn.call1(tup)?.unbind())
}
