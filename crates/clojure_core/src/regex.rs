//! Regex matcher — stateful wrapper around Python's `re.Pattern.finditer` so
//! `(re-find matcher)` advances and `(nth matcher i)` returns the i-th group
//! of the most recent match. Mirrors vanilla `java.util.regex.Matcher`.

use std::sync::Arc;

use arc_swap::ArcSwap;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::indexed::Indexed;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Matcher", frozen)]
pub struct Matcher {
    /// Python iterator from `pattern.finditer(string)`. Advanced by `advance`.
    iter: PyObject,
    /// Most recent re.Match returned by `advance`, or None.
    last: ArcSwap<Option<PyObject>>,
}

#[pymethods]
impl Matcher {
    #[new]
    pub fn new(pattern: PyObject, s: PyObject, py: Python<'_>) -> PyResult<Self> {
        let it = pattern.bind(py).call_method1("finditer", (s,))?.unbind();
        Ok(Matcher {
            iter: it,
            last: ArcSwap::new(Arc::new(None)),
        })
    }

    /// Pull the next match from the iterator and store it as `last`. Returns
    /// the Python re.Match (or None if the iterator is exhausted).
    pub(crate) fn advance(&self, py: Python<'_>) -> PyResult<PyObject> {
        let next_fn = py.import("builtins")?.getattr("next")?;
        let none_sentinel = py.None();
        let v = next_fn.call1((self.iter.bind(py), &none_sentinel))?;
        if v.is_none() {
            self.last.store(Arc::new(None));
            Ok(py.None())
        } else {
            let m = v.unbind();
            self.last.store(Arc::new(Some(m.clone_ref(py))));
            Ok(m)
        }
    }

    /// The most recent match, or None if no match has been pulled.
    #[getter]
    pub fn last(&self, py: Python<'_>) -> PyObject {
        let g = self.last.load();
        match (*g).as_ref() {
            Some(m) => m.clone_ref(py),
            None => py.None(),
        }
    }
}

#[implements(Indexed)]
impl Indexed for Matcher {
    fn nth(this: Py<Self>, py: Python<'_>, i: PyObject) -> PyResult<PyObject> {
        let idx: i64 = i.bind(py).extract()?;
        let last = this.bind(py).get().last(py);
        if last.is_none(py) {
            return Err(crate::exceptions::IllegalStateException::new_err(
                "No match found",
            ));
        }
        let last_b = last.bind(py);
        let groups = last_b.call_method0("groups")?;
        let group_count: i64 = groups.len()? as i64;
        if idx < 0 || idx > group_count {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "Matcher index out of bounds: {}",
                idx
            )));
        }
        Ok(last_b.call_method1("group", (idx,))?.unbind())
    }

    fn nth_or_default(
        this: Py<Self>,
        py: Python<'_>,
        i: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        let idx: i64 = i.bind(py).extract()?;
        let last = this.bind(py).get().last(py);
        if last.is_none(py) {
            return Err(crate::exceptions::IllegalStateException::new_err(
                "No match found",
            ));
        }
        let last_b = last.bind(py);
        let groups = last_b.call_method0("groups")?;
        let group_count: i64 = groups.len()? as i64;
        if idx < 0 || idx > group_count {
            return Ok(default);
        }
        Ok(last_b.call_method1("group", (idx,))?.unbind())
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Matcher>()?;
    Ok(())
}
