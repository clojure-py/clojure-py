//! `IReader` / `IPushbackReader` / `IWriter` smoke tests against
//! the concrete `StringReader` / `StringWriter` implementations.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::string::StringObj;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

// --- StringReader ----------------------------------------------------------

#[test]
fn read_char_returns_chars_then_nil() {
    init();
    let r = rt::string_reader("ab");
    let c1 = rt::read_char(r);
    let c2 = rt::read_char(r);
    let eof = rt::read_char(r);
    assert_eq!(c1.payload, 'a' as u64);
    assert_eq!(c2.payload, 'b' as u64);
    assert!(eof.is_nil());
    drop_value(r);
}

#[test]
fn peek_does_not_advance() {
    init();
    let r = rt::string_reader("xyz");
    let p1 = rt::peek_char(r);
    let p2 = rt::peek_char(r);
    let read1 = rt::read_char(r);
    assert_eq!(p1.payload, 'x' as u64);
    assert_eq!(p2.payload, 'x' as u64);
    assert_eq!(read1.payload, 'x' as u64);
    drop_value(r);
}

#[test]
fn read_char_handles_multibyte_unicode() {
    init();
    // λ is two UTF-8 bytes; Σ is two; 𝄞 is four. Reader must
    // walk char-by-char, not byte-by-byte.
    let r = rt::string_reader("λΣ𝄞");
    let c1 = rt::read_char(r);
    let c2 = rt::read_char(r);
    let c3 = rt::read_char(r);
    let eof = rt::read_char(r);
    assert_eq!(c1.payload, 'λ' as u64);
    assert_eq!(c2.payload, 'Σ' as u64);
    assert_eq!(c3.payload, '𝄞' as u64);
    assert!(eof.is_nil());
    drop_value(r);
}

#[test]
fn unread_makes_next_read_return_pushed_char() {
    init();
    let r = rt::string_reader("ab");
    let c1 = rt::read_char(r);
    let _ = rt::unread(r, c1);
    let c2 = rt::read_char(r);
    let c3 = rt::read_char(r);
    let eof = rt::read_char(r);
    assert_eq!(c2.payload, 'a' as u64, "unread restored the first char");
    assert_eq!(c3.payload, 'b' as u64);
    assert!(eof.is_nil());
    drop_value(r);
}

#[test]
fn unread_then_peek_sees_the_pushback() {
    init();
    let r = rt::string_reader("xy");
    let c = rt::read_char(r);     // consumes 'x'
    let _ = rt::unread(r, c);     // pushback 'x'
    let p = rt::peek_char(r);
    assert_eq!(p.payload, 'x' as u64);
    drop_value(r);
}

#[test]
fn second_unread_without_intervening_read_is_an_error() {
    init();
    let r = rt::string_reader("ab");
    let c = rt::read_char(r);
    let _ = rt::unread(r, c);
    let r2 = rt::unread(r, Value::char('z'));
    assert!(r2.is_exception(), "single-slot pushback can't take a second char");
    drop_value(r2);
    drop_value(r);
}

#[test]
fn empty_reader_returns_nil_immediately() {
    init();
    let r = rt::string_reader("");
    assert!(rt::read_char(r).is_nil());
    assert!(rt::peek_char(r).is_nil());
    drop_value(r);
}

// --- StringWriter ----------------------------------------------------------

#[test]
fn write_appends_strings() {
    init();
    let w = rt::string_writer();
    let _ = rt::write_str(w, rt::str_new("hello"));
    let _ = rt::write_str(w, rt::str_new(", "));
    let _ = rt::write_str(w, rt::str_new("world"));
    let s = rt::string_writer_to_string(w);
    let s_str = unsafe { StringObj::as_str_unchecked(s) };
    assert_eq!(s_str, "hello, world");
    drop_all(&[s, w]);
}

#[test]
fn empty_writer_to_string_is_empty() {
    init();
    let w = rt::string_writer();
    let s = rt::string_writer_to_string(w);
    let s_str = unsafe { StringObj::as_str_unchecked(s) };
    assert_eq!(s_str, "");
    drop_all(&[s, w]);
}

#[test]
fn flush_is_a_no_op() {
    init();
    let w = rt::string_writer();
    let _ = rt::write_str(w, rt::str_new("x"));
    let r = rt::flush(w);
    assert!(r.is_nil());
    let s = rt::string_writer_to_string(w);
    let s_str = unsafe { StringObj::as_str_unchecked(s) };
    assert_eq!(s_str, "x", "flush must not discard buffered content");
    drop_all(&[s, w]);
}

#[test]
fn write_non_string_returns_exception() {
    init();
    let w = rt::string_writer();
    let r = rt::write_str(w, Value::int(42));
    assert!(r.is_exception());
    drop_all(&[r, w]);
}
