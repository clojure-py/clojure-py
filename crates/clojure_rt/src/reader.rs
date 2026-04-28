//! Clojure reader. Mirrors `clojure.lang.LispReader` (JVM) and
//! `clojure.tools.reader`.
//!
//! Recursive-descent over a character stream. Dispatches on the
//! first non-whitespace character to a handler; collection
//! handlers recurse via `try_read`. Forms that satisfy
//! `IWithMeta` get `:line` / `:column` metadata attached.
//!
//! Coverage:
//! - Whitespace + `;` line comments + comma-as-whitespace.
//! - Atomic literals: nil, true, false; the full numeric tower
//!   (i64 with auto-promote to BigInt on overflow, f64,
//!   1234N → BigInt, 3.14M → BigDecimal, 1/3 → Ratio,
//!   0xFF / 0777 / 2r1010 radix forms); strings with the
//!   standard escape set including `\\u####` and `\\o###`;
//!   chars including `\\u####`, `\\o###`, and the named-char
//!   registry; symbols / keywords with `ns/name` split and
//!   `::foo` / `::alias/foo` auto-resolve.
//! - Collections: list, vector, map, set, with the even-count
//!   check on map literals.
//! - Reader macros: `'` quote, `` ` `` syntax-quote, `~` unquote,
//!   `~@` unquote-splicing, `@` deref, `^` meta, `#'` var,
//!   `#_` discard, `#"..."` regex, `#{...}` set, `#(...)`
//!   anonymous fn, `#?(...)` / `#?@(...)` reader conditional,
//!   `#:ns{...}` / `#::{...}` / `#::alias{...}` namespaced map,
//!   `#tag form` tagged literal (including built-in `#inst`
//!   and `#uuid` plus user-defined via `*data-readers*` and
//!   `*default-data-readers*`).
//!
//! # Errors
//! Parse errors surface as throwable Foreign exception `Value`s
//! whose message includes the offending line/column. Sub-parsers
//! propagate exceptions via `is_exception()` checks at every
//! collection / pair boundary so partial collections never escape.
//!
//! # EOF
//! `read_string` throws on EOF before any form starts; sub-parsers
//! throw on EOF inside an unclosed delimiter or escape.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::bootstrap::{self, with_source_pos};
use crate::rt;
use crate::types::big_decimal::BigDecimalObj;
use crate::types::big_int::BigIntObj;
use crate::types::inst::InstObj;
use crate::types::namespace::Namespace;
use crate::types::pattern::PatternObj;
use crate::types::ratio::RatioObj;
use crate::types::string::StringObj;
use crate::types::uuid::UUIDObj;
use crate::value::Value;

// === Public entry points ===================================================

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

/// Read all forms from a string, returning a vector of forms (or
/// an exception value on the first parse error).
pub fn read_all(s: &str) -> Value {
    let r = rt::string_reader(s);
    let mut forms: Vec<Value> = Vec::new();
    loop {
        match try_read(r) {
            Some(form) => {
                if form.is_exception() {
                    for f in forms { crate::rc::drop_value(f); }
                    crate::rc::drop_value(r);
                    return form;
                }
                forms.push(form);
            }
            None => break,
        }
    }
    let v = rt::vector(&forms);
    for f in forms { crate::rc::drop_value(f); }
    crate::rc::drop_value(r);
    v
}

// === Internal entry (with skip-loop for #_ and #?-no-match) ==============

/// Outcome of one read step. `Form` is a parsed value (possibly
/// an exception); `Splice` is a sequence whose elements should be
/// inserted into a surrounding collection (`#?@`); `Skip` means a
/// reader macro consumed input without producing a form (`#_` or
/// `#?` with no match) and the caller should re-read; `Eof`
/// means clean end-of-input.
enum ReadOutcome {
    Form(Value),
    Splice(Value),
    Skip,
    Eof,
}

/// Public entry — returns `None` on clean EOF, `Some(form)`
/// otherwise. Loops past `Skip` outcomes; rejects top-level
/// `Splice` as an error.
fn try_read(reader: Value) -> Option<Value> {
    loop {
        match read_one(reader) {
            ReadOutcome::Form(f) => return Some(f),
            ReadOutcome::Splice(_seq) => {
                // We don't drop _seq because the value was already
                // returned by reader-conditional, and read_one in
                // top-level context shouldn't have produced it
                // (the dispatcher knows we're at top level via the
                // collection flag). Defensive error in case of bug.
                crate::rc::drop_value(_seq);
                return Some(crate::exception::make_foreign(
                    "Reader-conditional splice not in collection".to_string(),
                ));
            }
            ReadOutcome::Skip => continue,
            ReadOutcome::Eof => return None,
        }
    }
}

/// Single read step — does NOT loop on Skip. Used by both
/// `try_read` (which loops) and `read_collection` (which handles
/// each outcome explicitly so splice can splice).
fn read_one(reader: Value) -> ReadOutcome {
    skip_whitespace_and_comments(reader);
    let c = rt::peek_char(reader);
    if c.is_nil() {
        return ReadOutcome::Eof;
    }
    let line = rt::current_line(reader).as_int().unwrap_or(0);
    let col = rt::current_column(reader).as_int().unwrap_or(0);
    let ch = match decode_char(c) {
        Some(c) => c,
        None => return ReadOutcome::Form(err_at(reader, "Invalid character")),
    };
    let outcome = match ch {
        '(' => { consume(reader); ReadOutcome::Form(read_collection(reader, ')', CollKind::List)) }
        '[' => { consume(reader); ReadOutcome::Form(read_collection(reader, ']', CollKind::Vector)) }
        '{' => { consume(reader); ReadOutcome::Form(read_collection(reader, '}', CollKind::Map)) }
        ')' | ']' | '}' => ReadOutcome::Form(err_at(reader, &format!("Unmatched delimiter: {ch}"))),
        '"' => { consume(reader); ReadOutcome::Form(read_string_literal(reader)) }
        ':' => { consume(reader); ReadOutcome::Form(read_keyword(reader)) }
        '\\' => { consume(reader); ReadOutcome::Form(read_char_literal(reader)) }
        '\'' => { consume(reader); ReadOutcome::Form(wrap_macro(reader, "quote")) }
        '@' => { consume(reader); ReadOutcome::Form(wrap_qualified_macro(reader, "clojure.core", "deref")) }
        '^' => { consume(reader); ReadOutcome::Form(read_meta(reader)) }
        '`' => { consume(reader); ReadOutcome::Form(read_syntax_quote(reader)) }
        '~' => { consume(reader); ReadOutcome::Form(read_unquote(reader)) }
        '#' => { consume(reader); read_dispatch(reader) }
        c if c.is_ascii_digit() => ReadOutcome::Form(read_number(reader)),
        c if (c == '-' || c == '+') && peek_next_is_ascii_digit(reader) => {
            ReadOutcome::Form(read_number(reader))
        }
        _ => ReadOutcome::Form(read_symbolic_token(reader)),
    };
    // Attach :line/:column to Form outcomes only (Splice/Skip
    // don't produce a positioned value).
    match outcome {
        ReadOutcome::Form(form) => {
            if form.is_exception() {
                return ReadOutcome::Form(form);
            }
            let with_pos = with_source_pos(form, line, col, "");
            crate::rc::drop_value(form);
            ReadOutcome::Form(with_pos)
        }
        other => other,
    }
}

// === Whitespace + comments =================================================

fn skip_whitespace_and_comments(reader: Value) {
    loop {
        let c = rt::peek_char(reader);
        if c.is_nil() {
            return;
        }
        let ch = match decode_char(c) { Some(c) => c, None => return };
        if is_whitespace(ch) {
            consume(reader);
            continue;
        }
        if ch == ';' {
            consume(reader);
            loop {
                let c2 = rt::peek_char(reader);
                if c2.is_nil() { return; }
                let ch2 = match decode_char(c2) { Some(c) => c, None => return };
                consume(reader);
                if ch2 == '\n' { break; }
            }
            continue;
        }
        return;
    }
}

#[inline]
fn is_whitespace(c: char) -> bool {
    c.is_whitespace() || c == ','
}

// === Collections ===========================================================

#[derive(Copy, Clone)]
enum CollKind { List, Vector, Map, Set }

fn read_collection(reader: Value, end: char, kind: CollKind) -> Value {
    let mut elements: Vec<Value> = Vec::new();
    loop {
        skip_whitespace_and_comments(reader);
        let c = rt::peek_char(reader);
        if c.is_nil() {
            return drop_and_err(elements, reader, &format!("EOF while reading; expected `{end}`"));
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => return drop_and_err(elements, reader, "Invalid character in collection"),
        };
        if ch == end {
            consume(reader);
            break;
        }
        match read_one(reader) {
            ReadOutcome::Form(f) => {
                if f.is_exception() { return drop_and_err_v(elements, f); }
                elements.push(f);
            }
            ReadOutcome::Splice(seq) => {
                let mut s = rt::seq(seq);
                crate::rc::drop_value(seq);
                while !s.is_nil() {
                    let v = rt::first(s);
                    elements.push(v);
                    let n = rt::next(s);
                    crate::rc::drop_value(s);
                    s = n;
                }
                crate::rc::drop_value(s);
            }
            ReadOutcome::Skip => continue,
            ReadOutcome::Eof => {
                return drop_and_err(elements, reader, &format!("EOF while reading; expected `{end}`"));
            }
        }
    }
    build_collection(elements, kind, reader)
}

fn build_collection(elements: Vec<Value>, kind: CollKind, reader: Value) -> Value {
    let result = match kind {
        CollKind::List => rt::list(&elements),
        CollKind::Vector => rt::vector(&elements),
        CollKind::Map => {
            if elements.len() % 2 != 0 {
                return drop_and_err(elements, reader, "Map literal must contain an even number of forms");
            }
            rt::array_map(&elements)
        }
        CollKind::Set => rt::hash_set(&elements),
    };
    for e in elements.into_iter() { crate::rc::drop_value(e); }
    result
}

#[inline]
fn drop_and_err(els: Vec<Value>, reader: Value, msg: &str) -> Value {
    for e in els.into_iter() { crate::rc::drop_value(e); }
    err_at(reader, msg)
}

#[inline]
fn drop_and_err_v(els: Vec<Value>, err: Value) -> Value {
    for e in els.into_iter() { crate::rc::drop_value(e); }
    err
}

// === String literal ========================================================

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
        if ch == '"' { break; }
        if ch == '\\' {
            match read_string_escape(reader) {
                Ok(c) => buf.push(c),
                Err(e) => return e,
            }
            continue;
        }
        buf.push(ch);
    }
    rt::str_new(&buf)
}

fn read_string_escape(reader: Value) -> Result<char, Value> {
    let c = rt::read_char(reader);
    if c.is_nil() {
        return Err(err_at(reader, "EOF in string escape"));
    }
    let ch = decode_char(c).ok_or_else(|| err_at(reader, "Invalid character in escape"))?;
    Ok(match ch {
        '"' => '"',
        '\\' => '\\',
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'b' => '\u{08}',
        'f' => '\u{0C}',
        '0'..='7' => {
            // Octal escape: read up to two more octal digits; total
            // value must fit in 8 bits (max \377 = 255).
            let mut digits = String::new();
            digits.push(ch);
            for _ in 0..2 {
                let next = rt::peek_char(reader);
                if next.is_nil() { break; }
                let n = match decode_char(next) { Some(c) => c, None => break };
                if !n.is_digit(8) { break; }
                consume(reader);
                digits.push(n);
            }
            let n = u32::from_str_radix(&digits, 8)
                .map_err(|_| err_at(reader, &format!("Invalid octal escape: \\{digits}")))?;
            if n > 0xFF {
                return Err(err_at(reader, &format!("Octal escape \\{digits} out of 8-bit range")));
            }
            char::from_u32(n).ok_or_else(|| err_at(reader, &format!("Invalid char from octal escape \\{digits}")))?
        }
        'u' => {
            // Unicode escape: exactly four hex digits.
            let mut digits = String::new();
            for _ in 0..4 {
                let n = rt::read_char(reader);
                if n.is_nil() {
                    return Err(err_at(reader, "EOF in \\u escape"));
                }
                let nc = decode_char(n).ok_or_else(|| err_at(reader, "Invalid char in \\u escape"))?;
                if !nc.is_ascii_hexdigit() {
                    return Err(err_at(reader, &format!("Invalid hex digit in \\u escape: {nc}")));
                }
                digits.push(nc);
            }
            let n = u32::from_str_radix(&digits, 16)
                .map_err(|_| err_at(reader, &format!("Invalid unicode escape \\u{digits}")))?;
            char::from_u32(n).ok_or_else(|| err_at(reader, &format!("Invalid codepoint \\u{digits}")))?
        }
        _ => return Err(err_at(reader, &format!("Unsupported escape character: \\{ch}"))),
    })
}

// === Char literal ==========================================================

fn read_char_literal(reader: Value) -> Value {
    let first = rt::read_char(reader);
    if first.is_nil() {
        return err_at(reader, "EOF while reading character");
    }
    let first_ch = match decode_char(first) {
        Some(c) => c,
        None => return err_at(reader, "Invalid character literal"),
    };
    let mut name = String::new();
    name.push(first_ch);
    loop {
        let c = rt::peek_char(reader);
        if c.is_nil() { break; }
        let ch = match decode_char(c) { Some(c) => c, None => break };
        if is_terminating(ch) { break; }
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
        _ => {
            // \u#### → unicode
            if first_ch == 'u' && name.len() == 5 && name[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                let n = u32::from_str_radix(&name[1..], 16)
                    .map_err(|_| ()).ok();
                if let Some(n) = n {
                    if let Some(c) = char::from_u32(n) {
                        return Value::char(c);
                    }
                }
                return err_at(reader, &format!("Invalid unicode char: \\{name}"));
            }
            // \o### → octal (1-3 digits, max 0o377)
            if first_ch == 'o' && name.len() >= 2 && name.len() <= 4
                && name[1..].chars().all(|c| c.is_digit(8))
            {
                let n = u32::from_str_radix(&name[1..], 8).ok();
                if let Some(n) = n {
                    if n <= 0o377 {
                        if let Some(c) = char::from_u32(n) {
                            return Value::char(c);
                        }
                    }
                    return err_at(reader, &format!("Octal char out of range: \\{name}"));
                }
            }
            err_at(reader, &format!("Unsupported character: \\{name}"))
        }
    }
}

// === Keyword ===============================================================

fn read_keyword(reader: Value) -> Value {
    // We've consumed the leading `:`. Check for `::` (auto-resolve).
    let auto = {
        let c = rt::peek_char(reader);
        !c.is_nil() && decode_char(c) == Some(':')
    };
    if auto { consume(reader); }
    let token = read_token_string(reader);
    if token.is_empty() {
        return err_at(reader, "Invalid token: :");
    }
    let (raw_ns, name) = split_ns_name(&token);
    if name.is_empty() {
        return err_at(reader, &format!("Invalid keyword: :{token}"));
    }
    if !auto {
        return rt::keyword(raw_ns.as_deref(), &name);
    }
    // ::foo or ::alias/foo — resolve via *ns*.
    let ns_var = bootstrap::current_ns_var();
    let ns = rt::deref(ns_var);
    let resolved_ns_str = match raw_ns {
        None => {
            // ::foo → :current-ns/foo
            let nsname = Namespace::name(ns);
            let nm = rt::name(nsname);
            let s = unsafe { StringObj::as_str_unchecked(nm) }.to_string();
            crate::rc::drop_value(nm);
            crate::rc::drop_value(nsname);
            s
        }
        Some(alias) => {
            // ::alias/foo → resolve `alias` via current ns aliases.
            let alias_sym = rt::symbol(None, &alias);
            let target = Namespace::lookup_alias(ns, alias_sym);
            crate::rc::drop_value(alias_sym);
            if target.is_nil() {
                crate::rc::drop_value(ns);
                return err_at(reader, &format!("Unknown alias: {alias}"));
            }
            let nsname = Namespace::name(target);
            let nm = rt::name(nsname);
            let s = unsafe { StringObj::as_str_unchecked(nm) }.to_string();
            crate::rc::drop_value(nm);
            crate::rc::drop_value(nsname);
            crate::rc::drop_value(target);
            s
        }
    };
    crate::rc::drop_value(ns);
    rt::keyword(Some(&resolved_ns_str), &name)
}

// === Symbolic tokens =======================================================

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
            // Inside an anonymous-fn body, `%`, `%1`, etc. are
            // replaced by the auto-arg gensym for that position.
            if ns.is_none() && is_anon_fn_active() && is_anon_fn_arg(&name) {
                return anon_fn_arg(&name);
            }
            // Inside a syntax-quote, a name ending with `#` becomes
            // a stable per-syntax-quote-scope auto-gensym.
            if ns.is_none() && is_syntax_quote_active() && name.ends_with('#') && name.len() > 1 {
                return syntax_quote_autogensym(&name);
            }
            rt::symbol(ns.as_deref(), &name)
        }
    }
}

// === Numbers ===============================================================

fn read_number(reader: Value) -> Value {
    let token = read_token_string(reader);
    parse_number(&token).unwrap_or_else(|| err_at(reader, &format!("Invalid number: {token}")))
}

/// Parse a numeric token. Returns `None` on failure (caller
/// produces an error with line/col). Value on success.
fn parse_number(token: &str) -> Option<Value> {
    if token.is_empty() { return None; }

    // Ratios: `[+-]?\d+/\d+` (denominator may also have a sign).
    if let Some(slash_idx) = token.find('/') {
        let n_str = &token[..slash_idx];
        let d_str = &token[slash_idx+1..];
        if n_str.is_empty() || d_str.is_empty() { return None; }
        // Numerator and denominator must be integers (no decimal).
        if n_str.contains('.') || d_str.contains('.') { return None; }
        let n_bi = parse_bigint_decimal(n_str)?;
        let d_bi = parse_bigint_decimal(d_str)?;
        return Some(RatioObj::canonical(n_bi, d_bi));
    }

    // BigDecimal — explicit M suffix.
    if let Some(stripped) = token.strip_suffix('M') {
        return Some(BigDecimalObj::from_str(stripped));
    }

    // Float — has '.' or 'e'/'E' (and no other suffix).
    if token.contains('.') || token.contains('e') || token.contains('E') {
        return token.parse::<f64>().ok().map(Value::float);
    }

    // BigInt — explicit N suffix.
    if let Some(stripped) = token.strip_suffix('N') {
        return Some(parse_int_with_radix(stripped, /*allow_bigint*/ true));
    }

    // Plain integer with auto-promote on overflow.
    Some(parse_int_with_radix(token, /*allow_bigint*/ true))
}

/// Parse `+/-` `0xHHHH` | `0OOO` | `NrXX` | `DEC`. If
/// `allow_bigint`, falls back to BigInt on i64 overflow.
fn parse_int_with_radix(token: &str, allow_bigint: bool) -> Value {
    let (sign, rest) = strip_sign(token);
    let (radix, digits) = if let Some(after) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16u32, after.to_string())
    } else if let Some(idx) = rest.find(['r', 'R']) {
        // `NrXX` — N is decimal radix, XX is digits in that radix.
        let (n_str, after) = rest.split_at(idx);
        let after = &after[1..]; // skip the 'r'/'R'
        match n_str.parse::<u32>() {
            Ok(n) if (2..=36).contains(&n) => (n, after.to_string()),
            _ => return crate::exception::make_foreign(format!(
                "Invalid radix in numeric literal: {token}"
            )),
        }
    } else if rest.len() > 1 && rest.starts_with('0') && rest.chars().all(|c| c.is_digit(8)) {
        (8u32, rest[1..].to_string())
    } else {
        (10u32, rest.to_string())
    };

    if digits.is_empty() {
        return crate::exception::make_foreign(format!("Invalid numeric digits: {token}"));
    }
    let signed = if sign.is_empty() { digits.clone() } else { format!("{sign}{digits}") };

    if radix == 10 {
        // Try i64 first; on overflow, BigInt.
        if let Ok(n) = signed.parse::<i64>() {
            return Value::int(n);
        }
        if allow_bigint {
            return BigIntObj::from_str(&signed);
        }
        return crate::exception::make_foreign(format!("Integer overflow: {token}"));
    }
    // Non-decimal radix.
    if let Ok(n) = i64::from_str_radix(&signed, radix) {
        return Value::int(n);
    }
    if allow_bigint {
        match num_bigint::BigInt::parse_bytes(signed.as_bytes(), radix) {
            Some(n) => return BigIntObj::new(n),
            None => return crate::exception::make_foreign(format!("Invalid numeric literal: {token}")),
        }
    }
    crate::exception::make_foreign(format!("Integer overflow: {token}"))
}

fn parse_bigint_decimal(s: &str) -> Option<num_bigint::BigInt> {
    s.parse::<num_bigint::BigInt>().ok()
}

#[inline]
fn strip_sign(s: &str) -> (&str, &str) {
    if let Some(r) = s.strip_prefix('+') { ("", r) }
    else if s.starts_with('-') { ("-", &s[1..]) }
    else { ("", s) }
}

// === Quote / deref / var (simple wrap macros) =============================

/// `(macro form)` for the simple two-element wrap macros.
fn wrap_macro(reader: Value, macro_name: &str) -> Value {
    let inner = match try_read(reader) {
        Some(f) => f,
        None => return err_at(reader, &format!("EOF after `{macro_name}`")),
    };
    if inner.is_exception() { return inner; }
    let sym = rt::symbol(None, macro_name);
    let lst = rt::list(&[sym, inner]);
    crate::rc::drop_value(sym);
    crate::rc::drop_value(inner);
    lst
}

fn wrap_qualified_macro(reader: Value, ns: &str, name: &str) -> Value {
    let inner = match try_read(reader) {
        Some(f) => f,
        None => return err_at(reader, &format!("EOF after `{ns}/{name}`")),
    };
    if inner.is_exception() { return inner; }
    let sym = rt::symbol(Some(ns), name);
    let lst = rt::list(&[sym, inner]);
    crate::rc::drop_value(sym);
    crate::rc::drop_value(inner);
    lst
}

// === Meta `^` ==============================================================

fn read_meta(reader: Value) -> Value {
    let meta_form = match try_read(reader) {
        Some(f) => f,
        None => return err_at(reader, "EOF while reading meta"),
    };
    if meta_form.is_exception() { return meta_form; }
    let body = match try_read(reader) {
        Some(f) => f,
        None => {
            crate::rc::drop_value(meta_form);
            return err_at(reader, "EOF while reading form for meta");
        }
    };
    if body.is_exception() {
        crate::rc::drop_value(meta_form);
        return body;
    }
    // Convert meta_form to a map.
    let meta_map = meta_form_to_map(meta_form);
    crate::rc::drop_value(meta_form);
    if meta_map.is_exception() {
        crate::rc::drop_value(body);
        return meta_map;
    }
    let result = merge_meta(body, meta_map);
    crate::rc::drop_value(body);
    crate::rc::drop_value(meta_map);
    result
}

fn meta_form_to_map(form: Value) -> Value {
    // Keyword X → {X true}
    let kw_tid = *crate::types::keyword::KEYWORDOBJ_TYPE_ID.get().unwrap_or(&0);
    let sym_tid = *crate::types::symbol::SYMBOLOBJ_TYPE_ID.get().unwrap_or(&0);
    let str_tid = *crate::types::string::STRINGOBJ_TYPE_ID.get().unwrap_or(&0);
    let pam_tid = *crate::types::array_map::PERSISTENTARRAYMAP_TYPE_ID.get().unwrap_or(&0);
    let phm_tid = *crate::types::hash_map::PERSISTENTHASHMAP_TYPE_ID.get().unwrap_or(&0);

    if form.tag == kw_tid {
        crate::rc::dup(form);
        let m = rt::array_map(&[form, Value::TRUE]);
        crate::rc::drop_value(form);
        return m;
    }
    if form.tag == sym_tid || form.tag == str_tid {
        let tag_kw = rt::keyword(None, "tag");
        crate::rc::dup(form);
        let m = rt::array_map(&[tag_kw, form]);
        crate::rc::drop_value(tag_kw);
        crate::rc::drop_value(form);
        return m;
    }
    if form.tag == pam_tid || form.tag == phm_tid {
        crate::rc::dup(form);
        return form;
    }
    crate::exception::make_foreign(
        "Metadata must be Symbol, Keyword, String, or Map".to_string(),
    )
}

fn merge_meta(form: Value, new_meta: Value) -> Value {
    let cur = rt::meta(form);
    if cur.is_nil() {
        crate::rc::drop_value(cur);
        crate::rc::dup(new_meta);
        let r = rt::with_meta(form, new_meta);
        crate::rc::drop_value(new_meta);
        return r;
    }
    let mut acc = cur;
    let mut s = rt::seq(new_meta);
    while !s.is_nil() {
        let entry = rt::first(s);
        let k = rt::key(entry);
        let v = rt::val(entry);
        let next = rt::assoc(acc, k, v);
        crate::rc::drop_value(acc);
        crate::rc::drop_value(k);
        crate::rc::drop_value(v);
        crate::rc::drop_value(entry);
        acc = next;
        let n = rt::next(s);
        crate::rc::drop_value(s);
        s = n;
    }
    crate::rc::drop_value(s);
    let r = rt::with_meta(form, acc);
    crate::rc::drop_value(acc);
    r
}

// === `#`-dispatch ==========================================================

fn read_dispatch(reader: Value) -> ReadOutcome {
    let c = rt::read_char(reader);
    if c.is_nil() {
        return ReadOutcome::Form(err_at(reader, "EOF after `#`"));
    }
    let ch = match decode_char(c) {
        Some(c) => c,
        None => return ReadOutcome::Form(err_at(reader, "Invalid char after `#`")),
    };
    match ch {
        '_' => {
            // Discard one form. Loop past nested skips so the
            // outer caller sees a real form (or EOF).
            loop {
                match read_one(reader) {
                    ReadOutcome::Form(f) => {
                        if f.is_exception() { return ReadOutcome::Form(f); }
                        crate::rc::drop_value(f);
                        return ReadOutcome::Skip;
                    }
                    ReadOutcome::Splice(seq) => {
                        crate::rc::drop_value(seq);
                        return ReadOutcome::Skip;
                    }
                    ReadOutcome::Skip => continue,
                    ReadOutcome::Eof => return ReadOutcome::Form(err_at(reader, "EOF after `#_`")),
                }
            }
        }
        '\'' => ReadOutcome::Form(wrap_macro(reader, "var")),
        '"' => ReadOutcome::Form(read_regex(reader)),
        '{' => ReadOutcome::Form(read_collection(reader, '}', CollKind::Set)),
        '(' => ReadOutcome::Form(read_anon_fn(reader)),
        '?' => {
            let next = rt::peek_char(reader);
            let is_splice = !next.is_nil() && decode_char(next) == Some('@');
            if is_splice { consume(reader); }
            match read_reader_conditional_inner(reader, is_splice) {
                Some(f) => {
                    if f.is_exception() {
                        return ReadOutcome::Form(f);
                    }
                    if is_splice {
                        ReadOutcome::Splice(f)
                    } else {
                        ReadOutcome::Form(f)
                    }
                }
                None => ReadOutcome::Skip,
            }
        }
        ':' => ReadOutcome::Form(read_namespaced_map(reader)),
        _ => {
            // Tagged literal: `#tag form`. Put the dispatch char
            // back and let `read_tagged_literal` parse the tag.
            let _ = rt::unread(reader, Value::char(ch));
            ReadOutcome::Form(read_tagged_literal(reader))
        }
    }
}

// --- Regex `#"..."` --------------------------------------------------------

fn read_regex(reader: Value) -> Value {
    // The opening `"` was already consumed by the dispatch path.
    let mut buf = String::new();
    loop {
        let c = rt::read_char(reader);
        if c.is_nil() {
            return err_at(reader, "EOF while reading regex");
        }
        let ch = match decode_char(c) {
            Some(c) => c,
            None => return err_at(reader, "Invalid char in regex"),
        };
        if ch == '"' { break; }
        if ch == '\\' {
            // Preserve the backslash and the next char verbatim —
            // the regex engine handles its own escape semantics.
            buf.push('\\');
            let nxt = rt::read_char(reader);
            if nxt.is_nil() {
                return err_at(reader, "EOF in regex escape");
            }
            let nch = match decode_char(nxt) {
                Some(c) => c,
                None => return err_at(reader, "Invalid char in regex escape"),
            };
            buf.push(nch);
            continue;
        }
        buf.push(ch);
    }
    PatternObj::from_str(&buf)
}

// --- Anonymous fn `#(...)` -------------------------------------------------

thread_local! {
    static ANON_FN_ARGS: RefCell<Option<BTreeMap<i32, Value>>>
        = const { RefCell::new(None) };
}

fn is_anon_fn_active() -> bool {
    ANON_FN_ARGS.with(|c| c.borrow().is_some())
}

fn is_anon_fn_arg(name: &str) -> bool {
    if name == "%" { return true; }
    if name == "%&" { return true; }
    if let Some(rest) = name.strip_prefix('%') {
        return rest.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty();
    }
    false
}

/// Resolve `%`, `%N`, or `%&` to the gensym registered in the
/// current anon-fn env (creating a fresh one if absent).
fn anon_fn_arg(name: &str) -> Value {
    let position: i32 = if name == "%" {
        1
    } else if name == "%&" {
        -1
    } else {
        name[1..].parse::<i32>().unwrap_or(0)
    };
    ANON_FN_ARGS.with(|cell| {
        let mut env_opt = cell.borrow_mut();
        let env = env_opt.as_mut().expect("anon-fn env not active");
        if let Some(existing) = env.get(&position) {
            crate::rc::dup(*existing);
            return *existing;
        }
        let prefix = if position == -1 { "rest".to_string() } else { format!("p{position}") };
        let g = bootstrap::gensym(&prefix);
        crate::rc::share(g);
        crate::rc::dup(g);
        env.insert(position, g);
        g
    })
}

fn read_anon_fn(reader: Value) -> Value {
    if is_anon_fn_active() {
        return err_at(reader, "Nested #() not allowed");
    }
    ANON_FN_ARGS.with(|c| { *c.borrow_mut() = Some(BTreeMap::new()); });
    // Read body as a regular list (terminator is `)`).
    let body = read_collection(reader, ')', CollKind::List);
    let env = ANON_FN_ARGS.with(|c| c.borrow_mut().take()).unwrap_or_default();

    if body.is_exception() {
        for (_, v) in env { crate::rc::drop_value(v); }
        return body;
    }

    // Build params vector. Positions in env are positive ints
    // (1-based) plus optional -1 for rest.
    let mut positions: Vec<i32> = env.keys().copied().filter(|p| *p > 0).collect();
    positions.sort();
    let max_pos = positions.last().copied().unwrap_or(0);
    let mut params: Vec<Value> = Vec::new();
    // Fill in any gaps so positions match arity (e.g., #(+ %1 %3)
    // implies a 3-arity fn; %2 needs a placeholder gensym).
    for p in 1..=max_pos {
        let v = env.get(&p).copied().unwrap_or_else(|| {
            let g = bootstrap::gensym(&format!("p{p}"));
            crate::rc::share(g);
            g
        });
        params.push(v);
    }
    let has_rest = env.contains_key(&-1);
    if has_rest {
        let amp = rt::symbol(None, "&");
        params.push(amp);
        let rest = env.get(&-1).copied().unwrap();
        crate::rc::dup(rest);
        params.push(rest);
    }

    let params_vec = rt::vector(&params);
    for v in &params { crate::rc::drop_value(*v); }
    let fn_sym = rt::symbol(None, "fn*");
    let result = rt::list(&[fn_sym, params_vec, body]);
    crate::rc::drop_value(fn_sym);
    crate::rc::drop_value(params_vec);
    crate::rc::drop_value(body);
    // Drop env values (they were dup'd into params).
    for (_, v) in env { crate::rc::drop_value(v); }
    result
}

// --- Reader conditional `#?` / `#?@` ---------------------------------------

/// Read the dispatch list after `#?` or `#?@`, walk pairs against
/// `*reader-features*`, return the matched form (or `None` if no
/// match — caller handles skip semantics). For `#?@`, the matched
/// form is the seq-to-be-spliced; the caller handles the splice.
fn read_reader_conditional_inner(reader: Value, _is_splice: bool) -> Option<Value> {
    skip_whitespace_and_comments(reader);
    let c = rt::peek_char(reader);
    if c.is_nil() {
        return Some(err_at(reader, "EOF in reader conditional"));
    }
    let ch = match decode_char(c) { Some(c) => c, None => return Some(err_at(reader, "Invalid char in reader conditional")) };
    if ch != '(' {
        return Some(err_at(reader, "Reader conditional requires a list"));
    }
    consume(reader);
    let lst = read_collection(reader, ')', CollKind::List);
    if lst.is_exception() { return Some(lst); }

    let cnt = rt::count(lst).as_int().unwrap_or(0);
    if cnt % 2 != 0 {
        crate::rc::drop_value(lst);
        return Some(err_at(reader, "Reader conditional requires an even number of forms"));
    }

    let features_var = bootstrap::reader_features_var();
    let features = rt::deref(features_var);

    let mut s = rt::seq(lst);
    let mut chosen: Option<Value> = None;
    while !s.is_nil() {
        let key = rt::first(s);
        let s_after_key = rt::next(s);
        crate::rc::drop_value(s);
        s = s_after_key;
        let val = rt::first(s);
        let s_after_val = rt::next(s);
        crate::rc::drop_value(s);
        s = s_after_val;

        // :default always matches.
        let default_kw = rt::keyword(None, "default");
        let is_default = rt::equiv(key, default_kw).as_bool().unwrap_or(false);
        crate::rc::drop_value(default_kw);
        let in_features = rt::contains_key(features, key).as_bool().unwrap_or(false);
        crate::rc::drop_value(key);

        if (is_default || in_features) && chosen.is_none() {
            chosen = Some(val);
        } else {
            crate::rc::drop_value(val);
        }
    }
    crate::rc::drop_value(s);
    crate::rc::drop_value(features);
    crate::rc::drop_value(lst);
    chosen
}

// --- Namespaced map `#:ns{...}` / `#::{...}` / `#::alias{...}` ----------

fn read_namespaced_map(reader: Value) -> Value {
    // We've consumed `#:`. Determine the namespace.
    let auto = {
        let c = rt::peek_char(reader);
        !c.is_nil() && decode_char(c) == Some(':')
    };
    if auto { consume(reader); }
    // Read the namespace token (may be empty if `#::{...}`).
    let token = read_token_string(reader);
    let ns_name: String = if !auto {
        if token.is_empty() {
            return err_at(reader, "Namespaced map requires a namespace");
        }
        if token.contains('/') {
            return err_at(reader, "Invalid namespace token in namespaced map");
        }
        token.clone()
    } else if token.is_empty() {
        // `#::{...}` — current ns
        let ns_var = bootstrap::current_ns_var();
        let ns = rt::deref(ns_var);
        let nsname = Namespace::name(ns);
        let nm = rt::name(nsname);
        let s = unsafe { StringObj::as_str_unchecked(nm) }.to_string();
        crate::rc::drop_value(nm);
        crate::rc::drop_value(nsname);
        crate::rc::drop_value(ns);
        s
    } else {
        // `#::alias{...}` — resolve alias.
        let ns_var = bootstrap::current_ns_var();
        let ns = rt::deref(ns_var);
        let alias_sym = rt::symbol(None, &token);
        let target = Namespace::lookup_alias(ns, alias_sym);
        crate::rc::drop_value(alias_sym);
        crate::rc::drop_value(ns);
        if target.is_nil() {
            return err_at(reader, &format!("Unknown alias: {token}"));
        }
        let nsname = Namespace::name(target);
        let nm = rt::name(nsname);
        let s = unsafe { StringObj::as_str_unchecked(nm) }.to_string();
        crate::rc::drop_value(nm);
        crate::rc::drop_value(nsname);
        crate::rc::drop_value(target);
        s
    };
    skip_whitespace_and_comments(reader);
    let c = rt::peek_char(reader);
    if c.is_nil() || decode_char(c) != Some('{') {
        return err_at(reader, "Namespaced map requires a `{` after the prefix");
    }
    consume(reader);
    let map = read_collection(reader, '}', CollKind::Map);
    if map.is_exception() { return map; }
    requalify_map_keys(map, &ns_name)
}

/// Walk a map's entries; for each key that's an unqualified
/// keyword/symbol, prepend `ns_name`. Keys with namespace `_` are
/// stripped (left unqualified). Other keys pass through.
fn requalify_map_keys(map: Value, ns_name: &str) -> Value {
    let kw_tid = *crate::types::keyword::KEYWORDOBJ_TYPE_ID.get().unwrap_or(&0);
    let sym_tid = *crate::types::symbol::SYMBOLOBJ_TYPE_ID.get().unwrap_or(&0);

    let mut entries: Vec<Value> = Vec::new();
    let mut s = rt::seq(map);
    while !s.is_nil() {
        let entry = rt::first(s);
        let k = rt::key(entry);
        let v = rt::val(entry);
        let new_k = if k.tag == kw_tid || k.tag == sym_tid {
            let ns_v = rt::namespace(k);
            if ns_v.is_nil() {
                let nm = rt::name(k);
                let nm_s = unsafe { StringObj::as_str_unchecked(nm) }.to_string();
                crate::rc::drop_value(nm);
                if k.tag == kw_tid {
                    rt::keyword(Some(ns_name), &nm_s)
                } else {
                    rt::symbol(Some(ns_name), &nm_s)
                }
            } else {
                let ns_s = unsafe { StringObj::as_str_unchecked(ns_v) }.to_string();
                if ns_s == "_" {
                    let nm = rt::name(k);
                    let nm_s = unsafe { StringObj::as_str_unchecked(nm) }.to_string();
                    crate::rc::drop_value(nm);
                    crate::rc::drop_value(ns_v);
                    if k.tag == kw_tid {
                        rt::keyword(None, &nm_s)
                    } else {
                        rt::symbol(None, &nm_s)
                    }
                } else {
                    crate::rc::drop_value(ns_v);
                    crate::rc::dup(k);
                    k
                }
            }
        } else {
            crate::rc::dup(k);
            k
        };
        entries.push(new_k);
        entries.push(v);
        crate::rc::drop_value(k);
        crate::rc::drop_value(entry);
        let n = rt::next(s);
        crate::rc::drop_value(s);
        s = n;
    }
    crate::rc::drop_value(s);
    crate::rc::drop_value(map);
    let new_map = rt::array_map(&entries);
    for e in entries { crate::rc::drop_value(e); }
    new_map
}

// --- Tagged literal `#tag form` -------------------------------------------

fn read_tagged_literal(reader: Value) -> Value {
    let tag = match try_read(reader) {
        Some(f) => f,
        None => return err_at(reader, "EOF while reading tag"),
    };
    if tag.is_exception() { return tag; }
    let sym_tid = *crate::types::symbol::SYMBOLOBJ_TYPE_ID.get().unwrap_or(&0);
    if tag.tag != sym_tid {
        crate::rc::drop_value(tag);
        return err_at(reader, "Reader tag must be a symbol");
    }
    let form = match try_read(reader) {
        Some(f) => f,
        None => {
            crate::rc::drop_value(tag);
            return err_at(reader, "EOF while reading tagged form");
        }
    };
    if form.is_exception() {
        crate::rc::drop_value(tag);
        return form;
    }
    apply_tagged_literal(tag, form, reader)
}

fn apply_tagged_literal(tag: Value, form: Value, reader: Value) -> Value {
    // Get the tag's name (no namespace expected for built-in tags).
    let tag_name_v = rt::name(tag);
    let tag_name = unsafe { StringObj::as_str_unchecked(tag_name_v) }.to_string();
    let tag_ns_v = rt::namespace(tag);
    let has_ns = !tag_ns_v.is_nil();
    crate::rc::drop_value(tag_name_v);
    crate::rc::drop_value(tag_ns_v);

    // Built-in #inst / #uuid (namespace-less tags only).
    if !has_ns {
        match tag_name.as_str() {
            "inst" => {
                let str_tid = *crate::types::string::STRINGOBJ_TYPE_ID.get().unwrap_or(&0);
                if form.tag != str_tid {
                    crate::rc::drop_value(tag);
                    crate::rc::drop_value(form);
                    return err_at(reader, "#inst requires a string");
                }
                let s = unsafe { StringObj::as_str_unchecked(form) }.to_string();
                let r = InstObj::from_rfc3339(&s);
                crate::rc::drop_value(tag);
                crate::rc::drop_value(form);
                return r;
            }
            "uuid" => {
                let str_tid = *crate::types::string::STRINGOBJ_TYPE_ID.get().unwrap_or(&0);
                if form.tag != str_tid {
                    crate::rc::drop_value(tag);
                    crate::rc::drop_value(form);
                    return err_at(reader, "#uuid requires a string");
                }
                let s = unsafe { StringObj::as_str_unchecked(form) }.to_string();
                let r = UUIDObj::from_str(&s);
                crate::rc::drop_value(tag);
                crate::rc::drop_value(form);
                return r;
            }
            _ => {}
        }
    }

    // User-defined readers: *data-readers* then *default-data-readers*.
    let dr_var = bootstrap::data_readers_var();
    let dr = rt::deref(dr_var);
    let from_user = rt::get(dr, tag);
    crate::rc::drop_value(dr);
    if !from_user.is_nil() {
        let result = rt::invoke(from_user, &[form]);
        crate::rc::drop_value(from_user);
        crate::rc::drop_value(tag);
        crate::rc::drop_value(form);
        return result;
    }
    crate::rc::drop_value(from_user);

    let ddr_var = bootstrap::default_data_readers_var();
    let ddr = rt::deref(ddr_var);
    let from_default = rt::get(ddr, tag);
    crate::rc::drop_value(ddr);
    if !from_default.is_nil() {
        let result = rt::invoke(from_default, &[form]);
        crate::rc::drop_value(from_default);
        crate::rc::drop_value(tag);
        crate::rc::drop_value(form);
        return result;
    }
    crate::rc::drop_value(from_default);

    let tag_str = format!("#{}", tag_display(tag));
    crate::rc::drop_value(tag);
    crate::rc::drop_value(form);
    err_at(reader, &format!("No reader function for tag {tag_str}"))
}

fn tag_display(tag: Value) -> String {
    let ns_v = rt::namespace(tag);
    let nm_v = rt::name(tag);
    let nm = unsafe { StringObj::as_str_unchecked(nm_v) }.to_string();
    let r = if ns_v.is_nil() {
        nm
    } else {
        let ns = unsafe { StringObj::as_str_unchecked(ns_v) }.to_string();
        format!("{ns}/{nm}")
    };
    crate::rc::drop_value(ns_v);
    crate::rc::drop_value(nm_v);
    r
}

// === Syntax-quote ==========================================================

thread_local! {
    /// Stack of gensym envs. Each `` ` `` push s a fresh map; on
    /// exit, the current env is popped and the previous (or
    /// `None`) restored.
    static GENSYM_ENV_STACK: RefCell<Vec<std::collections::HashMap<String, Value>>>
        = const { RefCell::new(Vec::new()) };
}

fn is_syntax_quote_active() -> bool {
    GENSYM_ENV_STACK.with(|c| !c.borrow().is_empty())
}

fn syntax_quote_autogensym(name: &str) -> Value {
    // `name` ends with '#'. Strip the suffix, look up in env;
    // gensym one if absent.
    let bare = &name[..name.len() - 1];
    GENSYM_ENV_STACK.with(|cell| {
        let mut stack = cell.borrow_mut();
        let env = stack.last_mut().expect("syntax-quote env not active");
        if let Some(existing) = env.get(bare) {
            crate::rc::dup(*existing);
            return *existing;
        }
        let g = bootstrap::gensym(bare);
        crate::rc::share(g);
        crate::rc::dup(g);
        env.insert(bare.to_string(), g);
        g
    })
}

fn read_unquote(reader: Value) -> Value {
    // `~@form` vs `~form` — peek next char.
    let next = rt::peek_char(reader);
    if next.is_nil() {
        return err_at(reader, "EOF after `~`");
    }
    let nch = match decode_char(next) {
        Some(c) => c,
        None => return err_at(reader, "Invalid char after `~`"),
    };
    if nch == '@' {
        consume(reader);
        wrap_macro(reader, "unquote-splicing")
    } else {
        wrap_macro(reader, "unquote")
    }
}

fn read_syntax_quote(reader: Value) -> Value {
    GENSYM_ENV_STACK.with(|c| c.borrow_mut().push(std::collections::HashMap::new()));
    let inner = match try_read(reader) {
        Some(f) => f,
        None => {
            drop_top_gensym_env();
            return err_at(reader, "EOF in syntax-quote");
        }
    };
    if inner.is_exception() {
        drop_top_gensym_env();
        return inner;
    }
    let expanded = syntax_quote_expand(inner);
    crate::rc::drop_value(inner);
    drop_top_gensym_env();
    expanded
}

fn drop_top_gensym_env() {
    GENSYM_ENV_STACK.with(|c| {
        if let Some(env) = c.borrow_mut().pop() {
            for (_, v) in env { crate::rc::drop_value(v); }
        }
    });
}

/// Expand a syntax-quoted form into its emission code.
fn syntax_quote_expand(form: Value) -> Value {
    let sym_tid = *crate::types::symbol::SYMBOLOBJ_TYPE_ID.get().unwrap_or(&0);
    let kw_tid = *crate::types::keyword::KEYWORDOBJ_TYPE_ID.get().unwrap_or(&0);
    let str_tid = *crate::types::string::STRINGOBJ_TYPE_ID.get().unwrap_or(&0);
    let list_tid = *crate::types::list::PERSISTENTLIST_TYPE_ID.get().unwrap_or(&0);
    let empty_list_tid = *crate::types::list::EMPTYLIST_TYPE_ID.get().unwrap_or(&0);
    let pv_tid = *crate::types::vector::PERSISTENTVECTOR_TYPE_ID.get().unwrap_or(&0);
    let pam_tid = *crate::types::array_map::PERSISTENTARRAYMAP_TYPE_ID.get().unwrap_or(&0);
    let phm_tid = *crate::types::hash_map::PERSISTENTHASHMAP_TYPE_ID.get().unwrap_or(&0);
    let phs_tid = *crate::types::hash_set::PERSISTENTHASHSET_TYPE_ID.get().unwrap_or(&0);

    // Symbols → (quote ns/name) with auto-resolve.
    if form.tag == sym_tid {
        return wrap_quote(resolve_symbol_for_syntax_quote(form));
    }

    // Unquote/unquote-splicing handling on lists.
    if (form.tag == list_tid || form.tag == empty_list_tid) && is_unquote(form) {
        // `~x` → x verbatim.
        let s = rt::seq(form);
        let _head = rt::first(s);
        let n = rt::next(s);
        let target = rt::first(n);
        crate::rc::drop_value(_head);
        crate::rc::drop_value(s);
        crate::rc::drop_value(n);
        return target;
    }
    if (form.tag == list_tid || form.tag == empty_list_tid) && is_unquote_splicing(form) {
        // Top-level `~@` is illegal — must be inside a list.
        return crate::exception::make_foreign(
            "splice not in list".to_string(),
        );
    }

    // Lists / sequences → (seq (concat ...))
    if form.tag == list_tid || form.tag == empty_list_tid {
        return sq_wrap_seq(sq_expand_seq(form));
    }
    // Vectors → (apply vector (seq (concat ...)))
    if form.tag == pv_tid {
        let inner = sq_wrap_seq(sq_expand_seq(form));
        return sq_wrap_apply("vector", inner);
    }
    // Sets → (apply hash-set (seq (concat ...)))
    if form.tag == phs_tid {
        let inner = sq_wrap_seq(sq_expand_seq(form));
        return sq_wrap_apply("hash-set", inner);
    }
    // Maps → (apply hash-map (seq (concat ...))) over flattened pairs.
    if form.tag == pam_tid || form.tag == phm_tid {
        let flat = flatten_map_pairs(form);
        let inner = sq_wrap_seq(sq_expand_pairs(flat));
        crate::rc::drop_value(flat);
        return sq_wrap_apply("hash-map", inner);
    }

    // Keywords, numbers, chars, strings, nil, bool — pass through
    // unchanged. (No need to (quote …) them.)
    if form.tag == kw_tid || form.tag == str_tid {
        crate::rc::dup(form);
        return form;
    }
    crate::rc::dup(form);
    form
}

#[inline]
fn wrap_quote(inner: Value) -> Value {
    let q = rt::symbol(None, "quote");
    let r = rt::list(&[q, inner]);
    crate::rc::drop_value(q);
    crate::rc::drop_value(inner);
    r
}

fn is_unquote(form: Value) -> bool { is_form_starting_with(form, "unquote") }
fn is_unquote_splicing(form: Value) -> bool { is_form_starting_with(form, "unquote-splicing") }

fn is_form_starting_with(form: Value, name: &str) -> bool {
    let s = rt::seq(form);
    if s.is_nil() {
        crate::rc::drop_value(s);
        return false;
    }
    let head = rt::first(s);
    let sym_tid = *crate::types::symbol::SYMBOLOBJ_TYPE_ID.get().unwrap_or(&0);
    let r = head.tag == sym_tid && {
        let nm_v = rt::name(head);
        let nm = unsafe { StringObj::as_str_unchecked(nm_v) }.to_string();
        crate::rc::drop_value(nm_v);
        let ns_v = rt::namespace(head);
        let ns_nil = ns_v.is_nil();
        crate::rc::drop_value(ns_v);
        ns_nil && nm == name
    };
    crate::rc::drop_value(head);
    crate::rc::drop_value(s);
    r
}

fn resolve_symbol_for_syntax_quote(sym: Value) -> Value {
    if is_special_form_symbol(sym) {
        crate::rc::dup(sym);
        return sym;
    }
    // Auto-gensym `foo#` is handled at read time
    // (read_symbolic_token), so by here gensym suffixes are
    // already resolved to plain symbols.
    let ns_v = rt::namespace(sym);
    if !ns_v.is_nil() {
        crate::rc::drop_value(ns_v);
        crate::rc::dup(sym);
        return sym;
    }
    crate::rc::drop_value(ns_v);

    let nm_v = rt::name(sym);
    let nm = unsafe { StringObj::as_str_unchecked(nm_v) }.to_string();
    crate::rc::drop_value(nm_v);

    // Try to resolve via current ns mappings: if the symbol is
    // mapped to a Var, use the Var's ns + name. Otherwise emit
    // current-ns/name.
    let ns_var = bootstrap::current_ns_var();
    let ns = rt::deref(ns_var);
    let mapped = Namespace::get_mapping(ns, sym);

    let result_sym = if !mapped.is_nil() {
        // Var: use its declared ns + sym.
        let var_ns = crate::types::var::Var::ns(mapped);
        let var_sym = crate::types::var::Var::sym(mapped);
        let var_ns_name_sym = Namespace::name(var_ns);
        let var_ns_name_v = rt::name(var_ns_name_sym);
        let var_ns_name = unsafe { StringObj::as_str_unchecked(var_ns_name_v) }.to_string();
        let var_sym_name_v = rt::name(var_sym);
        let var_sym_name = unsafe { StringObj::as_str_unchecked(var_sym_name_v) }.to_string();
        crate::rc::drop_value(var_ns_name_v);
        crate::rc::drop_value(var_sym_name_v);
        crate::rc::drop_value(var_ns_name_sym);
        crate::rc::drop_value(var_ns);
        crate::rc::drop_value(var_sym);
        rt::symbol(Some(&var_ns_name), &var_sym_name)
    } else {
        // Not mapped — qualify with current ns name.
        let ns_name_sym = Namespace::name(ns);
        let nsname_v = rt::name(ns_name_sym);
        let nsname = unsafe { StringObj::as_str_unchecked(nsname_v) }.to_string();
        crate::rc::drop_value(nsname_v);
        crate::rc::drop_value(ns_name_sym);
        rt::symbol(Some(&nsname), &nm)
    };
    crate::rc::drop_value(mapped);
    crate::rc::drop_value(ns);
    result_sym
}

const SPECIAL_FORMS: &[&str] = &[
    "if", "do", "let*", "fn*", "def", "var", "recur", "throw",
    "try", "catch", "finally", "monitor-enter", "monitor-exit",
    "new", "&", "set!", "quote", "loop*", ".",
];

fn is_special_form_symbol(sym: Value) -> bool {
    let ns_v = rt::namespace(sym);
    if !ns_v.is_nil() {
        crate::rc::drop_value(ns_v);
        return false;
    }
    crate::rc::drop_value(ns_v);
    let nm_v = rt::name(sym);
    let nm = unsafe { StringObj::as_str_unchecked(nm_v) }.to_string();
    crate::rc::drop_value(nm_v);
    SPECIAL_FORMS.iter().any(|s| *s == nm)
}

fn sq_wrap_seq(concat_args: Value) -> Value {
    let seq_sym = rt::symbol(Some("clojure.core"), "seq");
    let r = rt::list(&[seq_sym, concat_args]);
    crate::rc::drop_value(seq_sym);
    crate::rc::drop_value(concat_args);
    r
}

fn sq_wrap_apply(target: &str, inner: Value) -> Value {
    let apply_sym = rt::symbol(Some("clojure.core"), "apply");
    let target_sym = rt::symbol(Some("clojure.core"), target);
    let r = rt::list(&[apply_sym, target_sym, inner]);
    crate::rc::drop_value(apply_sym);
    crate::rc::drop_value(target_sym);
    crate::rc::drop_value(inner);
    r
}

/// Produce a `(clojure.core/concat E1 E2 ...)` form where each
/// `Ei` is `(list (quote x))`, `(list y)` for `~y`, or `z` for `~@z`.
fn sq_expand_seq(coll: Value) -> Value {
    let mut concat_args: Vec<Value> = Vec::new();
    let mut s = rt::seq(coll);
    while !s.is_nil() {
        let item = rt::first(s);
        let e = sq_expand_item(item);
        crate::rc::drop_value(item);
        concat_args.push(e);
        let n = rt::next(s);
        crate::rc::drop_value(s);
        s = n;
    }
    crate::rc::drop_value(s);

    let concat_sym = rt::symbol(Some("clojure.core"), "concat");
    let mut all: Vec<Value> = Vec::with_capacity(concat_args.len() + 1);
    all.push(concat_sym);
    for a in &concat_args { crate::rc::dup(*a); all.push(*a); }
    let r = rt::list(&all);
    crate::rc::drop_value(concat_sym);
    for a in concat_args { crate::rc::drop_value(a); }
    for a in &all[1..] { crate::rc::drop_value(*a); }
    r
}

/// Same expansion shape as `sq_expand_seq` but starting from a
/// pre-flattened pair vector (used for maps).
fn sq_expand_pairs(flat: Value) -> Value {
    sq_expand_seq(flat)
}

fn flatten_map_pairs(map: Value) -> Value {
    let mut out: Vec<Value> = Vec::new();
    let mut s = rt::seq(map);
    while !s.is_nil() {
        let entry = rt::first(s);
        let k = rt::key(entry);
        let v = rt::val(entry);
        out.push(k);
        out.push(v);
        crate::rc::drop_value(entry);
        let n = rt::next(s);
        crate::rc::drop_value(s);
        s = n;
    }
    crate::rc::drop_value(s);
    let r = rt::list(&out);
    for o in out { crate::rc::drop_value(o); }
    r
}

fn sq_expand_item(item: Value) -> Value {
    if is_unquote(item) {
        // `~x` → `(list x)`
        let s = rt::seq(item);
        let _head = rt::first(s);
        let n = rt::next(s);
        let target = rt::first(n);
        crate::rc::drop_value(_head);
        crate::rc::drop_value(s);
        crate::rc::drop_value(n);
        let list_sym = rt::symbol(Some("clojure.core"), "list");
        let r = rt::list(&[list_sym, target]);
        crate::rc::drop_value(list_sym);
        crate::rc::drop_value(target);
        return r;
    }
    if is_unquote_splicing(item) {
        // `~@x` → x (caller's concat handles the splice).
        let s = rt::seq(item);
        let _head = rt::first(s);
        let n = rt::next(s);
        let target = rt::first(n);
        crate::rc::drop_value(_head);
        crate::rc::drop_value(s);
        crate::rc::drop_value(n);
        return target;
    }
    // Default: `(list (syntax-quote x))`.
    let inner = syntax_quote_expand(item);
    let list_sym = rt::symbol(Some("clojure.core"), "list");
    let r = rt::list(&[list_sym, inner]);
    crate::rc::drop_value(list_sym);
    crate::rc::drop_value(inner);
    r
}

// === Token / char helpers =================================================

fn read_token_string(reader: Value) -> String {
    let mut s = String::new();
    loop {
        let c = rt::peek_char(reader);
        if c.is_nil() { break; }
        let ch = match decode_char(c) { Some(c) => c, None => break };
        if is_terminating(ch) { break; }
        consume(reader);
        s.push(ch);
    }
    s
}

#[inline]
fn is_terminating(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | ','
            | '\'' | '`' | '~' | '@' | '^' | '#' | '\\'
    ) || c.is_whitespace()
}

fn split_ns_name(token: &str) -> (Option<String>, String) {
    if token == "/" { return (None, "/".to_string()); }
    if let Some(idx) = token.find('/') {
        let ns = &token[..idx];
        let name = &token[idx + 1..];
        if ns.is_empty() { return (None, String::new()); }
        return (Some(ns.to_string()), name.to_string());
    }
    (None, token.to_string())
}

#[inline]
fn decode_char(c: Value) -> Option<char> {
    char::from_u32(c.payload as u32)
}

#[inline]
fn consume(reader: Value) {
    let _ = rt::read_char(reader);
}

fn peek_next_is_ascii_digit(reader: Value) -> bool {
    let first = rt::read_char(reader);
    if first.is_nil() { return false; }
    let next = rt::peek_char(reader);
    let _ = rt::unread(reader, first);
    if next.is_nil() { return false; }
    match decode_char(next) { Some(c) => c.is_ascii_digit(), None => false }
}

fn err_at(reader: Value, msg: &str) -> Value {
    let line = rt::current_line(reader).as_int().unwrap_or(0);
    let col = rt::current_column(reader).as_int().unwrap_or(0);
    crate::exception::make_foreign(format!("{msg} (line {line}, column {col})"))
}
