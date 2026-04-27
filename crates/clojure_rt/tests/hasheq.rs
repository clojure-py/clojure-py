//! End-to-end tests for `IHashEq` per-primitive impls. The numeric
//! constants are pinned from `clojure_rt::hash::murmur3` (a literal
//! port of `clojure.lang.Murmur3`) and from Java's well-defined
//! `Boolean.hashCode` / `Double.hashCode` rules. JVM cross-validation
//! is a one-time spot check — see `hash::murmur3::tests` for the
//! pinned-value notes.

use clojure_rt::hash::murmur3;
use clojure_rt::{init, rt, Value};

fn h(v: Value) -> i64 {
    rt::hasheq(v).as_int().expect("IHashEq returned non-int Value")
}

#[test]
fn hasheq_nil_is_zero() {
    init();
    assert_eq!(h(Value::NIL), 0);
}

#[test]
fn hasheq_zero_is_zero() {
    init();
    // Murmur3.hash_long short-circuits 0.
    assert_eq!(h(Value::int(0)), 0);
}

#[test]
fn hasheq_int_matches_murmur3() {
    init();
    assert_eq!(h(Value::int(1)), murmur3::hash_long(1) as i64);
    assert_eq!(h(Value::int(-1)), murmur3::hash_long(-1) as i64);
    assert_eq!(h(Value::int(i64::MAX)), murmur3::hash_long(i64::MAX) as i64);
    assert_eq!(h(Value::int(i64::MIN)), murmur3::hash_long(i64::MIN) as i64);
}

#[test]
fn hasheq_bool_matches_java() {
    init();
    // Java's Boolean.hashCode constants.
    assert_eq!(h(Value::TRUE), 1231);
    assert_eq!(h(Value::FALSE), 1237);
}

#[test]
fn hasheq_float_zero_is_zero() {
    init();
    assert_eq!(h(Value::float(0.0)), 0);
}

#[test]
fn hasheq_float_negative_zero_is_zero() {
    init();
    // Numbers.hasheq override: -0.0 hashes the same as +0.0.
    assert_eq!(h(Value::float(-0.0)), 0);
}

#[test]
fn hasheq_float_one_matches_java_double_hashcode() {
    init();
    // Java: Double.hashCode(1.0) = (int)(bits ^ (bits >>> 32))
    //                            = (int)(0x3ff0000000000000L ^ 0x3ff00000)
    //                            = 1072693248
    assert_eq!(h(Value::float(1.0)), 1072693248);
}

#[test]
fn hasheq_char_is_codepoint() {
    init();
    assert_eq!(h(Value::char('a')), 'a' as i64);
    assert_eq!(h(Value::char('λ')), 'λ' as i64);
}

#[test]
fn hasheq_int_and_float_with_same_numeric_value_differ() {
    init();
    // Category-discriminated: (= 1 1.0) is false in Clojure, so their
    // hashes are allowed to differ — and they do.
    assert_ne!(h(Value::int(1)), h(Value::float(1.0)));
}
