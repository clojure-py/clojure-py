//! Core pr_str implementation — renders any value to Clojure's read-compatible textual form.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyFloat, PyInt, PyString};

type PyObject = Py<PyAny>;

/// Print `x` to a string in reader-compatible form (strings quoted).
pub fn pr_str(py: Python<'_>, x: PyObject) -> PyResult<String> {
    pr_str_with(py, x, true)
}

/// Print `x` to a string in non-readable form (strings un-quoted) — backs
/// `print-str` / `print` / `println`.
pub fn print_str(py: Python<'_>, x: PyObject) -> PyResult<String> {
    pr_str_with(py, x, false)
}

fn pr_str_with(py: Python<'_>, x: PyObject, readable: bool) -> PyResult<String> {
    let b = x.bind(py);

    // nil
    if b.is_none() {
        return Ok("nil".to_string());
    }

    // Booleans (check BEFORE ints — PyBool is a subclass of PyInt in CPython).
    if let Ok(bv) = b.cast::<PyBool>() {
        return Ok(if bv.is_true() { "true".to_string() } else { "false".to_string() });
    }

    // Our types.
    if let Ok(sym) = b.cast::<crate::symbol::Symbol>() {
        let s = sym.get();
        return match s.ns.as_deref() {
            Some(ns) => Ok(format!("{}/{}", ns, s.name)),
            None => Ok(s.name.to_string()),
        };
    }

    if let Ok(kw) = b.cast::<crate::keyword::Keyword>() {
        let k = kw.get();
        return match k.ns.as_deref() {
            Some(ns) => Ok(format!(":{}/{}", ns, k.name)),
            None => Ok(format!(":{}", k.name)),
        };
    }

    if let Ok(ch) = b.cast::<crate::char::Char>() {
        let c = ch.get().value;
        return Ok(if readable {
            crate::char::named_or_escaped(c)
        } else {
            c.to_string()
        });
    }

    // PersistentList
    if let Ok(pl) = b.cast::<crate::collections::plist::PersistentList>() {
        return pr_list(py, pl.clone().unbind().into_any(), readable);
    }
    if b.cast::<crate::collections::plist::EmptyList>().is_ok() {
        return Ok("()".to_string());
    }

    // PersistentVector
    if let Ok(pv) = b.cast::<crate::collections::pvector::PersistentVector>() {
        return pr_vector(py, pv.get(), readable);
    }

    // PersistentHashMap / ArrayMap / PersistentTreeMap
    if b.cast::<crate::collections::phashmap::PersistentHashMap>().is_ok()
        || b.cast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
        || b.cast::<crate::collections::ptreemap::PersistentTreeMap>().is_ok()
    {
        return pr_map(py, x, readable);
    }

    // PersistentHashSet / PersistentTreeSet
    if b.cast::<crate::collections::phashset::PersistentHashSet>().is_ok()
        || b.cast::<crate::collections::ptreeset::PersistentTreeSet>().is_ok()
    {
        return pr_set(py, x, readable);
    }

    // Cons / LazySeq / VectorSeq — print as (a b c)
    if b.cast::<crate::seqs::cons::Cons>().is_ok()
        || b.cast::<crate::seqs::lazy_seq::LazySeq>().is_ok()
        || b.cast::<crate::seqs::vector_seq::VectorSeq>().is_ok()
    {
        return pr_seq(py, x, readable);
    }

    // Var — print as #'ns/sym
    if let Ok(var) = b.cast::<crate::var::Var>() {
        // Var's pymethod __repr__ returns #'ns/sym; just use it.
        let r = var.call_method0("__repr__")?;
        return Ok(r.extract::<String>()?);
    }

    // Python primitives.
    if let Ok(iv) = b.cast::<PyInt>() {
        return Ok(iv.extract::<i128>().map(|n| n.to_string())
            .or_else(|_| iv.call_method0("__repr__").and_then(|r| r.extract::<String>()))?);
    }
    if let Ok(fv) = b.cast::<PyFloat>() {
        let f = fv.extract::<f64>()?;
        // Clojure-style: 1.0 prints as 1.0 (Python's default too).
        return Ok(format!("{}", f_display(f)));
    }
    if let Ok(s) = b.cast::<PyString>() {
        let raw = s.extract::<String>()?;
        return Ok(if readable { escape_string(&raw) } else { raw });
    }

    // Fallback: call Python repr.
    let r = b.repr()?;
    let s = r.extract::<String>()?;
    Ok(s)
}

fn f_display(f: f64) -> String {
    // Use Python's repr so 3.14 stays 3.14 (not 3.14000000000001 etc).
    // Python's str(float) is generally good enough.
    // From Rust, {:?} produces 3.14 for 3.14; use that.
    let s = format!("{:?}", f);
    // Rust's {:?} on f64 writes "3.14" as "3.14" (good). For NaN: "NaN" — Clojure uses "##NaN" via tagged-literal; out of scope.
    s
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn pr_list(py: Python<'_>, lst: PyObject, readable: bool) -> PyResult<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur: PyObject = lst;
    loop {
        let b = cur.bind(py);
        if b.cast::<crate::collections::plist::EmptyList>().is_ok() {
            break;
        }
        if let Ok(pl) = b.cast::<crate::collections::plist::PersistentList>() {
            let head = pl.get().head.clone_ref(py);
            parts.push(pr_str_with(py, head, readable)?);
            cur = pl.get().tail.clone_ref(py);
            continue;
        }
        break;
    }
    Ok(format!("({})", parts.join(" ")))
}

fn pr_vector(
    py: Python<'_>,
    v: &crate::collections::pvector::PersistentVector,
    readable: bool,
) -> PyResult<String> {
    let mut parts: Vec<String> = Vec::with_capacity(v.cnt as usize);
    for i in 0..(v.cnt as usize) {
        let item = v.nth_internal_pub(py, i)?;
        parts.push(pr_str_with(py, item, readable)?);
    }
    Ok(format!("[{}]", parts.join(" ")))
}

fn pr_map(py: Python<'_>, m: PyObject, readable: bool) -> PyResult<String> {
    // Iterate via Python __iter__ (yields keys for hash/array maps, MapEntries
    // for tree maps). Unify by treating the iter item as either a MapEntry or
    // a bare key (fetch value via val_at).
    let b = m.bind(py);
    let iter = b.try_iter()?;
    let mut parts: Vec<String> = Vec::new();
    for item in iter {
        let it = item?;
        // If it's a MapEntry (tree-map case), extract its key/val directly.
        if let Ok(me) = it.cast::<crate::collections::map_entry::MapEntry>() {
            let ke = me.get().key.clone_ref(py);
            let ve = me.get().val.clone_ref(py);
            let ks = pr_str_with(py, ke, readable)?;
            let vs = pr_str_with(py, ve, readable)?;
            parts.push(format!("{} {}", ks, vs));
            continue;
        }
        let k = it.unbind();
        let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
        let ks = pr_str_with(py, k, readable)?;
        let vs = pr_str_with(py, v, readable)?;
        parts.push(format!("{} {}", ks, vs));
    }
    Ok(format!("{{{}}}", parts.join(", ")))
}

fn pr_set(py: Python<'_>, s: PyObject, readable: bool) -> PyResult<String> {
    let b = s.bind(py);
    let iter = b.try_iter()?;
    let mut parts: Vec<String> = Vec::new();
    for item in iter {
        let v = item?.unbind();
        parts.push(pr_str_with(py, v, readable)?);
    }
    Ok(format!("#{{{}}}", parts.join(" ")))
}

fn pr_seq(py: Python<'_>, s: PyObject, readable: bool) -> PyResult<String> {
    // Use rt::first/rt::next_ to walk.
    let mut parts: Vec<String> = Vec::new();
    let mut cur: PyObject = crate::rt::seq(py, s)?;
    loop {
        if cur.is_none(py) { break; }
        let head = crate::rt::first(py, cur.clone_ref(py))?;
        parts.push(pr_str_with(py, head, readable)?);
        cur = crate::rt::next_(py, cur)?;
    }
    Ok(format!("({})", parts.join(" ")))
}
