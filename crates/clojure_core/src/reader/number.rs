//! Number parsing — integers, floats, ratios.

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

    if matches!(src.peek(), Some('+') | Some('-')) {
        buf.push(src.advance().unwrap());
    }
    while let Some(c) = src.peek() {
        if lexer::is_digit(c) {
            buf.push(c);
            src.advance();
        } else {
            break;
        }
    }

    // Ratio branch: `/` followed by a digit, and we have NOT yet seen
    // `.` or `e`. Vanilla forbids fractional/exponent on either side of
    // the slash, so we check this BEFORE the float branches.
    if src.peek() == Some('/') {
        if let Some(next) = src.peek_second() {
            if lexer::is_digit(next) {
                src.advance(); // consume '/'
                let mut denom_buf = String::new();
                while let Some(c) = src.peek() {
                    if lexer::is_digit(c) {
                        denom_buf.push(c);
                        src.advance();
                    } else {
                        break;
                    }
                }
                return finish_ratio(py, &buf, &denom_buf, start_line, start_col);
            }
        }
        // `/` not followed by a digit — bail with a precise reader error
        // so the surrounding "trailing content" message doesn't surface.
        return Err(errors::make(
            "Expected digit after '/' in ratio literal",
            start_line,
            start_col,
        ));
    }

    let mut is_float = false;
    if src.peek() == Some('.') {
        is_float = true;
        buf.push(src.advance().unwrap());
        while let Some(c) = src.peek() {
            if lexer::is_digit(c) {
                buf.push(c);
                src.advance();
            } else {
                break;
            }
        }
    }
    if matches!(src.peek(), Some('e') | Some('E')) {
        is_float = true;
        buf.push(src.advance().unwrap());
        if matches!(src.peek(), Some('+') | Some('-')) {
            buf.push(src.advance().unwrap());
        }
        while let Some(c) = src.peek() {
            if lexer::is_digit(c) {
                buf.push(c);
                src.advance();
            } else {
                break;
            }
        }
    }

    if is_float {
        let v: f64 = buf.parse().map_err(|_| {
            errors::make(format!("Invalid float literal: {}", buf), start_line, start_col)
        })?;
        Ok(PyFloat::new(py, v).unbind().into_any())
    } else {
        // Try i64 first; fall back to Python int via int(str, 10) on overflow.
        match buf.parse::<i64>() {
            Ok(n) => Ok(n.into_pyobject(py)?.unbind().into_any()),
            Err(_) => {
                let builtins = py.import("builtins")?;
                let int_type = builtins.getattr("int")?;
                let big = int_type.call1((buf.as_str(),))?;
                Ok(big.unbind())
            }
        }
    }
}

/// Parse `num_buf` and `denom_buf` to Python ints, then build a Fraction.
/// Returns the numerator as an int when the reduced denominator is 1
/// (vanilla "Ratio with denominator 1 collapses to BigInt").
fn finish_ratio(
    py: Python<'_>,
    num_buf: &str,
    denom_buf: &str,
    start_line: u32,
    start_col: u32,
) -> PyResult<PyObject> {
    let builtins = py.import("builtins")?;
    let int_type = builtins.getattr("int")?;
    let num = int_type.call1((num_buf,))?;
    let denom = int_type.call1((denom_buf,))?;

    // Zero-denominator → reader error (not a panic from Fraction).
    let zero = 0i64.into_pyobject(py)?;
    if denom.eq(&zero)? {
        return Err(errors::make(
            "Divide by zero in ratio literal",
            start_line,
            start_col,
        ));
    }

    let fractions = py.import("fractions")?;
    let frac = fractions.getattr("Fraction")?.call1((&num, &denom))?;
    let one = 1i64.into_pyobject(py)?;
    let denom_reduced = frac.getattr("denominator")?;
    if denom_reduced.eq(&one)? {
        return Ok(frac.getattr("numerator")?.unbind());
    }
    Ok(frac.unbind())
}
