//! `IIndexingReader` tests against `StringReader` — line/col
//! advance on read, newline handling, unread restoration.

use clojure_rt::{drop_value, init, rt, Value};

#[test]
fn initial_position_is_one_one() {
    init();
    let r = rt::string_reader("");
    assert_eq!(rt::current_line(r).as_int(), Some(1));
    assert_eq!(rt::current_column(r).as_int(), Some(1));
    drop_value(r);
}

#[test]
fn column_advances_per_char() {
    init();
    let r = rt::string_reader("abc");
    let _ = rt::read_char(r); // 'a' at (1,1) → (1,2)
    assert_eq!(rt::current_column(r).as_int(), Some(2));
    let _ = rt::read_char(r); // 'b' at (1,2) → (1,3)
    assert_eq!(rt::current_column(r).as_int(), Some(3));
    let _ = rt::read_char(r); // 'c' at (1,3) → (1,4)
    assert_eq!(rt::current_column(r).as_int(), Some(4));
    assert_eq!(rt::current_line(r).as_int(), Some(1));
    drop_value(r);
}

#[test]
fn newline_bumps_line_and_resets_column() {
    init();
    let r = rt::string_reader("ab\ncd");
    let _ = rt::read_char(r); // 'a'
    let _ = rt::read_char(r); // 'b'
    let _ = rt::read_char(r); // '\n' → line bumps, col resets
    assert_eq!(rt::current_line(r).as_int(), Some(2));
    assert_eq!(rt::current_column(r).as_int(), Some(1));
    let _ = rt::read_char(r); // 'c' at (2,1) → (2,2)
    assert_eq!(rt::current_line(r).as_int(), Some(2));
    assert_eq!(rt::current_column(r).as_int(), Some(2));
    drop_value(r);
}

#[test]
fn multiple_consecutive_newlines() {
    init();
    let r = rt::string_reader("\n\n\n");
    let _ = rt::read_char(r);
    let _ = rt::read_char(r);
    let _ = rt::read_char(r);
    assert_eq!(rt::current_line(r).as_int(), Some(4));
    assert_eq!(rt::current_column(r).as_int(), Some(1));
    drop_value(r);
}

#[test]
fn unread_restores_position_after_simple_advance() {
    init();
    let r = rt::string_reader("xy");
    let c = rt::read_char(r);              // 'x' → (1,2)
    assert_eq!(rt::current_column(r).as_int(), Some(2));
    let _ = rt::unread(r, c);              // back to (1,1)
    assert_eq!(rt::current_line(r).as_int(), Some(1));
    assert_eq!(rt::current_column(r).as_int(), Some(1));
    let _ = rt::read_char(r);              // 'x' again → (1,2)
    assert_eq!(rt::current_column(r).as_int(), Some(2));
    drop_value(r);
}

#[test]
fn unread_restores_position_after_newline_advance() {
    init();
    let r = rt::string_reader("a\nb");
    let _ = rt::read_char(r); // 'a'  → (1,2)
    let nl = rt::read_char(r); // '\n' → (2,1)
    assert_eq!(rt::current_line(r).as_int(), Some(2));
    assert_eq!(rt::current_column(r).as_int(), Some(1));
    let _ = rt::unread(r, nl); // back to (1,2)
    assert_eq!(rt::current_line(r).as_int(), Some(1));
    assert_eq!(rt::current_column(r).as_int(), Some(2));
    drop_value(r);
}
