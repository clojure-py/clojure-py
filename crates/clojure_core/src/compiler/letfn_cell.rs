//! Mutable forward-reference cell used by `letfn*`.
//!
//! `letfn*` binds a set of mutually-recursive fns. The compiler allocates
//! a fresh `LetfnCell` for each name *before* compiling any of the fn
//! bodies. Each fn's compiled closure captures the cell (PyObjects are
//! reference-counted; capturing a slot containing a cell shares the
//! cell). After a fn is constructed, its closure is stored into its cell
//! via `Op::LetfnCellSet`. Name references inside a letfn-bound body
//! compile to `Load{Local,Capture} + LetfnCellGet`, which reads the
//! current contents of the cell.
//!
//! Cells are uninitialized on construction and contain `None` until
//! their corresponding `LetfnCellSet` runs. Reading an uninitialized
//! cell raises an internal compiler error — the compiler arranges all
//! sets to happen before any body that could observe an unset cell.

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "LetfnCell", frozen)]
pub struct LetfnCell {
    inner: Mutex<Option<PyObject>>,
}

impl LetfnCell {
    pub fn new() -> Self {
        Self { inner: Mutex::new(None) }
    }

    pub fn set(&self, v: PyObject) {
        *self.inner.lock() = Some(v);
    }

    pub fn get(&self, py: Python<'_>) -> PyResult<PyObject> {
        let guard = self.inner.lock();
        match guard.as_ref() {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(crate::eval::errors::err(
                "LetfnCell read before set (internal compiler error)",
            )),
        }
    }
}
