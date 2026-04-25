//! Core pr_str implementation — renders any value to Clojure's read-compatible textual form.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyFloat, PyInt, PyString};

type PyObject = Py<PyAny>;

#[derive(Clone, Copy)]
struct PrintCtx {
    /// Max collection elements before truncation. None = no limit.
    length: Option<i64>,
    /// Max recursion depth. None = no limit. A collection at depth >= level
    /// prints as "#".
    level: Option<i64>,
    /// Print meta as `^M val`?
    meta: bool,
    /// Use #:ns{...} short form for same-ns keyword-key maps?
    ns_maps: bool,
    /// Readable form (for strings, chars).
    readable: bool,
    /// Current recursion depth (0 at top-level call).
    depth: i64,
}

impl PrintCtx {
    fn deeper(self) -> Self {
        Self { depth: self.depth + 1, ..self }
    }
}

/// Print `x` to a string in reader-compatible form (strings quoted).
pub fn pr_str(py: Python<'_>, x: PyObject) -> PyResult<String> {
    let ctx = read_print_vars(py, true);
    pr_str_ctx(py, x, ctx)
}

/// Print `x` to a string in non-readable form (strings un-quoted) — backs
/// `print-str` / `print` / `println`.
pub fn print_str(py: Python<'_>, x: PyObject) -> PyResult<String> {
    let ctx = read_print_vars(py, false);
    pr_str_ctx(py, x, ctx)
}

/// Read the four print-* dynamic vars at top-level entry. Defensive: any
/// failure (var missing during early bootstrap, type mismatch, etc.) falls
/// back to the vanilla default.
fn read_print_vars(py: Python<'_>, readable: bool) -> PrintCtx {
    let length = read_int_var(py, "*print-length*").unwrap_or(None);
    let level = read_int_var(py, "*print-level*").unwrap_or(None);
    let meta = read_bool_var(py, "*print-meta*").unwrap_or(false);
    let ns_maps = read_bool_var(py, "*print-namespace-maps*").unwrap_or(false);
    PrintCtx { length, level, meta, ns_maps, readable, depth: 0 }
}

fn read_int_var(py: Python<'_>, name: &str) -> PyResult<Option<i64>> {
    let core = py.import("clojure.core")?;
    let v = match core.getattr(name) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let var = match v.cast::<crate::var::Var>() {
        Ok(var) => var,
        Err(_) => return Ok(None),
    };
    let py_var: Py<crate::var::Var> = var.clone().unbind();
    let val = crate::var::Var::deref_fast(&py_var, py)?;
    if val.is_none(py) {
        Ok(None)
    } else {
        Ok(Some(val.bind(py).extract::<i64>()?))
    }
}

fn read_bool_var(py: Python<'_>, name: &str) -> PyResult<bool> {
    let core = py.import("clojure.core")?;
    let v = match core.getattr(name) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let var = match v.cast::<crate::var::Var>() {
        Ok(var) => var,
        Err(_) => return Ok(false),
    };
    let py_var: Py<crate::var::Var> = var.clone().unbind();
    let val = crate::var::Var::deref_fast(&py_var, py)?;
    if val.is_none(py) {
        Ok(false)
    } else {
        Ok(val.bind(py).is_truthy()?)
    }
}

fn pr_str_ctx(py: Python<'_>, x: PyObject, ctx: PrintCtx) -> PyResult<String> {
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
        return Ok(if ctx.readable {
            crate::char::named_or_escaped(c)
        } else {
            c.to_string()
        });
    }

    // PersistentList
    if let Ok(pl) = b.cast::<crate::collections::plist::PersistentList>() {
        return pr_list(py, pl.clone().unbind().into_any(), ctx);
    }
    if b.cast::<crate::collections::plist::EmptyList>().is_ok() {
        return Ok("()".to_string());
    }

    // PersistentVector
    if let Ok(pv) = b.cast::<crate::collections::pvector::PersistentVector>() {
        return pr_vector(py, pv.get(), ctx);
    }

    // PersistentHashMap / ArrayMap / PersistentTreeMap
    if b.cast::<crate::collections::phashmap::PersistentHashMap>().is_ok()
        || b.cast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
        || b.cast::<crate::collections::ptreemap::PersistentTreeMap>().is_ok()
    {
        return pr_map(py, x, ctx);
    }

    // PersistentHashSet / PersistentTreeSet
    if b.cast::<crate::collections::phashset::PersistentHashSet>().is_ok()
        || b.cast::<crate::collections::ptreeset::PersistentTreeSet>().is_ok()
    {
        return pr_set(py, x, ctx);
    }

    // Cons / LazySeq / VectorSeq — print as (a b c)
    if b.cast::<crate::seqs::cons::Cons>().is_ok()
        || b.cast::<crate::seqs::lazy_seq::LazySeq>().is_ok()
        || b.cast::<crate::seqs::vector_seq::VectorSeq>().is_ok()
    {
        return pr_seq(py, x, ctx);
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
        return Ok(if ctx.readable { escape_string(&raw) } else { raw });
    }

    // fractions.Fraction → "numerator/denominator". Vanilla Ratio.toString.
    let py_inner = b.py();
    let fractions = py_inner.import("fractions")?;
    let frac_cls = fractions.getattr("Fraction")?;
    if b.is_instance(&frac_cls)? {
        let n = b.getattr("numerator")?;
        let d = b.getattr("denominator")?;
        let n_s: String = n.str()?.extract()?;
        let d_s: String = d.str()?.extract()?;
        return Ok(format!("{}/{}", n_s, d_s));
    }

    // Fallback: call Python repr.
    let r = b.repr()?;
    let s = r.extract::<String>()?;
    Ok(s)
}

fn f_display(f: f64) -> String {
    if f.is_nan() {
        return "##NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "##Inf".to_string() } else { "##-Inf".to_string() };
    }
    // Use Rust's {:?} so 3.14 stays "3.14" (and 1.0 stays "1.0"). Matches
    // Clojure's printer: integer-valued floats include the decimal so the
    // form round-trips through the reader as a Double.
    format!("{:?}", f)
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

fn pr_list(py: Python<'_>, lst: PyObject, ctx: PrintCtx) -> PyResult<String> {
    let limit = ctx.length.map(|l| l.max(0) as usize);
    let mut parts: Vec<String> = Vec::new();
    let mut cur: PyObject = lst;
    let mut count: usize = 0;
    let mut more = false;
    loop {
        let b = cur.bind(py);
        if b.cast::<crate::collections::plist::EmptyList>().is_ok() {
            break;
        }
        if let Some(lim) = limit {
            if count >= lim { more = true; break; }
        }
        if let Ok(pl) = b.cast::<crate::collections::plist::PersistentList>() {
            let head = pl.get().head.clone_ref(py);
            parts.push(pr_str_ctx(py, head, ctx.deeper())?);
            cur = pl.get().tail.clone_ref(py);
            count += 1;
            continue;
        }
        break;
    }
    if more { parts.push("...".to_string()); }
    Ok(format!("({})", parts.join(" ")))
}

fn pr_vector(
    py: Python<'_>,
    v: &crate::collections::pvector::PersistentVector,
    ctx: PrintCtx,
) -> PyResult<String> {
    let n = v.cnt as usize;
    let limit = ctx.length.map(|l| l.max(0) as usize).unwrap_or(n);
    let to_print = limit.min(n);
    let mut parts: Vec<String> = Vec::with_capacity(to_print + 1);
    for i in 0..to_print {
        let item = v.nth_internal_pub(py, i)?;
        parts.push(pr_str_ctx(py, item, ctx.deeper())?);
    }
    if n > to_print {
        parts.push("...".to_string());
    }
    Ok(format!("[{}]", parts.join(" ")))
}

fn pr_map(py: Python<'_>, m: PyObject, ctx: PrintCtx) -> PyResult<String> {
    // Iterate via Python __iter__ (yields keys for hash/array maps, MapEntries
    // for tree maps). Unify by treating the iter item as either a MapEntry or
    // a bare key (fetch value via val_at).
    let limit = ctx.length.map(|l| l.max(0) as usize);
    let b = m.bind(py);
    let iter = b.try_iter()?;
    let mut parts: Vec<String> = Vec::new();
    let mut count: usize = 0;
    let mut more = false;
    for item in iter {
        if let Some(lim) = limit {
            if count >= lim { more = true; break; }
        }
        let it = item?;
        // If it's a MapEntry (tree-map case), extract its key/val directly.
        if let Ok(me) = it.cast::<crate::collections::map_entry::MapEntry>() {
            let ke = me.get().key.clone_ref(py);
            let ve = me.get().val.clone_ref(py);
            let ks = pr_str_ctx(py, ke, ctx.deeper())?;
            let vs = pr_str_ctx(py, ve, ctx.deeper())?;
            parts.push(format!("{} {}", ks, vs));
            count += 1;
            continue;
        }
        let k = it.unbind();
        let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
        let ks = pr_str_ctx(py, k, ctx.deeper())?;
        let vs = pr_str_ctx(py, v, ctx.deeper())?;
        parts.push(format!("{} {}", ks, vs));
        count += 1;
    }
    if more { parts.push("...".to_string()); }
    Ok(format!("{{{}}}", parts.join(", ")))
}

fn pr_set(py: Python<'_>, s: PyObject, ctx: PrintCtx) -> PyResult<String> {
    let limit = ctx.length.map(|l| l.max(0) as usize);
    let b = s.bind(py);
    let iter = b.try_iter()?;
    let mut parts: Vec<String> = Vec::new();
    let mut count: usize = 0;
    let mut more = false;
    for item in iter {
        if let Some(lim) = limit {
            if count >= lim { more = true; break; }
        }
        let v = item?.unbind();
        parts.push(pr_str_ctx(py, v, ctx.deeper())?);
        count += 1;
    }
    if more { parts.push("...".to_string()); }
    Ok(format!("#{{{}}}", parts.join(" ")))
}

fn pr_seq(py: Python<'_>, s: PyObject, ctx: PrintCtx) -> PyResult<String> {
    // Use rt::first/rt::next_ to walk.
    let limit = ctx.length.map(|l| l.max(0) as usize);
    let mut parts: Vec<String> = Vec::new();
    let mut cur: PyObject = crate::rt::seq(py, s)?;
    let mut count: usize = 0;
    let mut more = false;
    loop {
        if cur.is_none(py) { break; }
        if let Some(lim) = limit {
            if count >= lim { more = true; break; }
        }
        let head = crate::rt::first(py, cur.clone_ref(py))?;
        parts.push(pr_str_ctx(py, head, ctx.deeper())?);
        cur = crate::rt::next_(py, cur)?;
        count += 1;
    }
    if more { parts.push("...".to_string()); }
    Ok(format!("({})", parts.join(" ")))
}
