//! Reader dispatch — top-level `read_one` function chooses sub-reader based
//! on the next character.

use crate::reader::errors;
use crate::reader::lexer;
use crate::reader::number;
use crate::reader::source::Source;
use crate::reader::string;
use crate::reader::token;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Skip whitespace (including commas) and line comments. Loops for multi-line.
pub fn skip_ws_and_comments(src: &mut Source<'_>) {
    loop {
        let Some(c) = src.peek() else { return };
        if lexer::is_whitespace(c) {
            src.advance();
            continue;
        }
        if c == ';' {
            // Line comment — consume to end of line.
            while let Some(c2) = src.advance() {
                if c2 == '\n' {
                    break;
                }
            }
            continue;
        }
        return;
    }
}

/// Detect if the current position looks like a number: a digit, or a sign
/// followed immediately (no whitespace) by a digit.
pub fn looks_like_number(src: &Source<'_>) -> bool {
    match src.peek() {
        Some(c) if c.is_ascii_digit() => true,
        Some('+') | Some('-') => src
            .peek_second()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false),
        _ => false,
    }
}

/// Read exactly one form starting at the current position. Returns error on
/// EOF or unexpected delimiter.
pub fn read_one(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    skip_ws_and_comments(src);
    let line = src.line();
    let col = src.column();
    let ch = match src.peek() {
        Some(c) => c,
        None => return Err(errors::make("EOF while reading", line, col)),
    };
    match ch {
        '"' => string::parse_string(src, py),
        '\\' => string::parse_char(src, py),
        ':' => token::parse_keyword(src, py),
        '(' | '[' | '{' | '#' => Err(errors::make(
            format!("Collection reader not yet implemented for '{}'", ch),
            line,
            col,
        )),
        ')' | ']' | '}' => Err(errors::make(
            format!("Unmatched delimiter: {}", ch),
            line,
            col,
        )),
        '\'' | '@' | '^' | '`' | '~' => Err(errors::make(
            format!("Reader macro '{}' not yet implemented", ch),
            line,
            col,
        )),
        _ if looks_like_number(src) => number::parse_number(src, py),
        _ => token::parse_symbol_or_literal(src, py),
    }
}
