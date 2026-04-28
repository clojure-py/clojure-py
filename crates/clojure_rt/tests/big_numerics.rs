//! `BigInt` / `Ratio` / `BigDecimal` storage-type tests. Arithmetic
//! ops aren't here yet — these literals just need to round-trip
//! through the heap, hash deterministically, and compare as
//! Clojure's `=` (type-strict) demands.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::big_decimal::BigDecimalObj;
use clojure_rt::types::big_int::BigIntObj;
use clojure_rt::types::ratio::RatioObj;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

// --- BigInt ---------------------------------------------------------------

#[test]
fn big_int_round_trip_via_str() {
    init();
    let v = rt::big_int_from_str("123456789012345678901234567890");
    assert!(!v.is_exception());
    let v2 = rt::big_int_from_str("123456789012345678901234567890");
    assert!(rt::equiv(v, v2).as_bool().unwrap_or(false));
    drop_all(&[v, v2]);
}

#[test]
fn big_int_inequality_distinct_values() {
    init();
    let a = BigIntObj::from_i64(1);
    let b = BigIntObj::from_i64(2);
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn big_int_not_equal_to_int_under_strict_equiv() {
    // Clojure's `=` is type-strict: (= 1 1N) → false, (== 1 1N) → true.
    init();
    let bi = BigIntObj::from_i64(1);
    assert_eq!(rt::equiv(bi, Value::int(1)).as_bool(), Some(false));
    drop_value(bi);
}

#[test]
fn big_int_hash_deterministic_and_value_consistent() {
    init();
    let a = rt::big_int_from_str("999999999999999999999999999");
    let b = rt::big_int_from_str("999999999999999999999999999");
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

#[test]
fn big_int_invalid_str_returns_exception() {
    init();
    let v = rt::big_int_from_str("not-a-number");
    assert!(v.is_exception());
    drop_value(v);
}

// --- Ratio ----------------------------------------------------------------

#[test]
fn ratio_canonical_reduces_and_normalizes_sign() {
    init();
    // 4/2 should reduce to 2/1 — equiv to a directly-built 2/1.
    let r1 = rt::ratio_from_i64s(4, 2);
    let r2 = rt::ratio_from_i64s(2, 1);
    assert!(rt::equiv(r1, r2).as_bool().unwrap_or(false));
    // -1/-2 should normalize to 1/2.
    let r3 = rt::ratio_from_i64s(-1, -2);
    let r4 = rt::ratio_from_i64s(1, 2);
    assert!(rt::equiv(r3, r4).as_bool().unwrap_or(false));
    drop_all(&[r1, r2, r3, r4]);
}

#[test]
fn ratio_distinct_values_not_equal() {
    init();
    let a = rt::ratio_from_i64s(1, 3);
    let b = rt::ratio_from_i64s(2, 3);
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn ratio_zero_denominator_is_exception() {
    init();
    let v = rt::ratio_from_i64s(1, 0);
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn ratio_is_whole_predicate() {
    init();
    let whole = rt::ratio_from_i64s(6, 2);  // = 3/1
    let frac = rt::ratio_from_i64s(1, 3);
    assert!(RatioObj::is_whole(whole));
    assert!(!RatioObj::is_whole(frac));
    drop_all(&[whole, frac]);
}

#[test]
fn ratio_not_equal_to_int_under_strict_equiv() {
    init();
    let r = rt::ratio_from_i64s(1, 1);
    // Even though 1/1 is numerically 1, `=` is type-strict.
    assert_eq!(rt::equiv(r, Value::int(1)).as_bool(), Some(false));
    drop_value(r);
}

#[test]
fn ratio_hash_consistent_for_equal_ratios() {
    init();
    let a = rt::ratio_from_i64s(2, 4);
    let b = rt::ratio_from_i64s(1, 2);
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

// --- BigDecimal -----------------------------------------------------------

#[test]
fn big_decimal_round_trip_via_str() {
    init();
    let a = rt::big_decimal_from_str("3.14");
    let b = rt::big_decimal_from_str("3.14");
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

#[test]
fn big_decimal_scale_distinguishes_equal_numbers() {
    // (= 1.0M 1.00M) is false in Clojure JVM — different scales.
    init();
    let a = rt::big_decimal_from_str("1.0");
    let b = rt::big_decimal_from_str("1.00");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn big_decimal_distinct_values_not_equal() {
    init();
    let a = rt::big_decimal_from_str("3.14");
    let b = rt::big_decimal_from_str("2.72");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn big_decimal_invalid_str_returns_exception() {
    init();
    let v = rt::big_decimal_from_str("definitely not a number");
    assert!(v.is_exception());
    drop_value(v);
}

#[test]
fn big_decimal_not_equal_to_float_under_strict_equiv() {
    init();
    let bd = rt::big_decimal_from_str("3.14");
    assert_eq!(rt::equiv(bd, Value::float(3.14)).as_bool(), Some(false));
    drop_value(bd);
}

#[test]
fn big_decimal_hash_consistent_for_equal_decimals() {
    init();
    let a = rt::big_decimal_from_str("3.14");
    let b = rt::big_decimal_from_str("3.14");
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

// --- Cross-type ----------------------------------------------------------

#[test]
fn big_int_ratio_bigdecimal_distinct_under_strict_equiv() {
    // Even when "the same" number, `=` distinguishes types.
    init();
    let bi = BigIntObj::from_i64(1);
    let r = rt::ratio_from_i64s(1, 1);
    let bd = rt::big_decimal_from_str("1");
    assert_eq!(rt::equiv(bi, r).as_bool(), Some(false));
    assert_eq!(rt::equiv(bi, bd).as_bool(), Some(false));
    assert_eq!(rt::equiv(r, bd).as_bool(), Some(false));
    drop_all(&[bi, r, bd]);
}
