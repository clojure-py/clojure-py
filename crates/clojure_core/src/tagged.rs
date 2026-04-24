//! `TaggedLiteral` and `ReaderConditional` data representations.
//!
//! Vanilla exposes these as `clojure.lang.TaggedLiteral` and
//! `clojure.lang.ReaderConditional`. They're plain immutable value
//! types: a tag-symbol + form (TaggedLiteral) or a form + splicing
//! flag (ReaderConditional). The `tagged-literal` / `reader-conditional`
//! functions and `…?` predicates in core.clj construct and check them.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "TaggedLiteral", frozen)]
pub struct TaggedLiteral {
    #[pyo3(get)]
    pub tag: PyObject,
    #[pyo3(get)]
    pub form: PyObject,
}

#[pymethods]
impl TaggedLiteral {
    #[new]
    fn new(tag: PyObject, form: PyObject) -> Self {
        Self { tag, form }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let t = self.tag.bind(py).repr()?.to_string();
        let f = self.form.bind(py).repr()?.to_string();
        Ok(format!("#{} {}", t, f))
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        let Ok(o) = other.downcast::<TaggedLiteral>() else {
            return Ok(false);
        };
        let o = o.get();
        let t_eq = crate::rt::equiv(py, self.tag.clone_ref(py), o.tag.clone_ref(py))?;
        if !t_eq { return Ok(false); }
        crate::rt::equiv(py, self.form.clone_ref(py), o.form.clone_ref(py))
    }
}

#[pyclass(module = "clojure._core", name = "ReaderConditional", frozen)]
pub struct ReaderConditional {
    #[pyo3(get)]
    pub form: PyObject,
    #[pyo3(get)]
    pub splicing: bool,
}

#[pymethods]
impl ReaderConditional {
    #[new]
    fn new(form: PyObject, splicing: bool) -> Self {
        Self { form, splicing }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let f = self.form.bind(py).repr()?.to_string();
        Ok(if self.splicing {
            format!("#?@ {}", f)
        } else {
            format!("#? {}", f)
        })
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> PyResult<bool> {
        let Ok(o) = other.downcast::<ReaderConditional>() else {
            return Ok(false);
        };
        let o = o.get();
        if self.splicing != o.splicing { return Ok(false); }
        crate::rt::equiv(py, self.form.clone_ref(py), o.form.clone_ref(py))
    }
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TaggedLiteral>()?;
    m.add_class::<ReaderConditional>()?;
    let _ = py;
    Ok(())
}
