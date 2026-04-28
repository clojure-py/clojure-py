//! Clojure reader. Mirrors `clojure.lang.LispReader` (JVM) and
//! `tools.reader` (cljs/Clojure shared library).
//!
//! # Slice 1a: skeleton
//! This file implements the core reader: whitespace + line
//! comments, atomic literals (nil / true / false / int / float /
//! string / char / keyword / symbol), and the three primary
//! collections (list, vector, map). Forms that satisfy
//! `IWithMeta` get `:line` / `:column` metadata attached.
//!
//! Deferred to follow-on slices:
//! - Reader macros: `'`, `` ` ``, `~`, `~@`, `@`, `^`, `#'`, `#_`,
//!   `#"..."`, `#{...}`, `#(...)`, `#?`, `#?@`, `#:ns{...}`,
//!   `#tag form` (including built-in `#inst` / `#uuid`).
//! - Number-suffix forms: `1234N` (BigInt), `3.14M` (BigDecimal),
//!   `1/3` (Ratio).
//! - Radix / hex / octal integer literals: `0xff`, `0777`,
//!   `2r1010`.
//! - Auto-resolve keywords: `::foo`, `::alias/foo`.
//! - Char escapes `\u####`, `\o###` (only single-char and named
//!   chars are supported here).
//!
//! # Error handling
//! Parse errors are returned as throwable Foreign exception
//! `Value`s carrying a message that includes the line/column at
//! which the error was detected. Sub-parsers propagate exceptions
//! up by returning them; callers check `is_exception` before
//! using the returned form.
//!
//! # EOF
//! `read_string` throws (returns an exception) on EOF before any
//! form is parsed; sub-parsers throw on EOF inside an unclosed
//! delimiter.

use crate::bootstrap::with_source_pos;
use crate::rt;
use crate::value::Value;

// --- Public entry points ---------------------------------------------------

/// Parse one form from a string. Returns the form, or an
/// exception `Value` on EOF / parse error.
pub fn read_string(s: &str) -> Value {
    let r = rt::string_reader(s);
    let v = match try_read(r) {
        Some(form) => form,
        None => err_at(r, "EOF while reading"),
    };
    crate::rc::drop_value(r);
    v
}

// --- Internal entry --------------------------------------------------------

/// Read one form from `reader`. Returns `None` for clean EOF
/// before any non-whitespace; returns `Some(form)` otherwise. The
/// form may be an exception `Value` if a parse error occurred
/// mid-form.
fn try_read(reader: Value) -> Option<Value> {
    skip_whitespace_and_comments(reader);
    let c = rt::peek_char(reader);
    if c.is_nil() {
        return None;
    }
    let line = rt::current_line(reader).as_int().unwrap_or(0);
    let col = rt::current_column(reader).as_int().unwrap_or(0);
    let ch = decode_char(c)?;
    let form = match ch {
        '(' => { consume(reader); read_collection(reader, ')', CollKind::List) }
        '[' => { consume(reader); read_collection(reader, ']', CollKind::Vector) }
        '{' => { consume(reader); read_collection(reader, '}', CollKind::Map) }
        ')' | ']' | '}' => err_at(
            reader,
            &format!("Unmatched delimiter: {ch}"),
        ),
        '"' => { consume(reader); read_string_literal(reader) }
        ':' => { consume(reader); read_keyword(reader) }
        '\\' => { consume(reader); read_char_literal(reader) }
        c if c.is_ascii_digit() => read_number(reader),
        c if (c == '-' || c == '+') && peek_next_is_ascii_digit(reader) => {
            read_number(reader)
        }
        // Anything else starts a symbol-shaped token (which may
        // turn out to be `nil` / `true` / `false`).
        _ => read_symbolic_token(reader),
    };
    if form.is_exception() {
        return Some(form);
    }
    let with_pos = with_source_pos(form, line, col, "");
    crate::rc::drop_value(form);
    Some(with_pos)
}

// --- Whitespace + comments -------------------------------------------------

/// Skip whitespace, commas (which Clojure treats as whitespace),
/// and `;` line comments. Returns when the reader is positioned
/// at a non-whitespace character or EOF.
fn skip_whitespace_and_comments(reader: Value) {
    loop {
        let c = rt::peek_char(reader);
        if c.is_nil() {
            return;
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => return,
        };
        if is_whitespace(ch) {
            consume(reader);
            continue;
        }
        if ch == ';' {
            // Line comment — discard through end of line (or EOF).
            consume(reader);
            loop {
                let c2 = rt::peek_char(reader);
                if c2.is_nil() {
                    return;
                }
                let ch2 = match decode_char(c2) {
                    Some(c) => c,
                    None => return,
                };
                consume(reader);
                if ch2 == '\n' {
                    break;
                }
            }
            continue;
        }
        return;
    }
}

#[inline]
fn is_whitespace(c: char) -> bool {
    // Clojure treats commas as whitespace.
    c.is_whitespace() || c == ','
}

// --- Collections -----------------------------------------------------------

#[derive(Copy, Clone)]
enum CollKind {
    List,
    Vector,
    Map,
}

fn read_collection(reader: Value, end: char, kind: CollKind) -> Value {
    let mut elements: Vec<Value> = Vec::new();
    loop {
        skip_whitespace_and_comments(reader);
        let c = rt::peek_char(reader);
        if c.is_nil() {
            for e in elements.into_iter() {
                crate::rc::drop_value(e);
            }
            return err_at(
                reader,
                &format!("EOF while reading; expected `{end}`"),
            );
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => {
                for e in elements.into_iter() {
                    crate::rc::drop_value(e);
                }
                return err_at(reader, "Invalid character in collection");
            }
        };
        if ch == end {
            consume(reader);
            break;
        }
        let form = match try_read(reader) {
            Some(f) => f,
            None => {
                for e in elements.into_iter() {
                    crate::rc::drop_value(e);
                }
                return err_at(
                    reader,
                    &format!("EOF while reading; expected `{end}`"),
                );
            }
        };
        if form.is_exception() {
            for e in elements.into_iter() {
                crate::rc::drop_value(e);
            }
            return form;
        }
        elements.push(form);
    }
    build_collection(elements, kind, reader)
}

fn build_collection(elements: Vec<Value>, kind: CollKind, reader: Value) -> Value {
    let result = match kind {
        CollKind::List => rt::list(&elements),
        CollKind::Vector => rt::vector(&elements),
        CollKind::Map => {
            if elements.len() % 2 != 0 {
                let r = err_at(reader, "Map literal must contain an even number of forms");
                for e in elements.into_iter() {
                    crate::rc::drop_value(e);
                }
                return r;
            }
            rt::array_map(&elements)
        }
    };
    for e in elements.into_iter() {
        crate::rc::drop_value(e);
    }
    result
}

// --- String literal --------------------------------------------------------

fn read_string_literal(reader: Value) -> Value {
    let mut buf = String::new();
    loop {
        let c = rt::read_char(reader);
        if c.is_nil() {
            return err_at(reader, "EOF while reading string");
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => return err_at(reader, "Invalid character in string"),
        };
        if ch == '"' {
            break;
        }
        if ch == '\\' {
            let esc = rt::read_char(reader);
            if esc.is_nil() {
                return err_at(reader, "EOF in string escape");
            }
            let esc_ch = match decode_char(esc) {
                Some(c) => c,
                None => return err_at(reader, "Invalid character in string escape"),
            };
            let resolved = match esc_ch {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'b' => '\u{08}',
                'f' => '\u{0C}',
                _ => return err_at(
                    reader,
                    &format!("Unsupported escape character: \\{esc_ch}"),
                ),
            };
            buf.push(resolved);
            continue;
        }
        buf.push(ch);
    }
    rt::str_new(&buf)
}

// --- Char literal ----------------------------------------------------------

fn read_char_literal(reader: Value) -> Value {
    let first = rt::read_char(reader);
    if first.is_nil() {
        return err_at(reader, "EOF while reading character");
    }
    let first_ch = match decode_char(first) {
        Some(c) => c,
        None => return err_at(reader, "Invalid character literal"),
    };
    // Read additional non-terminating chars to support named
    // chars like `\space`. Terminate as soon as we see a
    // collection delimiter, whitespace, comma, or EOF.
    let mut name = String::new();
    name.push(first_ch);
    loop {
        let c = rt::peek_char(reader);
        if c.is_nil() {
            break;
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => break,
        };
        if is_terminating(ch) {
            break;
        }
        consume(reader);
        name.push(ch);
    }
    if name.chars().count() == 1 {
        return Value::char(first_ch);
    }
    match name.as_str() {
        "space" => Value::char(' '),
        "tab" => Value::char('\t'),
        "newline" => Value::char('\n'),
        "return" => Value::char('\r'),
        "formfeed" => Value::char('\u{0C}'),
        "backspace" => Value::char('\u{08}'),
        _ => err_at(
            reader,
            &format!("Unsupported character: \\{name}"),
        ),
    }
}

// --- Keyword ---------------------------------------------------------------

fn read_keyword(reader: Value) -> Value {
    // We've already consumed the leading `:`. Read the name token
    // and split on `/` for namespace/name. Auto-resolve `::foo`
    // and aliased `::a/foo` are deferred to a later slice.
    let token = read_token_string(reader);
    if token.is_empty() {
        return err_at(reader, "Invalid token: :");
    }
    let (ns, name) = split_ns_name(&token);
    if name.is_empty() {
        return err_at(reader, &format!("Invalid keyword: :{token}"));
    }
    rt::keyword(ns.as_deref(), &name)
}

// --- Symbolic tokens (symbols + nil / true / false) -----------------------

fn read_symbolic_token(reader: Value) -> Value {
    let token = read_token_string(reader);
    match token.as_str() {
        "nil" => Value::NIL,
        "true" => Value::TRUE,
        "false" => Value::FALSE,
        _ => {
            let (ns, name) = split_ns_name(&token);
            if name.is_empty() {
                return err_at(reader, &format!("Invalid symbol: {token}"));
            }
            rt::symbol(ns.as_deref(), &name)
        }
    }
}

// --- Numbers ---------------------------------------------------------------

fn read_number(reader: Value) -> Value {
    let token = read_token_string(reader);
    // Float: contains '.' or e/E (and not a leading-only sign).
    if token.contains('.') || token.contains('e') || token.contains('E') {
        return match token.parse::<f64>() {
            Ok(f) => Value::float(f),
            Err(_) => err_at(reader, &format!("Invalid number: {token}")),
        };
    }
    // Plain integer.
    match token.parse::<i64>() {
        Ok(n) => Value::int(n),
        Err(_) => err_at(reader, &format!("Invalid number: {token}")),
    }
}

// --- Token primitives ------------------------------------------------------

/// Read a "token" — a run of non-terminating characters from the
/// current position. Used for symbols, keywords, numbers, and
/// boolean/nil literal tokens. Does not consume the terminating
/// character.
fn read_token_string(reader: Value) -> String {
    let mut s = String::new();
    loop {
        let c = rt::peek_char(reader);
        if c.is_nil() {
            break;
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => break,
        };
        if is_terminating(ch) {
            break;
        }
        consume(reader);
        s.push(ch);
    }
    s
}

/// `true` if `c` ends a token. Whitespace, comma, and the
/// open/close delimiters of any collection terminate. Reader-macro
/// dispatch chars (`'`, `` ` ``, `~`, `@`, `^`, `#`) terminate too —
/// they introduce new forms so a token can't include them. `;`
/// starts a comment.
#[inline]
fn is_terminating(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | ','
            | '\'' | '`' | '~' | '@' | '^' | '#' | '\\'
    ) || c.is_whitespace()
}

/// Split a token into `(namespace, name)`. If the token contains a
/// `/`, the part before is the namespace (must be non-empty) and
/// the part after is the name. The lone token `/` is the symbol
/// named "/" with no namespace.
fn split_ns_name(token: &str) -> (Option<String>, String) {
    if token == "/" {
        return (None, "/".to_string());
    }
    if let Some(idx) = token.find('/') {
        // `foo/bar` - split. `/bar` (empty ns) is invalid; let the
        // caller catch via the empty-name check.
        let ns = &token[..idx];
        let name = &token[idx + 1..];
        if ns.is_empty() {
            return (None, String::new());
        }
        return (Some(ns.to_string()), name.to_string());
    }
    (None, token.to_string())
}

// --- Char/peek helpers -----------------------------------------------------

#[inline]
fn decode_char(c: Value) -> Option<char> {
    char::from_u32(c.payload as u32)
}

/// Consume one character from `reader`, discarding the value.
/// Char Values are primitives, so no rc bookkeeping is needed.
#[inline]
fn consume(reader: Value) {
    let _ = rt::read_char(reader);
}

/// Peek the character *after* the current peek position. Reads
/// the current char, peeks the next, then unreads the current —
/// effectively a 2-char lookahead via the existing 1-char buffer.
fn peek_next_is_ascii_digit(reader: Value) -> bool {
    let first = rt::read_char(reader);
    if first.is_nil() {
        return false;
    }
    let next = rt::peek_char(reader);
    let _ = rt::unread(reader, first);
    if next.is_nil() {
        return false;
    }
    match decode_char(next) {
        Some(c) => c.is_ascii_digit(),
        None => false,
    }
}

// --- Errors ----------------------------------------------------------------

fn err_at(reader: Value, msg: &str) -> Value {
    let line = rt::current_line(reader).as_int().unwrap_or(0);
    let col = rt::current_column(reader).as_int().unwrap_or(0);
    crate::exception::make_foreign(format!("{msg} (line {line}, column {col})"))
}
