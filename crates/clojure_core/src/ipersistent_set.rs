use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentSet", extend_via_metadata = false, emit_fn_primary = true)]
pub trait IPersistentSet: Sized {
    fn disjoin(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}

/// Equiv test that works across our two set types (`PersistentHashSet` and
/// `PersistentTreeSet`). Mirrors vanilla `APersistentSet.equals`: same size,
/// same hash, then every element of `a` is contained in `b`. The hash check
/// is also a guard against the `(sorted-set 1)` containing `:a` lookup
/// blowing up the comparator on incomparable elements.
pub fn cross_set_equiv(
    py: Python<'_>,
    a: PyObject,
    b: PyObject,
) -> PyResult<bool> {
    let a_b = a.bind(py);
    let b_b = b.bind(py);

    let a_phs = a_b.cast::<crate::collections::phashset::PersistentHashSet>().ok();
    let a_pts = a_b.cast::<crate::collections::ptreeset::PersistentTreeSet>().ok();
    let b_phs = b_b.cast::<crate::collections::phashset::PersistentHashSet>().ok();
    let b_pts = b_b.cast::<crate::collections::ptreeset::PersistentTreeSet>().ok();

    if (a_phs.is_none() && a_pts.is_none()) || (b_phs.is_none() && b_pts.is_none()) {
        return Ok(false);
    }

    let count_a = crate::rt::count(py, a.clone_ref(py))?;
    let count_b = crate::rt::count(py, b.clone_ref(py))?;
    if count_a != count_b { return Ok(false); }

    let ha = crate::rt::hash_eq(py, a.clone_ref(py))?;
    let hb = crate::rt::hash_eq(py, b.clone_ref(py))?;
    if ha != hb { return Ok(false); }

    for item in a_b.try_iter()? {
        let x = item?.unbind();
        let contained = if let Some(s) = &b_phs {
            s.get().contains_internal(py, x)?
        } else if let Some(s) = &b_pts {
            s.get().contains_internal(py, x)?
        } else { false };
        if !contained { return Ok(false); }
    }
    Ok(true)
}
