//! Token parser — nil, true, false, symbol, keyword.

use crate::keyword;
use crate::reader::errors;
use crate::reader::lexer;
use crate::reader::source::Source;
use crate::symbol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Read a token starting at the current source position. Assumes the first char
/// is a valid token start (non-digit, non-delimiter). Consumes all chars up to
/// (but not including) the next terminator.
fn read_token_chars(src: &mut Source<'_>) -> String {
    let mut tok = String::new();
    while let Some(c) = src.peek() {
        if lexer::is_token_terminating(c) {
            break;
        }
        tok.push(c);
        src.advance();
    }
    tok
}

/// Parse nil/true/false/symbol starting at the current position.
pub fn parse_symbol_or_literal(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let tok = read_token_chars(src);
    if tok.is_empty() {
        return Err(errors::make("empty token", start_line, start_col));
    }
    match tok.as_str() {
        "nil" => Ok(py.None()),
        "true" => Ok(PyBool::new(py, true).to_owned().unbind().into_any()),
        "false" => Ok(PyBool::new(py, false).to_owned().unbind().into_any()),
        _ => {
            // Split on '/' for namespaced symbols. But '/' alone is the division symbol.
            if tok == "/" {
                let sym = symbol::Symbol::new(None, Arc::from("/"));
                return Ok(Py::new(py, sym)?.into_any());
            }
            if let Some(slash_pos) = tok.find('/') {
                if slash_pos > 0 && slash_pos < tok.len() - 1 {
                    let ns = &tok[..slash_pos];
                    let name = &tok[slash_pos + 1..];
                    let sym = symbol::Symbol::new(Some(Arc::from(ns)), Arc::from(name));
                    return Ok(Py::new(py, sym)?.into_any());
                }
                // Trailing or leading '/' — invalid.
                return Err(errors::make(
                    format!("Invalid token: {}", tok),
                    start_line,
                    start_col,
                ));
            }
            let sym = symbol::Symbol::new(None, Arc::from(tok.as_str()));
            Ok(Py::new(py, sym)?.into_any())
        }
    }
}

/// Parse a keyword — source currently points at the ':'.
pub fn parse_keyword(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let colon = src.advance();
    debug_assert_eq!(colon, Some(':'));
    let tok = read_token_chars(src);
    if tok.is_empty() {
        return Err(errors::make("empty keyword name after ':'", start_line, start_col));
    }
    // Split on '/'.
    if let Some(slash_pos) = tok.find('/') {
        if slash_pos == 0 || slash_pos == tok.len() - 1 {
            return Err(errors::make(
                format!("Invalid keyword: :{}", tok),
                start_line,
                start_col,
            ));
        }
        let ns = &tok[..slash_pos];
        let name = &tok[slash_pos + 1..];
        let kw = keyword::keyword(py, ns, Some(name))?;
        return Ok(kw.into_any());
    }
    let kw = keyword::keyword(py, tok.as_str(), None)?;
    Ok(kw.into_any())
}
