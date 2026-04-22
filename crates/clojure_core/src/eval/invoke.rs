//! Function invocation: eval head + args, then rt::invoke_n.

use crate::collections::plist::{EmptyList, PersistentList};
use crate::eval::env::Env;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Evaluate a list as a function invocation. Head is already extracted but not evaluated.
pub fn eval_invocation(py: Python<'_>, list: PyObject, env: &Env) -> PyResult<PyObject> {
    let b = list.bind(py);
    let pl = b.downcast::<PersistentList>().unwrap();  // caller guarantees
    let head_form = pl.get().head.clone_ref(py);
    // Evaluate head to get a callable.
    let f = crate::eval::eval(py, head_form, env)?;
    // Collect arg forms from the rest.
    let mut arg_forms: Vec<PyObject> = Vec::new();
    let mut cur: PyObject = pl.get().tail.clone_ref(py);
    loop {
        let cb = cur.bind(py);
        if cb.downcast::<EmptyList>().is_ok() { break; }
        if let Ok(pl2) = cb.downcast::<PersistentList>() {
            arg_forms.push(pl2.get().head.clone_ref(py));
            cur = pl2.get().tail.clone_ref(py);
            continue;
        }
        break;
    }
    // Evaluate each argument.
    let mut arg_vals: Vec<PyObject> = Vec::with_capacity(arg_forms.len());
    for form in arg_forms {
        arg_vals.push(crate::eval::eval(py, form, env)?);
    }
    // Invoke via rt::invoke_n (routes through IFn dispatch).
    crate::rt::invoke_n(py, f, &arg_vals)
}
