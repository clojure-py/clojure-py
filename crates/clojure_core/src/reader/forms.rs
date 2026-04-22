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

type PyObject = Py<PyAny>;

// ---------------------------------------------------------------------------
// Reader macros (Phase R4)
// ---------------------------------------------------------------------------

/// `'form` → `(quote form)`. Caller has NOT consumed the leading `'`.
pub fn quote_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let quote_ch = src.advance();
    debug_assert_eq!(quote_ch, Some('\''));
    let form = dispatch::read_one(src, py)?;
    let quote_sym = crate::symbol::Symbol::new(None, std::sync::Arc::from("quote"));
    let quote_sym_py: PyObject = Py::new(py, quote_sym)?.into_any();
    let args = PyTuple::new(py, &[quote_sym_py, form])?;
    crate::collections::plist::list_(py, args)
}

/// `@form` → `(deref form)`. Caller has NOT consumed the leading `@`.
pub fn deref_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let at = src.advance();
    debug_assert_eq!(at, Some('@'));
    let form = dispatch::read_one(src, py)?;
    let deref_sym = crate::symbol::Symbol::new(None, std::sync::Arc::from("deref"));
    let deref_sym_py: PyObject = Py::new(py, deref_sym)?.into_any();
    let args = PyTuple::new(py, &[deref_sym_py, form])?;
    crate::collections::plist::list_(py, args)
}

/// `#'sym` → `(var sym)`. Caller has consumed `#` only; the next char is `'`.
pub fn var_quote_reader(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let quote_ch = src.advance();
    debug_assert_eq!(quote_ch, Some('\''));
    let form = dispatch::read_one(src, py)?;
    let var_sym = crate::symbol::Symbol::new(None, std::sync::Arc::from("var"));
    let var_sym_py: PyObject = Py::new(py, var_sym)?.into_any();
    let args = PyTuple::new(py, &[var_sym_py, form])?;
    crate::collections::plist::list_(py, args)
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
    if b.downcast::<crate::collections::phashmap::PersistentHashMap>().is_ok()
        || b.downcast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
    {
        return Ok(meta_raw);
    }
    // Keyword → {kw true}
    if b.downcast::<crate::keyword::Keyword>().is_ok() {
        let true_py: PyObject =
            pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any();
        let pair = PyTuple::new(py, &[meta_raw, true_py])?;
        return crate::collections::parraymap::array_map(py, pair);
    }
    // String or Symbol → {:tag <meta>}
    if b.downcast::<pyo3::types::PyString>().is_ok()
        || b.downcast::<crate::symbol::Symbol>().is_ok()
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
    let bound = target.bind(py);
    match bound.call_method1("with_meta", (meta_map,)) {
        Ok(new_target) => Ok(new_target.unbind()),
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
