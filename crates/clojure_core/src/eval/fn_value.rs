//! `Fn` — a compiled Clojure function value.
//!
//! Holds a shared `FnPool` + one `CompiledMethod` per arity + captured
//! closure values. Dispatch on arity picks the right method; overflow goes
//! to `variadic` (packed into a seq for the rest-arg slot).
//!
//! Python sees the same `clojure._core.Fn` pyclass as before — `IFn::invokeN`
//! and `__call__` preserve the existing surface. The body is executed via
//! `vm::run` instead of the old tree walker.

use crate::compiler::method::CompiledMethod;
use crate::compiler::pool::FnPool;
use crate::eval::errors;
use crate::ifn::IFn;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "Fn", frozen)]
pub struct Fn {
    pub name: Option<String>,
    pub current_ns: PyObject,
    pub captures: Vec<PyObject>,
    pub methods: Vec<CompiledMethod>,
    pub variadic: Option<CompiledMethod>,
    pub pool: Arc<FnPool>,
}

impl Fn {
    /// Find the method matching `n_args`; fall through to `variadic` if
    /// no fixed arity matches and the variadic's required count is ≤ n_args.
    fn dispatch_method(&self, n_args: usize) -> PyResult<(&CompiledMethod, bool)> {
        for m in &self.methods {
            if m.arity as usize == n_args { return Ok((m, false)); }
        }
        if let Some(v) = &self.variadic {
            if n_args >= v.arity as usize {
                return Ok((v, true));
            }
        }
        // Arity mismatch should surface as the vanilla-style
        // `ArityException`, not an `EvalError` — tests like
        // `(thrown-with-msg? clojure._core/ArityException #"..." ...)`
        // rely on the specific class.
        Err(crate::exceptions::ArityException::new_err(format!(
            "Wrong number of args ({}) passed to {}",
            n_args,
            self.name.as_deref().unwrap_or("<anonymous>")
        )))
    }

    pub fn apply_with_self(
        slf: Py<Self>,
        py: Python<'_>,
        args: &[PyObject],
    ) -> PyResult<PyObject> {
        let this = slf.bind(py).get();
        let (method, is_variadic_call) = this.dispatch_method(args.len())?;
        let self_any: PyObject = slf.clone_ref(py).into_any();
        if is_variadic_call {
            let required = method.arity as usize;
            let mut frame_args: Vec<PyObject> = args[..required]
                .iter()
                .map(|a| a.clone_ref(py))
                .collect();
            let rest: Vec<PyObject> = args[required..]
                .iter()
                .map(|a| a.clone_ref(py))
                .collect();
            let rest_seq = build_rest_seq(py, rest)?;
            frame_args.push(rest_seq);
            crate::vm::run(py, method, &this.pool, &this.captures, &frame_args, Some(&self_any))
        } else {
            crate::vm::run(py, method, &this.pool, &this.captures, args, Some(&self_any))
        }
    }
}

/// Package variadic overflow args as a seq — uses PersistentList since
/// that's our canonical seq type for reader-produced lists.
fn build_rest_seq(py: Python<'_>, items: Vec<PyObject>) -> PyResult<PyObject> {
    if items.is_empty() {
        return Ok(py.None());
    }
    let tup = PyTuple::new(py, &items)?;
    crate::collections::plist::list_(py, tup)
}

#[pymethods]
impl Fn {
    #[pyo3(signature = (*args))]
    fn __call__(slf: Py<Self>, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        let mut vals: Vec<PyObject> = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            vals.push(args.get_item(i)?.unbind());
        }
        Fn::apply_with_self(slf, py, &vals)
    }

    fn __repr__(&self) -> String {
        format!("#<Fn {}>", self.name.as_deref().unwrap_or("anonymous"))
    }

    /// Return a list of "<pc>: <op-dbg>" strings for each method body.
    /// Layout: {arity -> ["0: Op1", ...]}. Variadic methods are keyed on
    /// `"variadic-<arity>"`. Useful for bench / perf analysis.
    fn dump_code(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = pyo3::types::PyDict::new(py);
        for m in &self.methods {
            let lines: Vec<String> = m.code.iter().enumerate()
                .map(|(pc, op)| format!("{}: {:?}", pc, op))
                .collect();
            d.set_item(format!("{}", m.arity), lines)?;
        }
        if let Some(v) = &self.variadic {
            let lines: Vec<String> = v.code.iter().enumerate()
                .map(|(pc, op)| format!("{}: {:?}", pc, op))
                .collect();
            d.set_item(format!("variadic-{}", v.arity), lines)?;
        }
        Ok(d.unbind().into_any())
    }
}

#[implements(IFn)]
impl IFn for Fn {
    fn invoke0(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[])
    }
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0])
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1])
    }
    fn invoke3(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2])
    }
    fn invoke4(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3])
    }
    fn invoke5(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4])
    }
    fn invoke6(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5])
    }
    fn invoke7(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6])
    }
    fn invoke8(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7])
    }
    fn invoke9(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8])
    }
    fn invoke10(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9])
    }
    fn invoke11(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10])
    }
    fn invoke12(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11])
    }
    fn invoke13(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12])
    }
    fn invoke14(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13])
    }
    fn invoke15(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14])
    }
    fn invoke16(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15])
    }
    fn invoke17(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16])
    }
    fn invoke18(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17])
    }
    fn invoke19(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18])
    }
    fn invoke20(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject, a19: PyObject) -> PyResult<PyObject> {
        Fn::apply_with_self(this.clone_ref(py), py, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17, a18, a19])
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        let mut vals: Vec<PyObject> = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            vals.push(args.get_item(i)?.unbind());
        }
        Fn::apply_with_self(this.clone_ref(py), py, &vals)
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Fn>()?;
    Ok(())
}
