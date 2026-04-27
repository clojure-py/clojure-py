//! Integration tests for the native `StringObj` heap type.

use clojure_rt::hash::murmur3;
use clojure_rt::types::string::StringObj;
use clojure_rt::{drop_value, init, rt, Value};

#[test]
fn count_of_empty_string_is_zero() {
    init();
    let s = rt::str_new("");
    assert_eq!(rt::count(s).as_int(), Some(0));
    drop_value(s);
}

#[test]
fn count_of_ascii_string_is_byte_count() {
    init();
    let s = rt::str_new("hello");
    assert_eq!(rt::count(s).as_int(), Some(5));
    drop_value(s);
}

#[test]
fn count_of_unicode_string_is_codepoint_count() {
    init();
    // "λclojure" is 8 codepoints (1 Greek lambda + 7 ASCII).
    let s = rt::str_new("λclojure");
    assert_eq!(rt::count(s).as_int(), Some(8));
    drop_value(s);
}

#[test]
fn hash_of_empty_string_matches_murmur3() {
    init();
    let s = rt::str_new("");
    assert_eq!(
        rt::hash(s).as_int(),
        Some(murmur3::hash_unencoded_chars("") as i64),
    );
    drop_value(s);
}

#[test]
fn hash_of_ascii_string_matches_murmur3() {
    init();
    let s = rt::str_new("hello");
    assert_eq!(
        rt::hash(s).as_int(),
        Some(murmur3::hash_unencoded_chars("hello") as i64),
    );
    drop_value(s);
}

#[test]
fn hash_of_unicode_string_matches_murmur3() {
    init();
    let s = rt::str_new("λclojure");
    assert_eq!(
        rt::hash(s).as_int(),
        Some(murmur3::hash_unencoded_chars("λclojure") as i64),
    );
    drop_value(s);
}

#[test]
fn hash_is_cached_across_calls() {
    init();
    let s = rt::str_new("anything");
    let h1 = rt::hash(s);
    let h2 = rt::hash(s);
    assert_eq!(h1.as_int(), h2.as_int());
    drop_value(s);
}

#[test]
fn equiv_byte_equal_strings_is_true() {
    init();
    let a = rt::str_new("hello");
    let b = rt::str_new("hello");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(true));
    drop_value(a);
    drop_value(b);
}

#[test]
fn equiv_different_strings_is_false() {
    init();
    let a = rt::str_new("hello");
    let b = rt::str_new("world");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_value(a);
    drop_value(b);
}

#[test]
fn equiv_string_with_int_is_false() {
    init();
    let a = rt::str_new("5");
    assert_eq!(rt::equiv(a, Value::int(5)).as_bool(), Some(false));
    drop_value(a);
}

#[test]
fn as_str_round_trips_content() {
    init();
    let s = rt::str_new("λclojure");
    let view = unsafe { StringObj::as_str_unchecked(s) };
    assert_eq!(view, "λclojure");
    drop_value(s);
}
