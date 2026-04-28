//! Reader slice 1a — atomic literals + collections + position
//! metadata. Reader macros, syntax-quote, etc. land in follow-on
//! slices.

use clojure_rt::{drop_value, init, reader, rt, Value};
use clojure_rt::types::string::StringObj;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

// --- Atomic literals -------------------------------------------------------

#[test]
fn read_nil() {
    init();
    let v = reader::read_string("nil");
    assert!(v.is_nil());
}

#[test]
fn read_true_false() {
    init();
    let t = reader::read_string("true");
    let f = reader::read_string("false");
    assert_eq!(t.as_bool(), Some(true));
    assert_eq!(f.as_bool(), Some(false));
}

#[test]
fn read_positive_integer() {
    init();
    let v = reader::read_string("42");
    assert_eq!(v.as_int(), Some(42));
}

#[test]
fn read_negative_integer() {
    init();
    let v = reader::read_string("-7");
    assert_eq!(v.as_int(), Some(-7));
}

#[test]
fn read_explicit_positive_integer() {
    init();
    let v = reader::read_string("+9");
    assert_eq!(v.as_int(), Some(9));
}

#[test]
fn read_zero() {
    init();
    let v = reader::read_string("0");
    assert_eq!(v.as_int(), Some(0));
}

#[test]
fn read_float_with_dot() {
    init();
    let v = reader::read_string("3.14");
    assert!((v.as_float().unwrap() - 3.14).abs() < 1e-12);
}

#[test]
fn read_float_with_exponent() {
    init();
    let v = reader::read_string("1.5e3");
    assert!((v.as_float().unwrap() - 1500.0).abs() < 1e-9);
}

#[test]
fn read_negative_float() {
    init();
    let v = reader::read_string("-0.5");
    assert!((v.as_float().unwrap() - (-0.5)).abs() < 1e-12);
}

#[test]
fn read_simple_symbol() {
    init();
    let v = reader::read_string("foo");
    let expected = rt::symbol(None, "foo");
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_namespaced_symbol() {
    init();
    let v = reader::read_string("clojure.core/seq");
    let expected = rt::symbol(Some("clojure.core"), "seq");
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_division_symbol() {
    // `/` alone is the symbol `/` with no namespace.
    init();
    let v = reader::read_string("/");
    let expected = rt::symbol(None, "/");
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_simple_keyword() {
    init();
    let v = reader::read_string(":foo");
    let expected = rt::keyword(None, "foo");
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_namespaced_keyword() {
    init();
    let v = reader::read_string(":a/b");
    let expected = rt::keyword(Some("a"), "b");
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_string_literal_basic() {
    init();
    let v = reader::read_string(r#""hello""#);
    let s = unsafe { StringObj::as_str_unchecked(v) };
    assert_eq!(s, "hello");
    drop_value(v);
}

#[test]
fn read_string_with_escapes() {
    init();
    let v = reader::read_string(r#""a\nb\tc\\d\"e""#);
    let s = unsafe { StringObj::as_str_unchecked(v) };
    assert_eq!(s, "a\nb\tc\\d\"e");
    drop_value(v);
}

#[test]
fn read_string_eof_in_unclosed_string_is_error() {
    init();
    let v = reader::read_string(r#""unclosed"#);
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn read_simple_char() {
    init();
    let v = reader::read_string(r"\a");
    assert_eq!(v.payload as u32, 'a' as u32);
}

#[test]
fn read_named_chars() {
    init();
    let space = reader::read_string(r"\space");
    let tab = reader::read_string(r"\tab");
    let nl = reader::read_string(r"\newline");
    let ret = reader::read_string(r"\return");
    let ff = reader::read_string(r"\formfeed");
    let bs = reader::read_string(r"\backspace");
    assert_eq!(space.payload as u32, ' ' as u32);
    assert_eq!(tab.payload as u32, '\t' as u32);
    assert_eq!(nl.payload as u32, '\n' as u32);
    assert_eq!(ret.payload as u32, '\r' as u32);
    assert_eq!(ff.payload as u32, '\u{0C}' as u32);
    assert_eq!(bs.payload as u32, '\u{08}' as u32);
}

#[test]
fn read_unicode_char() {
    init();
    let v = reader::read_string(r"\λ");
    assert_eq!(v.payload as u32, 'λ' as u32);
}

#[test]
fn read_unknown_named_char_is_error() {
    init();
    let v = reader::read_string(r"\notachar");
    assert!(v.is_exception());
    drop_value(v);
}

// --- Whitespace + comments -------------------------------------------------

#[test]
fn whitespace_around_form_is_skipped() {
    init();
    let v = reader::read_string("   \n\t 42  \n");
    assert_eq!(v.as_int(), Some(42));
}

#[test]
fn line_comment_is_skipped() {
    init();
    let v = reader::read_string("; this is a comment\n42");
    assert_eq!(v.as_int(), Some(42));
}

#[test]
fn comma_is_whitespace() {
    init();
    let v = reader::read_string(",,, 42 ,,,");
    assert_eq!(v.as_int(), Some(42));
}

#[test]
fn empty_input_is_eof_error() {
    init();
    let v = reader::read_string("");
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn whitespace_only_input_is_eof_error() {
    init();
    let v = reader::read_string("   \n  ");
    assert!(v.is_exception());
    drop_value(v);
}

// --- Collections -----------------------------------------------------------

#[test]
fn read_empty_list() {
    init();
    let v = reader::read_string("()");
    assert_eq!(rt::count(v).as_int(), Some(0));
    drop_value(v);
}

#[test]
fn read_simple_list() {
    init();
    let v = reader::read_string("(1 2 3)");
    assert_eq!(rt::count(v).as_int(), Some(3));
    let expected = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_nested_list() {
    init();
    let v = reader::read_string("(1 (2 3) 4)");
    assert_eq!(rt::count(v).as_int(), Some(3));
    drop_value(v);
}

#[test]
fn read_empty_vector() {
    init();
    let v = reader::read_string("[]");
    assert_eq!(rt::count(v).as_int(), Some(0));
    drop_value(v);
}

#[test]
fn read_simple_vector() {
    init();
    let v = reader::read_string("[1 2 3]");
    assert_eq!(rt::count(v).as_int(), Some(3));
    let expected = rt::vector(&[Value::int(1), Value::int(2), Value::int(3)]);
    assert!(rt::equiv(v, expected).as_bool().unwrap_or(false));
    drop_all(&[v, expected]);
}

#[test]
fn read_empty_map() {
    init();
    let v = reader::read_string("{}");
    assert_eq!(rt::count(v).as_int(), Some(0));
    drop_value(v);
}

#[test]
fn read_simple_map() {
    init();
    let v = reader::read_string(r#"{:a 1 :b 2}"#);
    assert_eq!(rt::count(v).as_int(), Some(2));
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    assert_eq!(rt::get(v, ka).as_int(), Some(1));
    assert_eq!(rt::get(v, kb).as_int(), Some(2));
    drop_all(&[v, ka, kb]);
}

#[test]
fn read_map_with_odd_entry_count_is_error() {
    init();
    let v = reader::read_string("{:a 1 :b}");
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn read_unmatched_opener_is_error() {
    init();
    let v = reader::read_string("(1 2 3");
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn read_unmatched_closer_at_top_level_is_error() {
    init();
    let v = reader::read_string(")");
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn read_mixed_collection() {
    init();
    let v = reader::read_string(r#"[:a "hi" 1 (2 3) {:k :v}]"#);
    assert_eq!(rt::count(v).as_int(), Some(5));
    drop_value(v);
}

#[test]
fn commas_separating_collection_elements() {
    init();
    let v = reader::read_string("[1, 2, 3]");
    assert_eq!(rt::count(v).as_int(), Some(3));
    drop_value(v);
}

// --- Source position metadata ---------------------------------------------

#[test]
fn list_carries_source_position_meta() {
    init();
    let v = reader::read_string("(1 2 3)");
    let m = rt::meta(v);
    let line_kw = rt::keyword(None, "line");
    let col_kw = rt::keyword(None, "column");
    assert_eq!(rt::get(m, line_kw).as_int(), Some(1));
    assert_eq!(rt::get(m, col_kw).as_int(), Some(1));
    drop_all(&[m, line_kw, col_kw, v]);
}

#[test]
fn nested_form_position_is_local_to_form() {
    init();
    // Outer list at (1,1); inner vector at (1,4).
    let v = reader::read_string("[1 [2 3] 4]");
    // Get the inner vector via nth.
    let inner = rt::nth(v, Value::int(1));
    let m = rt::meta(inner);
    let line_kw = rt::keyword(None, "line");
    let col_kw = rt::keyword(None, "column");
    assert_eq!(rt::get(m, line_kw).as_int(), Some(1));
    assert_eq!(rt::get(m, col_kw).as_int(), Some(4));
    drop_all(&[m, line_kw, col_kw, inner, v]);
}

#[test]
fn position_after_newline_advances_line() {
    init();
    let v = reader::read_string("\n\n  (1 2)");
    let m = rt::meta(v);
    let line_kw = rt::keyword(None, "line");
    let col_kw = rt::keyword(None, "column");
    assert_eq!(rt::get(m, line_kw).as_int(), Some(3));
    assert_eq!(rt::get(m, col_kw).as_int(), Some(3));
    drop_all(&[m, line_kw, col_kw, v]);
}

// --- Round-trip via collections ------------------------------------------

#[test]
fn round_trip_through_a_map_keyed_by_keyword() {
    init();
    let v = reader::read_string(r#"{:name "Alice" :age 30}"#);
    let name_kw = rt::keyword(None, "name");
    let age_kw = rt::keyword(None, "age");
    let name = rt::get(v, name_kw);
    let age = rt::get(v, age_kw);
    let s = unsafe { StringObj::as_str_unchecked(name) };
    assert_eq!(s, "Alice");
    assert_eq!(age.as_int(), Some(30));
    drop_all(&[name, age, name_kw, age_kw, v]);
}
