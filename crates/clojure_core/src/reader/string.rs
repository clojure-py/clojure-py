//! String and character literal parsers.

use crate::reader::errors;
use crate::reader::source::Source;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyString};

type PyObject = Py<PyAny>;

/// Parse a Clojure string literal. Caller has NOT yet consumed the opening '"'.
pub fn parse_string(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let open = src.advance();
    debug_assert_eq!(open, Some('"'));

    let mut out = String::new();
    loop {
        let c = match src.advance() {
            Some(c) => c,
            None => return Err(errors::make("EOF while reading string", start_line, start_col)),
        };
        if c == '"' {
            return Ok(PyString::new(py, &out).unbind().into_any());
        }
        if c == '\\' {
            let esc = match src.advance() {
                Some(c) => c,
                None => return Err(errors::make("EOF mid-escape in string", start_line, start_col)),
            };
            match esc {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '0' => out.push('\0'),
                'u' => {
                    // Four hex digits.
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let d = src.advance().ok_or_else(|| {
                            errors::make("EOF in \\u escape", src.line(), src.column())
                        })?;
                        hex.push(d);
                    }
                    let cp = u32::from_str_radix(&hex, 16).map_err(|_| {
                        errors::make(format!("Invalid \\u escape: \\u{}", hex), src.line(), src.column())
                    })?;
                    let ch = char::from_u32(cp).ok_or_else(|| {
                        errors::make(format!("Invalid unicode codepoint: {:#x}", cp), src.line(), src.column())
                    })?;
                    out.push(ch);
                }
                other => {
                    return Err(errors::make(
                        format!("Unsupported string escape: \\{}", other),
                        src.line(),
                        src.column(),
                    ));
                }
            }
        } else {
            out.push(c);
        }
    }
}

/// Parse a Clojure character literal. Caller has NOT yet consumed the leading '\'.
pub fn parse_char(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let backslash = src.advance();
    debug_assert_eq!(backslash, Some('\\'));

    let first = match src.advance() {
        Some(c) => c,
        None => return Err(errors::make("EOF after \\", start_line, start_col)),
    };

    // Collect the full token — characters until a token-terminator (mostly for named chars).
    let mut tok = String::new();
    tok.push(first);
    while let Some(c) = src.peek() {
        if crate::reader::lexer::is_token_terminating(c) {
            break;
        }
        tok.push(c);
        src.advance();
    }

    let out_char: char = if tok.chars().count() == 1 {
        first
    } else {
        match tok.as_str() {
            "space" => ' ',
            "newline" => '\n',
            "tab" => '\t',
            "return" => '\r',
            "backspace" => '\x08',
            "formfeed" => '\x0C',
            "null" => '\0',
            _ if tok.starts_with('u') && tok.len() == 5 => {
                let hex = &tok[1..];
                let cp = u32::from_str_radix(hex, 16).map_err(|_| {
                    errors::make(format!("Invalid unicode char: \\{}", tok), start_line, start_col)
                })?;
                char::from_u32(cp).ok_or_else(|| {
                    errors::make(format!("Invalid codepoint: \\{}", tok), start_line, start_col)
                })?
            }
            _ => {
                return Err(errors::make(
                    format!("Unsupported character literal: \\{}", tok),
                    start_line,
                    start_col,
                ));
            }
        }
    };

    Ok(PyString::new(py, &out_char.to_string()).unbind().into_any())
}
