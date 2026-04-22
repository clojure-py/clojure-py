//! Special-form dispatch.

use crate::eval::env::Env;
use crate::eval::errors;
use crate::eval::eval;
use crate::symbol::Symbol;
use crate::collections::plist::{EmptyList, PersistentList};
use crate::collections::pvector::PersistentVector;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Return Some(name) if the list is a special form and `head` is a Symbol matching a known name.
pub fn lookup(head: &PyObject, py: Python<'_>) -> Option<&'static str> {
    let b = head.bind(py);
    let Ok(sym) = b.downcast::<Symbol>() else { return None; };
    let s = sym.get();
    if s.ns.is_some() { return None; }
    match s.name.as_ref() {
        "quote" => Some("quote"),
        "if" => Some("if"),
        "do" => Some("do"),
        "let" | "let*" => Some("let"),
        _ => None,
    }
}

/// Dispatch. `list` is the whole form e.g. (if a b c).
pub fn dispatch(
    py: Python<'_>,
    name: &str,
    list: PyObject,
    env: &Env,
) -> PyResult<PyObject> {
    let args = list_rest(py, list.clone_ref(py))?;
    match name {
        "quote" => quote_form(py, &args),
        "if" => if_form(py, &args, env),
        "do" => do_form(py, &args, env),
        "let" => let_form(py, &args, env),
        _ => Err(errors::err(format!("Unknown special form: {}", name))),
    }
}

/// Utility: collect the args of a PersistentList into Vec<PyObject>, skipping the head.
fn list_rest(py: Python<'_>, list: PyObject) -> PyResult<Vec<PyObject>> {
    let mut out: Vec<PyObject> = Vec::new();
    let mut cur: PyObject = {
        let b = list.bind(py);
        if let Ok(pl) = b.downcast::<PersistentList>() {
            pl.get().tail.clone_ref(py)
        } else {
            return Err(errors::err("list_rest: not a PersistentList"));
        }
    };
    loop {
        let b = cur.bind(py);
        if b.downcast::<EmptyList>().is_ok() { break; }
        if let Ok(pl) = b.downcast::<PersistentList>() {
            out.push(pl.get().head.clone_ref(py));
            cur = pl.get().tail.clone_ref(py);
            continue;
        }
        break;
    }
    Ok(out)
}

fn quote_form(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.len() != 1 {
        return Err(errors::err(format!(
            "quote requires 1 argument (got {})",
            args.len()
        )));
    }
    Ok(args[0].clone_ref(py))
}

fn if_form(py: Python<'_>, args: &[PyObject], env: &Env) -> PyResult<PyObject> {
    if args.len() != 2 && args.len() != 3 {
        return Err(errors::err(format!(
            "if requires 2 or 3 arguments (got {})",
            args.len()
        )));
    }
    let cond = eval(py, args[0].clone_ref(py), env)?;
    // Clojure truthiness: only nil and false are falsy.
    let is_truthy = !(cond.is_none(py) || is_false(py, &cond)?);
    if is_truthy {
        eval(py, args[1].clone_ref(py), env)
    } else if args.len() == 3 {
        eval(py, args[2].clone_ref(py), env)
    } else {
        Ok(py.None())
    }
}

fn is_false(py: Python<'_>, v: &PyObject) -> PyResult<bool> {
    let b = v.bind(py);
    if let Ok(bl) = b.downcast::<pyo3::types::PyBool>() {
        return Ok(!bl.is_true());
    }
    Ok(false)
}

fn do_form(py: Python<'_>, args: &[PyObject], env: &Env) -> PyResult<PyObject> {
    if args.is_empty() { return Ok(py.None()); }
    let mut result: PyObject = py.None();
    for form in args {
        result = eval(py, form.clone_ref(py), env)?;
    }
    Ok(result)
}

fn let_form(py: Python<'_>, args: &[PyObject], env: &Env) -> PyResult<PyObject> {
    if args.is_empty() {
        return Err(errors::err("let requires a binding vector"));
    }
    let bindings_form = args[0].clone_ref(py);
    let bindings_b = bindings_form.bind(py);
    let bindings_vec = bindings_b.downcast::<PersistentVector>().map_err(|_| {
        errors::err("let: first argument must be a vector of bindings")
    })?;
    let v = bindings_vec.get();
    if v.cnt % 2 != 0 {
        return Err(errors::err("let binding vector must have even length"));
    }
    let mut cur_env = env.clone_with(py);
    let n = v.cnt as usize;
    let mut i = 0;
    while i < n {
        let name_form = v.nth_internal_pub(py, i)?;
        let name_b = name_form.bind(py);
        let sym = name_b.downcast::<Symbol>().map_err(|_| {
            errors::err("let binding name must be a Symbol")
        })?;
        let name_str = sym.get().name.to_string();
        let val_form = v.nth_internal_pub(py, i + 1)?;
        let val = eval(py, val_form, &cur_env)?;
        cur_env = cur_env.extend(py, &name_str, val);
        i += 2;
    }
    // Body forms after the binding vector.
    let body = &args[1..];
    do_form(py, body, &cur_env)
}
