//! clojure.core namespace — stdlib shims.
//!
//! Populates clojure.core at module init with a handful of callable Vars
//! holding plain Python callables or rt::-backed wrappers. Provides the
//! minimum vocabulary for small evaluator tests to work.

use crate::exceptions::IllegalArgumentException;
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyCFunction, PyDict, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

fn sym(name: &str) -> Symbol {
    Symbol::new(None, Arc::from(name))
}

fn make_closure(
    py: Python<'_>,
    f: impl Fn(&Bound<'_, PyTuple>, Python<'_>) -> PyResult<PyObject> + Send + Sync + 'static,
) -> PyResult<PyObject> {
    let wrapper = PyCFunction::new_closure(
        py,
        None,
        None,
        move |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<PyObject> {
            let py = args.py();
            f(args, py)
        },
    )?;
    Ok(wrapper.unbind().into_any())
}

/// Intern `name` in `core_ns` with root = fn, where fn is a closure we wrap as a PyCFunction.
fn intern_fn(
    py: Python<'_>,
    core_ns: &PyObject,
    name: &str,
    f: impl Fn(&Bound<'_, PyTuple>, Python<'_>) -> PyResult<PyObject> + Send + Sync + 'static,
) -> PyResult<()> {
    let callable = make_closure(py, f)?;
    let sym_py = Py::new(py, sym(name))?;
    let var = crate::ns_ops::intern(py, core_ns.clone_ref(py), sym_py)?;
    var.bind(py).call_method1("bind_root", (callable,))?;
    Ok(())
}

pub(crate) fn init(py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Create clojure.core namespace.
    let core_sym = Py::new(py, sym("clojure.core"))?;
    let core_ns = crate::namespace::create_ns(py, core_sym)?;

    // --- Arithmetic ---

    intern_fn(py, &core_ns, "+", |args, py| {
        let mut acc_int: i128 = 0;
        let mut acc_float: f64 = 0.0;
        let mut is_float = false;
        for i in 0..args.len() {
            let a = args.get_item(i)?;
            if a.downcast::<pyo3::types::PyFloat>().is_ok() {
                if !is_float {
                    acc_float = acc_int as f64;
                    is_float = true;
                }
                acc_float += a.extract::<f64>()?;
            } else {
                let n: i128 = a.extract()?;
                if is_float { acc_float += n as f64; } else { acc_int += n; }
            }
        }
        if is_float { Ok(acc_float.into_pyobject(py)?.unbind().into_any()) }
        else { Ok(acc_int.into_pyobject(py)?.unbind().into_any()) }
    })?;

    intern_fn(py, &core_ns, "-", |args, py| {
        if args.len() == 0 {
            return Err(IllegalArgumentException::new_err("- requires at least 1 arg"));
        }
        let first = args.get_item(0)?;
        if args.len() == 1 {
            if first.downcast::<pyo3::types::PyFloat>().is_ok() {
                let f: f64 = first.extract()?;
                return Ok((-f).into_pyobject(py)?.unbind().into_any());
            }
            let n: i128 = first.extract()?;
            return Ok((-n).into_pyobject(py)?.unbind().into_any());
        }
        // Subtract rest from first.
        let mut is_float = first.downcast::<pyo3::types::PyFloat>().is_ok();
        let mut acc_i: i128 = if !is_float { first.extract()? } else { 0 };
        let mut acc_f: f64 = if is_float { first.extract()? } else { 0.0 };
        for i in 1..args.len() {
            let a = args.get_item(i)?;
            if a.downcast::<pyo3::types::PyFloat>().is_ok() {
                if !is_float { acc_f = acc_i as f64; is_float = true; }
                acc_f -= a.extract::<f64>()?;
            } else {
                let n: i128 = a.extract()?;
                if is_float { acc_f -= n as f64; } else { acc_i -= n; }
            }
        }
        if is_float { Ok(acc_f.into_pyobject(py)?.unbind().into_any()) }
        else { Ok(acc_i.into_pyobject(py)?.unbind().into_any()) }
    })?;

    intern_fn(py, &core_ns, "*", |args, py| {
        let mut acc_i: i128 = 1;
        let mut acc_f: f64 = 1.0;
        let mut is_float = false;
        for i in 0..args.len() {
            let a = args.get_item(i)?;
            if a.downcast::<pyo3::types::PyFloat>().is_ok() {
                if !is_float { acc_f = acc_i as f64; is_float = true; }
                acc_f *= a.extract::<f64>()?;
            } else {
                let n: i128 = a.extract()?;
                if is_float { acc_f *= n as f64; } else { acc_i *= n; }
            }
        }
        if is_float { Ok(acc_f.into_pyobject(py)?.unbind().into_any()) }
        else { Ok(acc_i.into_pyobject(py)?.unbind().into_any()) }
    })?;

    intern_fn(py, &core_ns, "/", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err("/ requires at least 2 args"));
        }
        let first: f64 = args.get_item(0)?.extract()?;
        let mut acc = first;
        for i in 1..args.len() {
            acc /= args.get_item(i)?.extract::<f64>()?;
        }
        Ok(acc.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "inc", |args, py| {
        let a = args.get_item(0)?;
        if a.downcast::<pyo3::types::PyFloat>().is_ok() {
            let f: f64 = a.extract()?;
            return Ok((f + 1.0).into_pyobject(py)?.unbind().into_any());
        }
        let n: i128 = a.extract()?;
        Ok((n + 1).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "dec", |args, py| {
        let a = args.get_item(0)?;
        if a.downcast::<pyo3::types::PyFloat>().is_ok() {
            let f: f64 = a.extract()?;
            return Ok((f - 1.0).into_pyobject(py)?.unbind().into_any());
        }
        let n: i128 = a.extract()?;
        Ok((n - 1).into_pyobject(py)?.unbind().into_any())
    })?;

    // --- Comparison ---

    intern_fn(py, &core_ns, "=", |args, py| {
        if args.len() < 2 {
            return Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any());
        }
        let first = args.get_item(0)?.unbind();
        for i in 1..args.len() {
            let x = args.get_item(i)?.unbind();
            if !crate::rt::equiv(py, first.clone_ref(py), x)? {
                return Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any());
            }
        }
        Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "<", |args, py| {
        let mut prev = args.get_item(0)?;
        for i in 1..args.len() {
            let cur = args.get_item(i)?;
            if !prev.lt(&cur)? {
                return Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any());
            }
            prev = cur;
        }
        Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &core_ns, ">", |args, py| {
        let mut prev = args.get_item(0)?;
        for i in 1..args.len() {
            let cur = args.get_item(i)?;
            if !prev.gt(&cur)? {
                return Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any());
            }
            prev = cur;
        }
        Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "<=", |args, py| {
        let mut prev = args.get_item(0)?;
        for i in 1..args.len() {
            let cur = args.get_item(i)?;
            if !prev.le(&cur)? {
                return Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any());
            }
            prev = cur;
        }
        Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &core_ns, ">=", |args, py| {
        let mut prev = args.get_item(0)?;
        for i in 1..args.len() {
            let cur = args.get_item(i)?;
            if !prev.ge(&cur)? {
                return Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any());
            }
            prev = cur;
        }
        Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any())
    })?;

    // --- Logical ---

    intern_fn(py, &core_ns, "not", |args, py| {
        let a = args.get_item(0)?;
        // Clojure truthiness: only nil and false are falsy.
        let falsy = a.is_none()
            || matches!(a.downcast::<pyo3::types::PyBool>(), Ok(b) if !b.is_true());
        Ok(pyo3::types::PyBool::new(py, falsy).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "nil?", |args, py| {
        let a = args.get_item(0)?;
        Ok(pyo3::types::PyBool::new(py, a.is_none()).to_owned().unbind().into_any())
    })?;

    // --- Collections ---

    intern_fn(py, &core_ns, "list", |args, _py| {
        crate::collections::plist::list_(args.py(), args.clone())
    })?;

    intern_fn(py, &core_ns, "vector", |args, _py| {
        let v = crate::collections::pvector::vector(args.py(), args.clone())?;
        Ok(v.into_any())
    })?;

    intern_fn(py, &core_ns, "hash-map", |args, py| {
        let m = crate::collections::phashmap::hash_map(args.py(), args.clone())?;
        Ok(m.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "hash-set", |args, _py| {
        let s = crate::collections::phashset::hash_set(args.py(), args.clone())?;
        Ok(s.into_any())
    })?;

    // --- Seq ops ---

    intern_fn(py, &core_ns, "count", |args, py| {
        let a = args.get_item(0)?.unbind();
        let n = crate::rt::count(py, a)?;
        Ok((n as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &core_ns, "first", |args, py| {
        crate::rt::first(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &core_ns, "rest", |args, py| {
        crate::rt::rest(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &core_ns, "next", |args, py| {
        crate::rt::next_(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &core_ns, "seq", |args, py| {
        crate::rt::seq(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &core_ns, "cons", |args, py| {
        let x = args.get_item(0)?.unbind();
        let coll = args.get_item(1)?.unbind();
        let cons = crate::seqs::cons::Cons::new(x, coll);
        Ok(Py::new(py, cons)?.into_any())
    })?;

    // --- Misc ---

    // --- Bytecode-compiler helpers ---

    // `(bind-root var value)` — used by compiled `def` forms.
    intern_fn(py, &core_ns, "bind-root", |args, py| {
        if args.len() != 2 {
            return Err(IllegalArgumentException::new_err(
                "bind-root requires 2 args: var and value",
            ));
        }
        let var_any = args.get_item(0)?;
        let var = var_any.downcast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("bind-root: first arg must be a Var")
        })?;
        let value = args.get_item(1)?.unbind();
        var.call_method1("bind_root", (value,))?;
        Ok(var.clone().unbind().into_any())
    })?;

    // `(_set-macro! var)` — tags a Var with `:macro true` metadata. Used
    // by `defmacro` expansions.
    intern_fn(py, &core_ns, "_set-macro!", |args, py| {
        if args.len() != 1 {
            return Err(IllegalArgumentException::new_err(
                "_set-macro! requires 1 arg: the Var",
            ));
        }
        let var_any = args.get_item(0)?;
        let var = var_any.downcast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("_set-macro!: arg must be a Var")
        })?;
        var.get().set_macro_flag(py)?;
        Ok(var.clone().unbind().into_any())
    })?;

    // `(_make-closure template capture1 capture2 ...)` — used by compiled `fn*`.
    // Pops a FnTemplate + N captures; returns a runtime Fn.
    intern_fn(py, &core_ns, "_make-closure", |args, py| {
        if args.len() < 1 {
            return Err(IllegalArgumentException::new_err(
                "_make-closure requires at least a template",
            ));
        }
        let template_any = args.get_item(0)?;
        let template = template_any
            .downcast::<crate::compiler::method::FnTemplate>()
            .map_err(|_| {
                IllegalArgumentException::new_err(
                    "_make-closure: first arg must be an FnTemplate",
                )
            })?;
        let t = template.get();
        let mut captures: Vec<PyObject> = Vec::with_capacity(args.len() - 1);
        for i in 1..args.len() {
            captures.push(args.get_item(i)?.unbind());
        }
        let fn_val = crate::eval::fn_value::Fn {
            name: t.name.clone(),
            current_ns: t.current_ns.clone_ref(py),
            captures,
            methods: t.methods.clone(),
            variadic: t.variadic.clone(),
            pool: t.pool.clone(),
        };
        Ok(Py::new(py, fn_val)?.into_any())
    })?;

    intern_fn(py, &core_ns, "str", |args, py| {
        let mut out = String::new();
        for i in 0..args.len() {
            let a = args.get_item(i)?;
            if a.is_none() {
                // Clojure: (str nil) is ""
                continue;
            }
            // Call Python str()
            let builtins = py.import("builtins")?;
            let py_str = builtins.getattr("str")?;
            let s = py_str.call1((a,))?;
            out.push_str(&s.extract::<String>()?);
        }
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    Ok(())
}
