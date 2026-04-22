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
