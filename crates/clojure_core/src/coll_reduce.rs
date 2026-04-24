//! CollReduce — per-collection reducer protocol backing `clojure.core/reduce`.
//!
//! Two arities: `coll_reduce1(coll, f)` (no init; uses first element) and
//! `coll_reduce2(coll, f, init)`. Collections that want a fast path implement
//! this directly; everything else falls back to seq-walking, with a
//! chunked-seq optimization that dispatches through `IChunk::chunk_reduce`.
//!
//! Short-circuit via `Reduced` is not yet implemented — callers like the
//! internal `reduce1` used by variadic arithmetic never short-circuit, so
//! folds always run to completion. Adding `Reduced` is a single-chunk
//! follow-up.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCFunction, PyDict, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/CollReduce", extend_via_metadata = false, emit_fn_primary = true)]
pub trait CollReduce: Sized {
    /// Reduce without an initial value. Equivalent to `(reduce f coll)`.
    fn coll_reduce1(this: Py<Self>, py: Python<'_>, f: PyObject) -> PyResult<PyObject>;
    /// Reduce with an initial value. Equivalent to `(reduce f init coll)`.
    fn coll_reduce2(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject>;
}

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let proto_any = m.getattr("CollReduce")?;
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

            // Install two impls — one per arity — for the target's Python type.
            let reduce1_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let coll = inner.get_item(0)?.unbind();
                    let f = inner.get_item(1)?.unbind();
                    fallback_reduce1(py, coll, f)
                },
            )?;
            let reduce2_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let coll = inner.get_item(0)?.unbind();
                    let f = inner.get_item(1)?.unbind();
                    let init = inner.get_item(2)?.unbind();
                    fallback_reduce2(py, coll, f, init)
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("coll_reduce1", &reduce1_wrapper)?;
            impls.set_item("coll_reduce2", &reduce2_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}

/// Seq-walking reducer with chunked-seq fast-path. Used both by the fallback
/// and by direct impls that don't have a better internal walk.
pub fn fallback_reduce1(py: Python<'_>, coll: PyObject, f: PyObject) -> PyResult<PyObject> {
    let s = crate::rt::seq(py, coll)?;
    if s.is_none(py) {
        // (f) — empty + no init
        return crate::rt::invoke_n(py, f, &[]);
    }
    let init = crate::rt::first(py, s.clone_ref(py))?;
    let rest = crate::rt::next_(py, s)?;
    if rest.is_none(py) {
        return Ok(init);
    }
    fallback_reduce2(py, rest, f, init)
}

pub fn fallback_reduce2(py: Python<'_>, coll: PyObject, f: PyObject, init: PyObject) -> PyResult<PyObject> {
    let mut acc = init;
    let mut cur = crate::rt::seq(py, coll)?;
    loop {
        if cur.is_none(py) { return Ok(acc); }
        if is_chunked_seq(py, &cur)? {
            let chunk = chunked_first(py, cur.clone_ref(py))?;
            acc = invoke_chunk_reduce(py, chunk, f.clone_ref(py), acc)?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
            cur = chunked_next(py, cur)?;
        } else {
            let head = crate::rt::first(py, cur.clone_ref(py))?;
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, head])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
            cur = crate::rt::next_(py, cur)?;
        }
    }
}

fn is_chunked_seq(py: Python<'_>, coll: &PyObject) -> PyResult<bool> {
    let proto_any = py.import("clojure._core")?.getattr("IChunkedSeq")?;
    let proto: &Bound<'_, crate::Protocol> = proto_any.cast()?;
    let ty = coll.bind(py).get_type();
    let exact_key = crate::protocol::CacheKey::for_py_type(&ty);
    if proto.get().cache.lookup(exact_key).is_some() {
        return Ok(true);
    }
    let mro = ty.getattr("__mro__")?;
    let mro_tuple: Bound<'_, PyTuple> = mro.cast_into()?;
    for parent in mro_tuple.iter().skip(1) {
        let parent_ty: Bound<'_, pyo3::types::PyType> = parent.cast_into()?;
        let pk = crate::protocol::CacheKey::for_py_type(&parent_ty);
        if proto.get().cache.lookup(pk).is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn chunked_first(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let proto_any = py.import("clojure._core")?.getattr("IChunkedSeq")?;
    let proto: Py<crate::Protocol> = proto_any.cast::<crate::Protocol>()?.clone().unbind();
    let key: Arc<str> = Arc::from("chunked_first");
    let empty = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, &proto, &key, coll, empty)
}

fn chunked_next(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let proto_any = py.import("clojure._core")?.getattr("IChunkedSeq")?;
    let proto: Py<crate::Protocol> = proto_any.cast::<crate::Protocol>()?.clone().unbind();
    let key: Arc<str> = Arc::from("chunked_next");
    let empty = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, &proto, &key, coll, empty)
}

fn invoke_chunk_reduce(py: Python<'_>, chunk: PyObject, f: PyObject, init: PyObject) -> PyResult<PyObject> {
    let proto_any = py.import("clojure._core")?.getattr("IChunk")?;
    let proto: Py<crate::Protocol> = proto_any.cast::<crate::Protocol>()?.clone().unbind();
    let key: Arc<str> = Arc::from("chunk_reduce");
    let args = PyTuple::new(py, &[f, init])?;
    crate::dispatch::dispatch(py, &proto, &key, chunk, args)
}
