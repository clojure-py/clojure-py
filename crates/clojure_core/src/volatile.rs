//! Volatile — an intentionally non-synchronized mutable cell.
//!
//! Clojure semantics: Volatile is for single-threaded use (e.g. transducer
//! state). `vswap!` is NOT atomic; it just reads, applies f, writes. On the
//! JVM it uses a `volatile` field for publication guarantees. We model it
//! with `ArcSwap` too — cheap enough, and it keeps publication semantics
//! consistent — but nothing here does CAS retries.

use crate::ideref::IDeref;
use arc_swap::ArcSwap;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Volatile", frozen)]
pub struct Volatile {
    pub value: ArcSwap<PyObject>,
}

impl Volatile {
    pub fn new(initial: PyObject) -> Self {
        Self { value: ArcSwap::new(Arc::new(initial)) }
    }

    pub fn current(&self, py: Python<'_>) -> PyObject {
        let g = self.value.load();
        let v: &PyObject = &g;
        v.clone_ref(py)
    }

    pub fn reset(&self, py: Python<'_>, new: PyObject) -> PyObject {
        let out = new.clone_ref(py);
        self.value.store(Arc::new(new));
        out
    }

    pub fn vswap(
        &self,
        py: Python<'_>,
        f: PyObject,
        args: &[PyObject],
    ) -> PyResult<PyObject> {
        let current = self.current(py);
        let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() + 1);
        call_args.push(current);
        for a in args.iter() {
            call_args.push(a.clone_ref(py));
        }
        let new = crate::rt::invoke_n(py, f, &call_args)?;
        self.value.store(Arc::new(new.clone_ref(py)));
        Ok(new)
    }
}

#[pymethods]
impl Volatile {
    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let g = self.value.load();
        let v: &PyObject = &g;
        let s = v.bind(py).repr()?.extract::<String>()?;
        Ok(format!("#<Volatile {}>", s))
    }
}

#[implements(IDeref)]
impl IDeref for Volatile {
    fn deref(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(this.bind(py).get().current(py))
    }
}

#[pyfunction]
#[pyo3(name = "volatile")]
pub fn py_volatile(initial: PyObject) -> Volatile {
    Volatile::new(initial)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Volatile>()?;
    m.add_function(wrap_pyfunction!(py_volatile, m)?)?;
    Ok(())
}
