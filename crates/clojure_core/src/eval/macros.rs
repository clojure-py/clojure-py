//! Hardcoded macros — expanded before special-form dispatch.
//!
//! Built in with the compiler; user-defined defmacro is a future spec.

use crate::collections::plist;
use crate::eval::errors;
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Check if `head` is a Symbol naming a built-in macro. Returns the name if so.
pub fn lookup_builtin_macro(head: &PyObject, py: Python<'_>) -> Option<&'static str> {
    let b = head.bind(py);
    let Ok(sym) = b.downcast::<Symbol>() else { return None; };
    let s = sym.get();
    if s.ns.is_some() { return None; }
    match s.name.as_ref() {
        "defn" => Some("defn"),
        "defmacro" => Some("defmacro"),
        "when" => Some("when"),
        "when-not" => Some("when-not"),
        "cond" => Some("cond"),
        "or" => Some("or"),
        "and" => Some("and"),
        _ => None,
    }
}

/// Expand a macro call into a form that eval can then evaluate.
pub fn expand(py: Python<'_>, name: &str, list: PyObject) -> PyResult<PyObject> {
    let items = list_items(py, list)?;
    // items[0] is the macro symbol itself.
    let args = &items[1..];
    match name {
        "defn" => expand_defn(py, args),
        "defmacro" => expand_defmacro(py, args),
        "when" => expand_when(py, args),
        "when-not" => expand_when_not(py, args),
        "cond" => expand_cond(py, args),
        "or" => expand_or(py, args),
        "and" => expand_and(py, args),
        _ => unreachable!("lookup_builtin_macro should have filtered"),
    }
}

/// Collect items of a PersistentList into a Vec.
fn list_items(py: Python<'_>, list: PyObject) -> PyResult<Vec<PyObject>> {
    let mut out: Vec<PyObject> = Vec::new();
    let mut cur = list;
    loop {
        let b = cur.bind(py);
        if b.downcast::<plist::EmptyList>().is_ok() { break; }
        if let Ok(pl) = b.downcast::<plist::PersistentList>() {
            out.push(pl.get().head.clone_ref(py));
            cur = pl.get().tail.clone_ref(py);
            continue;
        }
        break;
    }
    Ok(out)
}

fn sym(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let s = Symbol::new(None, Arc::from(name));
    Ok(Py::new(py, s)?.into_any())
}

/// Make a PersistentList from a slice of forms.
fn make_list(py: Python<'_>, items: &[PyObject]) -> PyResult<PyObject> {
    let tup = PyTuple::new(py, items)?;
    crate::collections::plist::list_(py, tup)
}

/// (defmacro name params body...)
/// →
/// (do (def name (fn name [&form &env p1 p2 ...] body...))
///     (_set-macro! (var name))
///     (var name))
///
/// The `&form` and `&env` params are prepended so the macro can see the
/// full original form and a (currently empty) locals map during expansion.
fn expand_defmacro(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.len() < 2 {
        return Err(errors::err("defmacro requires a name and a parameter vector"));
    }
    let name_form = args[0].clone_ref(py);
    let params_form = args[1].clone_ref(py);
    let body = &args[2..];

    // Prepend &form and &env to the params vector.
    let form_sym = sym(py, "&form")?;
    let env_sym = sym(py, "&env")?;
    let params_b = params_form.bind(py);
    let params_vec = params_b
        .downcast::<crate::collections::pvector::PersistentVector>()
        .map_err(|_| errors::err("defmacro parameters must be a vector"))?;
    let pv_ref = params_vec.get();
    let mut new_params: Vec<PyObject> = Vec::with_capacity(2 + pv_ref.cnt as usize);
    new_params.push(form_sym);
    new_params.push(env_sym);
    for i in 0..(pv_ref.cnt as usize) {
        new_params.push(pv_ref.nth_internal_pub(py, i)?);
    }
    let params_tup = PyTuple::new(py, &new_params)?;
    let new_params_vec =
        crate::collections::pvector::vector(py, params_tup)?.into_any();

    // (fn name [&form &env ...params] body...)
    let mut fn_items: Vec<PyObject> = Vec::with_capacity(3 + body.len());
    fn_items.push(sym(py, "fn")?);
    fn_items.push(name_form.clone_ref(py));
    fn_items.push(new_params_vec);
    for b in body {
        fn_items.push(b.clone_ref(py));
    }
    let fn_form = make_list(py, &fn_items)?;

    // (def name fn_form)
    let def_form = make_list(py, &[sym(py, "def")?, name_form.clone_ref(py), fn_form])?;

    // (var name)
    let var_form = make_list(py, &[sym(py, "var")?, name_form.clone_ref(py)])?;

    // (_set-macro! (var name))
    let set_macro_form = make_list(
        py,
        &[sym(py, "_set-macro!")?, var_form.clone_ref(py)],
    )?;

    // (do def-form set-macro-form (var name))
    make_list(py, &[sym(py, "do")?, def_form, set_macro_form, var_form])
}

/// (defn name params body...) → (def name (fn name params body...))
fn expand_defn(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.len() < 2 {
        return Err(errors::err("defn requires a name and a parameter vector"));
    }
    let name_form = args[0].clone_ref(py);
    let params_form = args[1].clone_ref(py);
    let body = &args[2..];

    // Build (fn name params body...)
    let mut fn_items: Vec<PyObject> = Vec::with_capacity(3 + body.len());
    fn_items.push(sym(py, "fn")?);
    fn_items.push(name_form.clone_ref(py));  // optional name for nicer stack traces
    fn_items.push(params_form);
    for b in body { fn_items.push(b.clone_ref(py)); }
    let fn_form = make_list(py, &fn_items)?;

    // Build (def name (fn ...))
    let def_items = vec![sym(py, "def")?, name_form, fn_form];
    make_list(py, &def_items)
}

/// (when c b...) → (if c (do b...) nil)
fn expand_when(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.is_empty() {
        return Err(errors::err("when requires a test and at least one body form"));
    }
    let cond = args[0].clone_ref(py);
    let body = &args[1..];
    let mut do_items: Vec<PyObject> = Vec::with_capacity(1 + body.len());
    do_items.push(sym(py, "do")?);
    for b in body { do_items.push(b.clone_ref(py)); }
    let do_form = make_list(py, &do_items)?;

    let if_items = vec![sym(py, "if")?, cond, do_form, py.None()];
    make_list(py, &if_items)
}

/// (when-not c b...) → (if c nil (do b...))
fn expand_when_not(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.is_empty() {
        return Err(errors::err("when-not requires a test and at least one body form"));
    }
    let cond = args[0].clone_ref(py);
    let body = &args[1..];
    let mut do_items: Vec<PyObject> = Vec::with_capacity(1 + body.len());
    do_items.push(sym(py, "do")?);
    for b in body { do_items.push(b.clone_ref(py)); }
    let do_form = make_list(py, &do_items)?;

    let if_items = vec![sym(py, "if")?, cond, py.None(), do_form];
    make_list(py, &if_items)
}

/// (cond c1 r1 c2 r2 ... :else rE)
/// → (if c1 r1 (if c2 r2 ... (if :else rE nil) ...))
fn expand_cond(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.is_empty() {
        return Ok(py.None());
    }
    if args.len() % 2 != 0 {
        return Err(errors::err("cond requires an even number of forms"));
    }
    // Build right to left.
    let mut result: PyObject = py.None();
    for chunk in args.chunks(2).rev() {
        let cond = chunk[0].clone_ref(py);
        let then = chunk[1].clone_ref(py);
        let if_items = vec![sym(py, "if")?, cond, then, result];
        result = make_list(py, &if_items)?;
    }
    Ok(result)
}

/// (or) → nil; (or x) → x; (or x & more) → (let [t x] (if t t (or & more)))
fn expand_or(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.is_empty() { return Ok(py.None()); }
    if args.len() == 1 { return Ok(args[0].clone_ref(py)); }
    let head = args[0].clone_ref(py);
    let rest_expansion = expand_or(py, &args[1..])?;
    // Build (let [or__ head] (if or__ or__ rest))
    let t_sym = sym(py, "or__")?;
    // Bindings: [or__ head]
    let bindings_items = vec![t_sym.clone_ref(py), head];
    let bindings_tup = PyTuple::new(py, &bindings_items)?;
    let bindings_vec = crate::collections::pvector::vector(py, bindings_tup)?.into_any();
    // (if or__ or__ rest)
    let if_form = {
        let items = vec![sym(py, "if")?, t_sym.clone_ref(py), t_sym, rest_expansion];
        make_list(py, &items)?
    };
    // (let [or__ head] if_form)
    let let_items = vec![sym(py, "let")?, bindings_vec, if_form];
    make_list(py, &let_items)
}

/// (and) → true; (and x) → x; (and x & more) → (let [t x] (if t (and & more) t))
fn expand_and(py: Python<'_>, args: &[PyObject]) -> PyResult<PyObject> {
    if args.is_empty() {
        return Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any());
    }
    if args.len() == 1 { return Ok(args[0].clone_ref(py)); }
    let head = args[0].clone_ref(py);
    let rest_expansion = expand_and(py, &args[1..])?;
    let t_sym = sym(py, "and__")?;
    let bindings_items = vec![t_sym.clone_ref(py), head];
    let bindings_tup = PyTuple::new(py, &bindings_items)?;
    let bindings_vec = crate::collections::pvector::vector(py, bindings_tup)?.into_any();
    let if_form = {
        let items = vec![sym(py, "if")?, t_sym.clone_ref(py), rest_expansion, t_sym];
        make_list(py, &items)?
    };
    let let_items = vec![sym(py, "let")?, bindings_vec, if_form];
    make_list(py, &let_items)
}
