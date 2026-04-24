//! Collection readers — lists, vectors, maps, sets.
//!
//! Each reader collects child forms into a `Vec<PyObject>`, then delegates to
//! the corresponding collection constructor (`list_`, `vector`, `array_map`,
//! `hash_set`). Those constructors already use the efficient internal
//! builders (`conj_internal` / `assoc_internal`), and `array_map` auto-
//! promotes to `PersistentHashMap` once the threshold is crossed.

use crate::reader::dispatch;
use crate::reader::errors;
use crate::reader::source::Source;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

type PyObject = Py<PyAny>;

/// Monotonic counter for auto-gensym (and the underlying `gensym` fn).
static GENSYM_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn gensym_id() -> u64 {
    GENSYM_COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Reader macros (Phase R4)
// ---------------------------------------------------------------------------

/// Advance past `lead`, read one form, and wrap it as `(wrapper form)`.
/// `wrapper_ns` is the namespace of the wrapping symbol (`None` for special
/// forms like `quote`/`var`; `Some("clojure.core")` for real Vars like
/// `deref`, to match Clojure/JVM and prevent local shadowing).
fn wrap_next_form(
    src: &mut Source<'_>,
    py: Python<'_>,
    lead: char,
    wrapper_ns: Option<&str>,
    wrapper_name: &str,
) -> PyResult<PyObject> {
    let ch = src.advance();
    debug_assert_eq!(ch, Some(lead));
    let form = dispatch::read_one(src, py)?;
    let ns_arc = wrapper_ns.map(std::sync::Arc::from);
    let sym = crate::symbol::Symbol::new(ns_arc, std::sync::Arc::from(wrapper_name));
    let sym_py: PyObject = Py::new(py, sym)?.into_any();
    let args = PyTuple::new(py, &[sym_py, form])?;
    crate::collections::plist::list_(py, args)
}

/// `'form` → `(quote form)`. Caller has NOT consumed the leading `'`.
pub fn quote_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    wrap_next_form(src, py, '\'', None, "quote")
}

/// `@form` → `(clojure.core/deref form)`. Caller has NOT consumed the leading `@`.
pub fn deref_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    wrap_next_form(src, py, '@', Some("clojure.core"), "deref")
}

/// `#'sym` → `(var sym)`. Caller has consumed `#` only; the next char is `'`.
pub fn var_quote_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    wrap_next_form(src, py, '\'', None, "var")
}

/// `^meta form` or `#^meta form` — read meta then target; attach meta to
/// target. Caller has already consumed the `^` (for bare form) or the `#^`
/// pair (for the dispatch form).
pub fn meta_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let meta_raw = dispatch::read_one(src, py)?;
    let meta_map = normalize_meta(py, meta_raw, start_line, start_col)?;
    let target = dispatch::read_one(src, py)?;
    attach_meta(py, target, meta_map)
}

/// `#_ form next` — discard `form`, then read and return `next`. Caller has
/// already consumed `#_`.
pub fn discard_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let _discarded = dispatch::read_one(src, py)?;
    dispatch::read_one(src, py)
}

fn normalize_meta(
    py: Python<'_>,
    meta_raw: PyObject,
    line: u32,
    col: u32,
) -> PyResult<PyObject> {
    let b = meta_raw.bind(py);
    // If it's already a map, return as-is.
    if b.cast::<crate::collections::phashmap::PersistentHashMap>().is_ok()
        || b.cast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
    {
        return Ok(meta_raw);
    }
    // Keyword → {kw true}
    if b.cast::<crate::keyword::Keyword>().is_ok() {
        let true_py: PyObject =
            pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any();
        let pair = PyTuple::new(py, &[meta_raw, true_py])?;
        return crate::collections::parraymap::array_map(py, pair);
    }
    // String or Symbol → {:tag <meta>}
    if b.cast::<pyo3::types::PyString>().is_ok()
        || b.cast::<crate::symbol::Symbol>().is_ok()
    {
        let tag_kw = crate::keyword::keyword(py, "tag", None)?;
        let tag_py: PyObject = tag_kw.into_any();
        let pair = PyTuple::new(py, &[tag_py, meta_raw])?;
        return crate::collections::parraymap::array_map(py, pair);
    }
    Err(errors::make(
        "Metadata must be a map, keyword, string, or symbol",
        line,
        col,
    ))
}

fn attach_meta(py: Python<'_>, target: PyObject, meta_map: PyObject) -> PyResult<PyObject> {
    // Route through the IMeta protocol (rt::with_meta) rather than a direct
    // pymethod lookup — collection/seq types now only expose `with_meta`
    // through the IMeta protocol impl, not as a standalone pymethod.
    // Types that don't implement IMeta at all fall back to returning `target`
    // unchanged so reader metadata is silently ignored on non-meta-bearing
    // values (e.g. literal ints).
    match crate::rt::with_meta(py, target.clone_ref(py), meta_map) {
        Ok(new_target) => Ok(new_target),
        Err(_) => Ok(target),
    }
}

/// Read a list: caller has NOT yet consumed the opening '('.
pub fn list_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let open = src.advance();
    debug_assert_eq!(open, Some('('));

    let mut items: Vec<PyObject> = Vec::new();
    loop {
        dispatch::skip_ws_and_comments(src);
        match src.peek() {
            Some(')') => {
                src.advance();
                let tup = PyTuple::new(py, &items)?;
                return crate::collections::plist::list_(py, tup);
            }
            Some(']') | Some('}') => {
                let ch = src.peek().unwrap();
                return Err(errors::make(
                    format!("Unmatched delimiter: expected ')' but got '{}'", ch),
                    src.line(),
                    src.column(),
                ));
            }
            None => {
                return Err(errors::make(
                    "EOF while reading list",
                    start_line,
                    start_col,
                ));
            }
            _ => {
                let el = dispatch::read_one(src, py)?;
                items.push(el);
            }
        }
    }
}

/// Read a vector: caller has NOT yet consumed the '['.
pub fn vector_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let open = src.advance();
    debug_assert_eq!(open, Some('['));

    let mut items: Vec<PyObject> = Vec::new();
    loop {
        dispatch::skip_ws_and_comments(src);
        match src.peek() {
            Some(']') => {
                src.advance();
                let tup = PyTuple::new(py, &items)?;
                let v = crate::collections::pvector::vector(py, tup)?;
                return Ok(v.into_any());
            }
            Some(')') | Some('}') => {
                let ch = src.peek().unwrap();
                return Err(errors::make(
                    format!("Unmatched delimiter: expected ']' but got '{}'", ch),
                    src.line(),
                    src.column(),
                ));
            }
            None => {
                return Err(errors::make(
                    "EOF while reading vector",
                    start_line,
                    start_col,
                ));
            }
            _ => {
                let el = dispatch::read_one(src, py)?;
                items.push(el);
            }
        }
    }
}

/// Read a map: caller has NOT yet consumed the '{'.
///
/// Uses the `array_map` constructor, which auto-promotes to
/// `PersistentHashMap` once the entry count exceeds the small-map threshold.
pub fn map_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let open = src.advance();
    debug_assert_eq!(open, Some('{'));

    let mut pairs: Vec<PyObject> = Vec::new();
    loop {
        dispatch::skip_ws_and_comments(src);
        match src.peek() {
            Some('}') => {
                src.advance();
                if pairs.len() % 2 != 0 {
                    return Err(errors::make(
                        "Map literal must have an even number of forms",
                        start_line,
                        start_col,
                    ));
                }
                let tup = PyTuple::new(py, &pairs)?;
                return crate::collections::parraymap::array_map(py, tup);
            }
            Some(')') | Some(']') => {
                let ch = src.peek().unwrap();
                return Err(errors::make(
                    format!("Unmatched delimiter: expected '}}' but got '{}'", ch),
                    src.line(),
                    src.column(),
                ));
            }
            None => {
                return Err(errors::make(
                    "EOF while reading map",
                    start_line,
                    start_col,
                ));
            }
            _ => {
                let el = dispatch::read_one(src, py)?;
                pairs.push(el);
            }
        }
    }
}

/// `#(body ...)` → `(fn [%1 %2 ... & %&] body ...)`.
///
/// Caller has consumed `#`; next char is `(`. We:
///   1. Read the inner body as a list.
///   2. Walk the body recursively, collecting the highest `%N` seen and
///      whether `%&` appears. Bare `%` is treated as `%1`.
///   3. Synthesize an arglist `[%1 %2 … %N & %&]` (omitting variadic portion
///      if `%&` was absent) and wrap the body in `(fn [args] body)`.
///
/// Nested `#(...)` is rejected (matches Clojure's behavior).
pub fn anon_fn_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    use crate::collections::plist::list_;
    use crate::collections::pvector::vector;
    use crate::symbol::Symbol;

    let start_line = src.line();
    let start_col = src.column();
    // At this point `src.peek() == Some('(')`. list_reader expects `(` still.
    let body_list = list_reader(src, py)?;

    // Walk the body form to count %-params. Errors on nested #() by noticing
    // a sentinel; since we don't actually emit one, detect by walking.
    let mut max_n: usize = 0;
    let mut has_rest = false;
    scan_anon_params(py, &body_list, &mut max_n, &mut has_rest)
        .map_err(|e| errors::make(e, start_line, start_col))?;

    // Build the arglist: [%1 %2 ... %N] plus optionally [& %&].
    let mut arg_items: Vec<PyObject> = Vec::new();
    for i in 1..=max_n {
        let s = Symbol::new(None, Arc::from(format!("%{}", i).as_str()));
        arg_items.push(Py::new(py, s)?.into_any());
    }
    if has_rest {
        let amp = Symbol::new(None, Arc::from("&"));
        let rest = Symbol::new(None, Arc::from("%&"));
        arg_items.push(Py::new(py, amp)?.into_any());
        arg_items.push(Py::new(py, rest)?.into_any());
    }
    let arg_vec = vector(py, PyTuple::new(py, &arg_items)?)?;

    // Rewrite `%` → `%1` in the body (since both refer to the first arg).
    let body_rewritten = if max_n >= 1 {
        rewrite_bare_percent(py, body_list)?
    } else {
        body_list
    };

    // Build `(fn arg_vec body)`. The list read after `#(` is the single
    // body form — `#(+ 1 %)` becomes `(fn [%1] (+ 1 %1))`, not
    // `(fn [%1] + 1 %1)`.
    let fn_sym = Symbol::new(None, Arc::from("fn"));
    let fn_sym_py: PyObject = Py::new(py, fn_sym)?.into_any();
    let outer_items: Vec<PyObject> =
        vec![fn_sym_py, arg_vec.into_any(), body_rewritten];
    list_(py, PyTuple::new(py, &outer_items)?)
}

/// Walk `form` (any Clojure data) and update `max_n` / `has_rest` for any
/// `%`, `%N`, or `%&` symbol found. Errors if a nested `#()` form is
/// detected (via a second anon-fn's generated arglist containing `%1`…
/// which is indistinguishable at read-time — Clojure forbids nesting and
/// we do too, though we don't have a robust nested-detect; just recurse).
fn scan_anon_params(
    py: Python<'_>,
    form: &PyObject,
    max_n: &mut usize,
    has_rest: &mut bool,
) -> Result<(), String> {
    use crate::collections::{
        parraymap::PersistentArrayMap, phashmap::PersistentHashMap,
        phashset::PersistentHashSet, plist::PersistentList, pvector::PersistentVector,
    };
    use crate::symbol::Symbol;

    let b = form.bind(py);
    if let Ok(sym_ref) = b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            let name = s.name.as_ref();
            if name == "%" {
                if *max_n < 1 { *max_n = 1; }
            } else if name == "%&" {
                *has_rest = true;
            } else if let Some(rest) = name.strip_prefix('%') {
                if let Ok(n) = rest.parse::<usize>() {
                    if n >= 1 && n > *max_n { *max_n = n; }
                }
            }
        }
        return Ok(());
    }
    if let Ok(pl) = b.cast::<PersistentList>() {
        // Walk via seq.
        let mut cur: PyObject = pl.clone().unbind().into_any();
        loop {
            let sb = crate::rt::seq(py, cur.clone_ref(py))
                .map_err(|e| format!("scan: {}", e))?;
            if sb.is_none(py) { break; }
            let head = crate::rt::first(py, sb.clone_ref(py))
                .map_err(|e| format!("scan: {}", e))?;
            scan_anon_params(py, &head, max_n, has_rest)?;
            cur = crate::rt::next_(py, sb)
                .map_err(|e| format!("scan: {}", e))?;
            if cur.is_none(py) { break; }
        }
        return Ok(());
    }
    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        for i in 0..(pv_ref.cnt as usize) {
            let el = pv_ref
                .nth_internal_pub(py, i)
                .map_err(|e| format!("scan: {}", e))?;
            scan_anon_params(py, &el, max_n, has_rest)?;
        }
        return Ok(());
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        for item in b.try_iter().map_err(|e| format!("scan: {}", e))? {
            let k = item.map_err(|e| format!("scan: {}", e))?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))
                .map_err(|e| format!("scan: {}", e))?.unbind();
            scan_anon_params(py, &k, max_n, has_rest)?;
            scan_anon_params(py, &v, max_n, has_rest)?;
        }
        return Ok(());
    }
    if let Ok(ps) = b.cast::<PersistentHashSet>() {
        let _ = ps;
        for item in b.try_iter().map_err(|e| format!("scan: {}", e))? {
            let el = item.map_err(|e| format!("scan: {}", e))?.unbind();
            scan_anon_params(py, &el, max_n, has_rest)?;
        }
        return Ok(());
    }
    // Primitive / other — nothing to scan.
    Ok(())
}

/// Replace any bare `%` symbol (no ns, name == "%") in `form` with `%1`.
/// Walks collections recursively, preserving collection types.
fn rewrite_bare_percent(py: Python<'_>, form: PyObject) -> PyResult<PyObject> {
    use crate::collections::{
        parraymap::PersistentArrayMap, phashmap::PersistentHashMap,
        phashset::{self, PersistentHashSet}, plist::{self, PersistentList},
        pvector::{self, PersistentVector},
    };
    use crate::symbol::Symbol;

    let b = form.bind(py);
    if let Ok(sym_ref) = b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() && s.name.as_ref() == "%" {
            let s1 = Symbol::new(None, Arc::from("%1"));
            return Ok(Py::new(py, s1)?.into_any());
        }
        return Ok(form);
    }
    if let Ok(pl) = b.cast::<PersistentList>() {
        let mut items: Vec<PyObject> = Vec::new();
        let mut cur: PyObject = pl.clone().unbind().into_any();
        loop {
            let sb = crate::rt::seq(py, cur.clone_ref(py))?;
            if sb.is_none(py) { break; }
            let head = crate::rt::first(py, sb.clone_ref(py))?;
            items.push(rewrite_bare_percent(py, head)?);
            cur = crate::rt::next_(py, sb)?;
            if cur.is_none(py) { break; }
        }
        return plist::list_(py, PyTuple::new(py, &items)?);
    }
    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        let mut items: Vec<PyObject> = Vec::with_capacity(pv_ref.cnt as usize);
        for i in 0..(pv_ref.cnt as usize) {
            let el = pv_ref.nth_internal_pub(py, i)?;
            items.push(rewrite_bare_percent(py, el)?);
        }
        let v = pvector::vector(py, PyTuple::new(py, &items)?)?;
        return Ok(v.into_any());
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        let mut kvs: Vec<PyObject> = Vec::new();
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            kvs.push(rewrite_bare_percent(py, k)?);
            kvs.push(rewrite_bare_percent(py, v)?);
        }
        let m = crate::collections::phashmap::hash_map(py, PyTuple::new(py, &kvs)?)?;
        return Ok(m.into_any());
    }
    if b.cast::<PersistentHashSet>().is_ok() {
        let mut items: Vec<PyObject> = Vec::new();
        for item in b.try_iter()? {
            items.push(rewrite_bare_percent(py, item?.unbind())?);
        }
        let s = phashset::hash_set(py, PyTuple::new(py, &items)?)?;
        return Ok(s.into_any());
    }
    Ok(form)
}

/// `#"pattern"` — regex literal. Reads the pattern string and constructs a
/// compiled Python regex via `re.compile`. Matches `(re-pattern "...")`.
pub fn regex_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let quote = src.advance();
    debug_assert_eq!(quote, Some('"'));

    let mut pattern = String::new();
    loop {
        match src.advance() {
            Some('"') => break,
            Some('\\') => {
                // Regex strings preserve the backslash: `\d` stays `\d`, not
                // the escaped char. But we still need to let `\"` escape a
                // quote inside the pattern.
                match src.advance() {
                    Some('"') => pattern.push('"'),
                    Some(c) => {
                        pattern.push('\\');
                        pattern.push(c);
                    }
                    None => {
                        return Err(errors::make(
                            "EOF inside regex literal (after backslash)",
                            start_line,
                            start_col,
                        ));
                    }
                }
            }
            Some(c) => pattern.push(c),
            None => {
                return Err(errors::make(
                    "EOF inside regex literal",
                    start_line,
                    start_col,
                ));
            }
        }
    }
    let re_mod = py.import("re")?;
    let pat = re_mod.call_method1("compile", (pattern,))?;
    Ok(pat.unbind())
}

/// Read a set: caller has already consumed '#' and the next char is '{'.
pub fn set_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let open = src.advance();
    debug_assert_eq!(open, Some('{'));

    let mut items: Vec<PyObject> = Vec::new();
    loop {
        dispatch::skip_ws_and_comments(src);
        match src.peek() {
            Some('}') => {
                src.advance();
                let items_len = items.len();
                let tup = PyTuple::new(py, &items)?;
                let s = crate::collections::phashset::hash_set(py, tup)?;
                // Duplicate check: if the set's count is less than items.len(),
                // at least one duplicate was present in the literal.
                let s_count: usize = s.bind(py).call_method0("__len__")?.extract()?;
                if s_count != items_len {
                    return Err(errors::make(
                        "Duplicate key in set literal",
                        start_line,
                        start_col,
                    ));
                }
                return Ok(s.into_any());
            }
            Some(')') | Some(']') => {
                let ch = src.peek().unwrap();
                return Err(errors::make(
                    format!("Unmatched delimiter: expected '}}' but got '{}'", ch),
                    src.line(),
                    src.column(),
                ));
            }
            None => {
                return Err(errors::make(
                    "EOF while reading set",
                    start_line,
                    start_col,
                ));
            }
            _ => {
                let el = dispatch::read_one(src, py)?;
                items.push(el);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Syntax quote / unquote / unquote-splicing (Stage 3)
// ---------------------------------------------------------------------------

/// `~form` → `(clojure.core/unquote form)`.
/// `~@form` → `(clojure.core/unquote-splicing form)`.
/// Caller has NOT consumed the leading `~`.
pub fn unquote_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let tilde = src.advance();
    debug_assert_eq!(tilde, Some('~'));
    let head_name = if src.peek() == Some('@') {
        src.advance();
        "clojure.core/unquote-splicing"
    } else {
        "clojure.core/unquote"
    };
    let inner = dispatch::read_one(src, py)?;
    let head_sym = parse_qualified_symbol(py, head_name)?;
    let args = PyTuple::new(py, &[head_sym, inner])?;
    crate::collections::plist::list_(py, args)
}

/// `` `form `` — syntax-quote. Walks `form` at read time, resolving unquotes
/// and generating auto-gensyms. Produces a form that, when evaluated,
/// reconstructs the quoted structure with unquotes filled in.
/// Caller has NOT consumed the leading backtick.
pub fn syntax_quote_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let bt = src.advance();
    debug_assert_eq!(bt, Some('`'));
    let inner = dispatch::read_one(src, py)?;
    let mut gensyms: std::collections::HashMap<String, PyObject> =
        std::collections::HashMap::new();
    syntax_quote(py, inner, &mut gensyms)
}

/// Names that syntax-quote must NOT qualify — special forms the compiler
/// recognizes directly, member-access sugar (`.foo`, `.-field`), and the
/// division symbol `/`. Qualifying any of these would break the
/// compile-time dispatch that depends on the bare name.
fn is_special_form(name: &str) -> bool {
    if name == "/" {
        return true;
    }
    // `.foo` (method call sugar), `.-foo` (field access sugar) —
    // handled by the compiler's special-form path.
    if name.starts_with('.') {
        return true;
    }
    // Dotted names like `builtins.Exception`, `java.util.ArrayList`,
    // `clojure.lang.RT` — these are class / module paths resolved by the
    // compiler's dotted-fallback, not Vars that could live in *ns*.
    // Vanilla Clojure's syntax-quote applies the same rule.
    if name.contains('.') && name != ".." {
        return true;
    }
    matches!(
        name,
        "quote"
            | "if"
            | "do"
            | "let*"
            | "loop*"
            | "letfn*"
            | "recur"
            | "var"
            | "fn*"
            | "def"
            | "set!"
            | "throw"
            | "try"
            | "catch"
            | "finally"
            | "new"
            | "&"
    )
}

/// Resolve an unqualified symbol at syntax-quote time. If the symbol is
/// already referred/aliased in `*ns*`, return a fully-qualified symbol
/// pointing at the Var's owning ns. Otherwise fall back to
/// `<*ns*-name>/<name>` — vanilla does this even when the Var doesn't
/// exist yet, supporting forward references inside macros.
fn qualify_syntax_quote_symbol(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let cur_ns = current_ns_for_syntax_quote(py)?;
    let cur_ns_b = cur_ns.bind(py);

    // Fast path: is this symbol already interned in *ns* (via `refer`)?
    // If yes and the resolved value is a Var, take that Var's home ns.
    if let Ok(attr) = cur_ns_b.getattr(name) {
        if let Ok(var) = attr.cast::<crate::var::Var>() {
            let var_ns = var.getattr("ns")?;
            // ns might be a ClojureNamespace (__name__ accessible) or nil.
            if !var_ns.is_none() {
                if let Ok(var_ns_name) = var_ns.getattr("__name__") {
                    let ns_name: String = var_ns_name.extract()?;
                    let qualified = crate::symbol::Symbol::new(
                        Some(Arc::from(ns_name.as_str())),
                        Arc::from(name),
                    );
                    return Ok(Py::new(py, qualified)?.into_any());
                }
            }
        }
    }

    // Not referred — qualify to *ns*'s own name.
    let cur_ns_name: String = cur_ns_b.getattr("__name__")?.extract()?;
    let qualified = crate::symbol::Symbol::new(
        Some(Arc::from(cur_ns_name.as_str())),
        Arc::from(name),
    );
    Ok(Py::new(py, qualified)?.into_any())
}

fn current_ns_for_syntax_quote(py: Python<'_>) -> PyResult<PyObject> {
    if let Ok(v) = crate::eval::load::ns_var(py) {
        let v_any: PyObject = v.clone_ref(py).into_any();
        if let Some(bound) = crate::binding::lookup_binding(py, &v_any) {
            return Ok(bound);
        }
        return v.bind(py).call_method0("deref").map(|o| o.unbind());
    }
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    Ok(modules.get_item("clojure.core")?.unbind())
}

fn parse_qualified_symbol(py: Python<'_>, qualified: &str) -> PyResult<PyObject> {
    let (ns, name) = qualified.split_once('/').ok_or_else(|| {
        errors::make(format!("expected qualified symbol: {}", qualified), 0, 0)
    })?;
    let s = crate::symbol::Symbol::new(Some(Arc::from(ns)), Arc::from(name));
    Ok(Py::new(py, s)?.into_any())
}

fn bare_symbol(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let s = crate::symbol::Symbol::new(None, Arc::from(name));
    Ok(Py::new(py, s)?.into_any())
}

/// Is `form` an `(unquote x)` call? If so, return `x`.
fn is_unquote(py: Python<'_>, form: &PyObject) -> Option<PyObject> {
    let b = form.bind(py);
    let pl = b.cast::<crate::collections::plist::PersistentList>().ok()?;
    let head = pl.get().head.clone_ref(py);
    let sym = head.bind(py).cast::<crate::symbol::Symbol>().ok()?;
    let s = sym.get();
    if s.ns.as_deref() == Some("clojure.core") && s.name.as_ref() == "unquote" {
        let tail = pl.get().tail.clone_ref(py);
        let tail_b = tail.bind(py);
        let tpl = tail_b.cast::<crate::collections::plist::PersistentList>().ok()?;
        Some(tpl.get().head.clone_ref(py))
    } else {
        None
    }
}

/// Is `form` an `(unquote-splicing x)` call? If so, return `x`.
fn is_unquote_splicing(py: Python<'_>, form: &PyObject) -> Option<PyObject> {
    let b = form.bind(py);
    let pl = b.cast::<crate::collections::plist::PersistentList>().ok()?;
    let head = pl.get().head.clone_ref(py);
    let sym = head.bind(py).cast::<crate::symbol::Symbol>().ok()?;
    let s = sym.get();
    if s.ns.as_deref() == Some("clojure.core") && s.name.as_ref() == "unquote-splicing" {
        let tail = pl.get().tail.clone_ref(py);
        let tail_b = tail.bind(py);
        let tpl = tail_b.cast::<crate::collections::plist::PersistentList>().ok()?;
        Some(tpl.get().head.clone_ref(py))
    } else {
        None
    }
}

/// Walk a syntax-quoted form, substituting unquotes and generating
/// auto-gensyms. Returns a form that builds the quoted structure at eval time.
fn syntax_quote(
    py: Python<'_>,
    form: PyObject,
    gensyms: &mut std::collections::HashMap<String, PyObject>,
) -> PyResult<PyObject> {
    let b = form.bind(py);

    // nil / self-evaluating.
    if form.is_none(py)
        || b.cast::<pyo3::types::PyBool>().is_ok()
        || b.cast::<pyo3::types::PyInt>().is_ok()
        || b.cast::<pyo3::types::PyFloat>().is_ok()
        || b.cast::<pyo3::types::PyString>().is_ok()
        || b.cast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(form);
    }

    // Unquote: return the inner form unwrapped — surrounding code evaluates it.
    if let Some(inner) = is_unquote(py, &form) {
        return Ok(inner);
    }
    if is_unquote_splicing(py, &form).is_some() {
        return Err(errors::make("splice not in list", 0, 0));
    }

    // Symbol — resolve auto-gensym, then qualify per syntax-quote semantics.
    if let Ok(sym_ref) = b.cast::<crate::symbol::Symbol>() {
        let s = sym_ref.get();
        // Auto-gensym: `foo#` → unique `foo__N__auto__`, reused across this
        // syntax-quoted form via the `gensyms` map.
        if s.ns.is_none() && s.name.as_ref().ends_with('#') && s.name.len() > 1 {
            let base = s.name.as_ref().trim_end_matches('#').to_string();
            if let Some(existing) = gensyms.get(&base) {
                return wrap_in_quote(py, existing.clone_ref(py));
            }
            let id = gensym_id();
            let fresh_name = format!("{}__{}__auto__", base, id);
            let fresh = bare_symbol(py, &fresh_name)?;
            gensyms.insert(base, fresh.clone_ref(py));
            return wrap_in_quote(py, fresh);
        }
        // Namespaced or special-form symbol: quote bare (no further resolution).
        if s.ns.is_some() || is_special_form(s.name.as_ref()) {
            return wrap_in_quote(py, form);
        }
        // Unqualified, non-special: resolve against `*ns*`.
        //   - If the symbol already resolves to a Var (refer or alias),
        //     qualify to that Var's owning ns.
        //   - Otherwise qualify to `*ns*`'s own name — matches vanilla's
        //     "compile-time resolution even when the var doesn't yet exist"
        //     behavior, e.g. `\`foo` in ns `user` → `user/foo`.
        let qualified = qualify_syntax_quote_symbol(py, s.name.as_ref())?;
        return wrap_in_quote(py, qualified);
    }

    // Collections: list / vector / map / set.
    if let Ok(_pl) = b.cast::<crate::collections::plist::PersistentList>() {
        return syntax_quote_seq(py, &form, "list", gensyms);
    }
    if b.cast::<crate::collections::plist::EmptyList>().is_ok() {
        let head = parse_qualified_symbol(py, "clojure.core/list")?;
        let args = PyTuple::new(py, &[head])?;
        return crate::collections::plist::list_(py, args);
    }
    if b.cast::<crate::collections::pvector::PersistentVector>().is_ok() {
        return syntax_quote_seq(py, &form, "vector", gensyms);
    }
    if b.cast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
        || b.cast::<crate::collections::phashmap::PersistentHashMap>().is_ok()
    {
        return syntax_quote_map(py, &form, gensyms);
    }
    if b.cast::<crate::collections::phashset::PersistentHashSet>().is_ok() {
        return syntax_quote_seq(py, &form, "hash-set", gensyms);
    }

    // Anything else: quoted literal.
    wrap_in_quote(py, form)
}

fn wrap_in_quote(py: Python<'_>, form: PyObject) -> PyResult<PyObject> {
    let quote = bare_symbol(py, "quote")?;
    let args = PyTuple::new(py, &[quote, form])?;
    crate::collections::plist::list_(py, args)
}

/// Build `(apply <builder> (concat fragment1 fragment2 ...))`: non-spliced
/// entries become `(list x)`, spliced entries are the unwrapped inner form.
fn syntax_quote_seq(
    py: Python<'_>,
    form: &PyObject,
    builder: &str,
    gensyms: &mut std::collections::HashMap<String, PyObject>,
) -> PyResult<PyObject> {
    let items: Vec<PyObject> = {
        let b = form.bind(py);
        let mut out = Vec::new();
        for it in b.try_iter()? {
            out.push(it?.unbind());
        }
        out
    };

    let mut fragments: Vec<PyObject> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(inner) = is_unquote_splicing(py, &item) {
            fragments.push(inner);
        } else {
            let processed = syntax_quote(py, item, gensyms)?;
            let list_sym = parse_qualified_symbol(py, "clojure.core/list")?;
            let args = PyTuple::new(py, &[list_sym, processed])?;
            fragments.push(crate::collections::plist::list_(py, args)?);
        }
    }

    let concat_sym = parse_qualified_symbol(py, "clojure.core/concat")?;
    let mut concat_items: Vec<PyObject> = Vec::with_capacity(1 + fragments.len());
    concat_items.push(concat_sym);
    concat_items.extend(fragments.into_iter());
    let concat_form = crate::collections::plist::list_(py, PyTuple::new(py, &concat_items)?)?;

    let builder_sym = parse_qualified_symbol(py, &format!("clojure.core/{}", builder))?;
    let apply_sym = parse_qualified_symbol(py, "clojure.core/apply")?;
    let args = PyTuple::new(py, &[apply_sym, builder_sym, concat_form])?;
    crate::collections::plist::list_(py, args)
}

fn syntax_quote_map(
    py: Python<'_>,
    form: &PyObject,
    gensyms: &mut std::collections::HashMap<String, PyObject>,
) -> PyResult<PyObject> {
    let b = form.bind(py);
    let mut ks: Vec<PyObject> = Vec::new();
    let mut vs: Vec<PyObject> = Vec::new();
    for k in b.try_iter()? {
        let k_obj = k?.unbind();
        let v_obj = b.call_method1("val_at", (k_obj.clone_ref(py),))?.unbind();
        ks.push(k_obj);
        vs.push(v_obj);
    }
    let head = parse_qualified_symbol(py, "clojure.core/hash-map")?;
    let mut items: Vec<PyObject> = Vec::with_capacity(1 + ks.len() * 2);
    items.push(head);
    for (k, v) in ks.into_iter().zip(vs.into_iter()) {
        items.push(syntax_quote(py, k, gensyms)?);
        items.push(syntax_quote(py, v, gensyms)?);
    }
    crate::collections::plist::list_(py, PyTuple::new(py, &items)?)
}
