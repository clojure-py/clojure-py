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
///
/// Supports `:name`, `:ns/name`, and auto-resolved `::name` / `::alias/name`.
/// For `::name` the current namespace (read from `*ns*`) becomes the
/// keyword's namespace; `::alias/name` looks `alias` up in `*ns*`'s alias
/// map and uses the aliased namespace's name.
pub fn parse_keyword(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let colon = src.advance();
    debug_assert_eq!(colon, Some(':'));

    // Detect `::` auto-resolve prefix.
    let auto_resolve = matches!(src.peek(), Some(':'));
    if auto_resolve {
        src.advance();
    }

    let tok = read_token_chars(src);
    if tok.is_empty() {
        let kind = if auto_resolve { "'::'" } else { "':'" };
        return Err(errors::make(
            format!("empty keyword name after {}", kind),
            start_line,
            start_col,
        ));
    }

    if !auto_resolve {
        // Plain `:name` or `:ns/name`.
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
        return Ok(kw.into_any());
    }

    // Auto-resolve path: consult `*ns*`.
    let cur_ns = current_ns_for_reader(py)?;
    let cur_ns_b = cur_ns.bind(py);

    if let Some(slash_pos) = tok.find('/') {
        if slash_pos == 0 || slash_pos == tok.len() - 1 {
            return Err(errors::make(
                format!("Invalid keyword: ::{}", tok),
                start_line,
                start_col,
            ));
        }
        let alias = &tok[..slash_pos];
        let name = &tok[slash_pos + 1..];
        // Look `alias` up in `*ns*`'s __clj_aliases__ dict. Keys are
        // Symbols; values are namespace objects.
        let aliases = cur_ns_b.getattr("__clj_aliases__")?;
        let alias_sym = symbol::Symbol::new(None, Arc::from(alias));
        let alias_sym_py: PyObject = Py::new(py, alias_sym)?.into_any();
        let target_ns_opt = aliases.get_item(alias_sym_py);
        let target_ns = match target_ns_opt {
            Ok(n) => n,
            Err(_) => {
                return Err(errors::make(
                    format!(
                        "Invalid keyword: ::{} — alias '{}' not found in current namespace",
                        tok, alias
                    ),
                    start_line,
                    start_col,
                ));
            }
        };
        let ns_name_py = target_ns.getattr("__name__")?;
        let ns_name: String = ns_name_py.extract()?;
        let kw = keyword::keyword(py, ns_name.as_str(), Some(name))?;
        return Ok(kw.into_any());
    }

    // `::name` — use current ns's name as keyword ns.
    let ns_name_py = cur_ns_b.getattr("__name__")?;
    let ns_name: String = ns_name_py.extract()?;
    let kw = keyword::keyword(py, ns_name.as_str(), Some(tok.as_str()))?;
    Ok(kw.into_any())
}

/// Look up the current namespace for reader use. Prefers the thread-bound
/// `*ns*`; falls back to the Var's root (set to clojure.core by core_shims).
fn current_ns_for_reader(py: Python<'_>) -> PyResult<PyObject> {
    if let Ok(v) = crate::eval::load::ns_var(py) {
        let v_any: PyObject = v.clone_ref(py).into_any();
        if let Some(bound) = crate::binding::lookup_binding(py, &v_any) {
            return Ok(bound);
        }
        // Fall back to the Var's root value via deref.
        return v.bind(py).call_method0("deref").map(|o| o.unbind());
    }
    // Hard fallback: clojure.core is always in sys.modules after init.
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    Ok(modules.get_item("clojure.core")?.unbind())
}
