//! Number parsing — integers (i64 fast path, fallback to Python BigInt via int()),
//! floats (Python float).

use crate::reader::errors;
use crate::reader::lexer;
use crate::reader::source::Source;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyFloat};

type PyObject = Py<PyAny>;

pub fn parse_number(src: &mut Source<'_>, py: Python<'_>) -> PyResult<PyObject> {
    let start_line = src.line();
    let start_col = src.column();
    let mut buf = String::new();
    // Optional sign
    if matches!(src.peek(), Some('+') | Some('-')) {
        buf.push(src.advance().unwrap());
    }
    // Digits
    while let Some(c) = src.peek() {
        if lexer::is_digit(c) {
            buf.push(c);
            src.advance();
        } else {
            break;
        }
    }
    // Optional fractional part
    let mut is_float = false;
    if src.peek() == Some('.') {
        is_float = true;
        buf.push(src.advance().unwrap());
        while let Some(c) = src.peek() {
            if lexer::is_digit(c) { buf.push(c); src.advance(); } else { break; }
        }
    }
    // Optional exponent
    if matches!(src.peek(), Some('e') | Some('E')) {
        is_float = true;
        buf.push(src.advance().unwrap());
        if matches!(src.peek(), Some('+') | Some('-')) {
            buf.push(src.advance().unwrap());
        }
        while let Some(c) = src.peek() {
            if lexer::is_digit(c) { buf.push(c); src.advance(); } else { break; }
        }
    }

    if is_float {
        let v: f64 = buf.parse().map_err(|_| {
            errors::make(format!("Invalid float literal: {}", buf), start_line, start_col)
        })?;
        Ok(PyFloat::new(py, v).unbind().into_any())
    } else {
        // Try i64 first. If it overflows, fall through to Python int via call_method1.
        match buf.parse::<i64>() {
            Ok(n) => Ok(n.into_pyobject(py)?.unbind().into_any()),
            Err(_) => {
                // Arbitrary-precision via Python int(str, 10):
                let builtins = py.import("builtins")?;
                let int_type = builtins.getattr("int")?;
                let big = int_type.call1((buf.as_str(),))?;
                Ok(big.unbind())
            }
        }
    }
}
