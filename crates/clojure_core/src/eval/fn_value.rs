//! Fn — a closure value. Captures env at creation; applies on invocation.

use crate::eval::env::Env;
use crate::eval::errors;
use crate::ifn::IFn;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Fn", frozen)]
pub struct Fn {
    pub captured_locals: parking_lot::RwLock<std::collections::HashMap<String, PyObject>>,
    pub current_ns: PyObject,
    pub param_names: Vec<String>,
    pub body: Vec<PyObject>,   // each form in the body; eval'd in sequence like (do ...)
    pub name: Option<String>,
}

impl Fn {
    /// Build the env for invocation: captured locals + param bindings.
    fn make_call_env(&self, py: Python<'_>, args: &[PyObject]) -> PyResult<Env> {
        if args.len() != self.param_names.len() {
            let fname = self.name.as_deref().unwrap_or("<anonymous>");
            return Err(errors::err(format!(
                "Wrong number of args ({}) passed to fn {} (expected {})",
                args.len(),
                fname,
                self.param_names.len()
            )));
        }
        let mut locals: std::collections::HashMap<String, PyObject> = self
            .captured_locals
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_ref(py)))
            .collect();
        for (name, val) in self.param_names.iter().zip(args.iter()) {
            locals.insert(name.clone(), val.clone_ref(py));
        }
        Ok(Env {
            locals,
            current_ns: self.current_ns.clone_ref(py),
        })
    }

    fn apply(&self, py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
        let env = self.make_call_env(py, args)?;
        let mut result: PyObject = py.None();
        for form in &self.body {
            result = crate::eval::eval(py, form.clone_ref(py), &env)?;
        }
        Ok(result)
    }
}

#[pymethods]
impl Fn {
    #[pyo3(signature = (*args))]
    fn __call__(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        let mut vals: Vec<PyObject> = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            vals.push(args.get_item(i)?.unbind());
        }
        self.apply(py, &vals)
    }

    fn __repr__(&self) -> String {
        format!("#<Fn {}>", self.name.as_deref().unwrap_or("anonymous"))
    }
}

#[implements(IFn)]
impl IFn for Fn {
    fn invoke0(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[])
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0])
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1])
    }
    fn invoke3(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2])
    }
    fn invoke4(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3])
    }
    fn invoke5(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4])
    }
    fn invoke6(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5])
    }
    fn invoke7(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6])
    }
    fn invoke8(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7])
    }
    fn invoke9(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8])
    }
    fn invoke10(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9])
    }
    fn invoke11(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10])
    }
    fn invoke12(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11])
    }
    fn invoke13(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12])
    }
    fn invoke14(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13])
    }
    fn invoke15(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14])
    }
    fn invoke16(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15])
    }
    fn invoke17(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16])
    }
    fn invoke18(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17])
    }
    fn invoke19(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18])
    }
    fn invoke20(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject, a19: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().apply(py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18, a19])
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        let mut vals: Vec<PyObject> = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            vals.push(args.get_item(i)?.unbind());
        }
        this.bind(py).get().apply(py, &vals)
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Fn>()?;
    Ok(())
}
