use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentMap", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IPersistentMap: Sized {
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
    fn without(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}

/// Equiv test that works across our three map types
/// (`PersistentHashMap`, `PersistentArrayMap`, `PersistentTreeMap`).
/// Mirrors vanilla `APersistentMap.equiv`: same count, same hash, then
/// every key/value pair in `a` matches what `b` returns for that key.
pub fn cross_map_equiv(
    py: Python<'_>,
    a: PyObject,
    b: PyObject,
) -> PyResult<bool> {
    use crate::collections::parraymap::PersistentArrayMap;
    use crate::collections::phashmap::PersistentHashMap;
    use crate::collections::ptreemap::PersistentTreeMap;
    let a_b = a.bind(py);
    let b_b = b.bind(py);

    let a_is_map = a_b.cast::<PersistentHashMap>().is_ok()
        || a_b.cast::<PersistentArrayMap>().is_ok()
        || a_b.cast::<PersistentTreeMap>().is_ok();
    let b_is_map = b_b.cast::<PersistentHashMap>().is_ok()
        || b_b.cast::<PersistentArrayMap>().is_ok()
        || b_b.cast::<PersistentTreeMap>().is_ok();
    if !a_is_map || !b_is_map { return Ok(false); }

    let count_a = crate::rt::count(py, a.clone_ref(py))?;
    let count_b = crate::rt::count(py, b.clone_ref(py))?;
    if count_a != count_b { return Ok(false); }

    let ha = crate::rt::hash_eq(py, a.clone_ref(py))?;
    let hb = crate::rt::hash_eq(py, b.clone_ref(py))?;
    if ha != hb { return Ok(false); }

    // Sentinel so a missing key in `b` can't accidentally equiv a present
    // `nil` value in `a` (vanilla uses a bespoke missing-marker).
    let sentinel: PyObject = pyo3::types::PyTuple::empty(py).unbind().into_any();

    // Walk `a` as MapEntries via `seq` — vanilla map iteration yields entries.
    let mut cur = crate::rt::seq(py, a.clone_ref(py))?;
    while !cur.is_none(py) {
        let entry = crate::rt::first(py, cur.clone_ref(py))?;
        let me = entry.bind(py).cast::<crate::collections::map_entry::MapEntry>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err(
                "cross_map_equiv: expected MapEntry from map iteration"))?;
        let k = me.get().key.clone_ref(py);
        let v = me.get().val.clone_ref(py);
        let bv = lookup_in_map(py, &b, k, sentinel.clone_ref(py))?;
        if bv.bind(py).is(sentinel.bind(py)) { return Ok(false); }
        if !crate::rt::equiv(py, v, bv)? { return Ok(false); }
        cur = crate::rt::next_(py, cur)?;
    }
    Ok(true)
}

fn lookup_in_map(
    py: Python<'_>,
    map: &PyObject,
    k: PyObject,
    not_found: PyObject,
) -> PyResult<PyObject> {
    use crate::collections::parraymap::PersistentArrayMap;
    use crate::collections::phashmap::PersistentHashMap;
    use crate::collections::ptreemap::PersistentTreeMap;
    let b = map.bind(py);
    if let Ok(m) = b.cast::<PersistentHashMap>() {
        return m.get().val_at_default_internal(py, k, not_found);
    }
    if let Ok(m) = b.cast::<PersistentArrayMap>() {
        return m.get().val_at_default_internal(py, k, not_found);
    }
    if let Ok(m) = b.cast::<PersistentTreeMap>() {
        return m.get().val_at_default_internal(py, k, not_found);
    }
    Ok(not_found)
}
