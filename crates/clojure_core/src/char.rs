//! `Char` — a value type wrapping a single Unicode scalar, distinct from
//! Python's `str`. Reader char literals (`\a`, `\space`, `A`) construct
//! `Char` instances so JVM-equivalent type predicates work: `(string? \a)` is
//! false and `(= \a "a")` is false.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyType};

#[pyclass(module = "clojure._core", name = "Char", frozen)]
pub struct Char {
    pub value: char,
}

impl Char {
    pub fn new(c: char) -> Self { Self { value: c } }
}

#[pymethods]
impl Char {
    #[new]
    fn py_new(arg: &Bound<'_, PyAny>) -> PyResult<Self> {
        // Accept Char (passthrough), int (codepoint), or 1-char str.
        if let Ok(c) = arg.cast::<Char>() {
            return Ok(Char::new(c.get().value));
        }
        if let Ok(s) = arg.cast::<pyo3::types::PyString>() {
            let st = s.to_str()?;
            let mut iter = st.chars();
            match (iter.next(), iter.next()) {
                (Some(c), None) => return Ok(Char::new(c)),
                _ => return Err(pyo3::exceptions::PyValueError::new_err(
                    "Char(str) requires a length-1 string",
                )),
            }
        }
        if let Ok(n) = arg.extract::<i64>() {
            if !(0..=0x10FFFF).contains(&n) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    format!("Invalid Unicode codepoint: {}", n),
                ));
            }
            return char::from_u32(n as u32)
                .map(Char::new)
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
                    format!("Invalid Unicode codepoint: {}", n),
                ));
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Char() expects a Char, int (codepoint), or 1-char str",
        ))
    }

    #[getter]
    fn value(&self) -> String {
        self.value.to_string()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.cast::<Self>() {
            Ok(o) => self.value == o.get().value,
            Err(_) => false,
        }
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> bool {
        !self.__eq__(other)
    }

    fn __hash__(&self) -> i64 {
        // Match JVM Character.hashCode() — returns the codepoint as int.
        self.value as i64
    }

    fn __repr__(&self) -> String {
        named_or_escaped(self.value)
    }

    fn __str__(&self) -> String {
        self.value.to_string()
    }

    fn __lt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let o = other.cast::<Self>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("can only compare Char to Char")
        })?;
        Ok((self.value as u32) < (o.get().value as u32))
    }

    fn __le__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let o = other.cast::<Self>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("can only compare Char to Char")
        })?;
        Ok((self.value as u32) <= (o.get().value as u32))
    }

    fn __gt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let o = other.cast::<Self>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("can only compare Char to Char")
        })?;
        Ok((self.value as u32) > (o.get().value as u32))
    }

    fn __ge__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let o = other.cast::<Self>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("can only compare Char to Char")
        })?;
        Ok((self.value as u32) >= (o.get().value as u32))
    }

    fn __int__(&self) -> u32 {
        self.value as u32
    }

    fn __bool__(&self) -> bool { true }
}

/// Render a Char as its reader-compatible form: named chars (`\space`,
/// `\newline`, etc.), `\uXXXX` for non-printable / non-ASCII otherwise, or
/// `\c` for printable single-codepoint values.
pub fn named_or_escaped(c: char) -> String {
    match c {
        ' '    => "\\space".to_string(),
        '\n'   => "\\newline".to_string(),
        '\t'   => "\\tab".to_string(),
        '\r'   => "\\return".to_string(),
        '\x08' => "\\backspace".to_string(),
        '\x0c' => "\\formfeed".to_string(),
        '\0'   => "\\null".to_string(),
        c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
            format!("\\u{:04x}", c as u32)
        }
        c => format!("\\{}", c),
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Char>()?;
    Ok(())
}

/// Get a `Bound<PyType>` for `Char` — used for protocol pre-registration
/// (IEquiv / IHashEq).
pub fn char_type(py: Python<'_>) -> Bound<'_, PyType> {
    py.get_type::<Char>()
}
