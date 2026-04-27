//! End-to-end tests for `IEquiv` (Clojure's `=`) per-primitive impls,
//! plus the contract test `equiv ⟹ hash-equal`.

use clojure_rt::{init, rt, Value};

fn eq(a: Value, b: Value) -> bool {
    rt::equiv(a, b).as_bool().expect("IEquiv returned non-bool Value")
}

#[test]
fn equiv_nil_nil_true() {
    init();
    assert!(eq(Value::NIL, Value::NIL));
}

#[test]
fn equiv_nil_int_false() {
    init();
    assert!(!eq(Value::NIL, Value::int(0)));
    assert!(!eq(Value::int(0), Value::NIL));
}

#[test]
fn equiv_int_int_by_value() {
    init();
    assert!(eq(Value::int(42), Value::int(42)));
    assert!(!eq(Value::int(42), Value::int(43)));
}

#[test]
fn equiv_int_and_float_same_numeric_value_is_false() {
    init();
    // Category-discriminated equiv. `(= 1 1.0)` is false in Clojure JVM
    // because Numbers.equal requires same category before comparison.
    assert!(!eq(Value::int(1), Value::float(1.0)));
    assert!(!eq(Value::float(1.0), Value::int(1)));
}

#[test]
fn equiv_bool_int_false() {
    init();
    // (= true 1) is false in Clojure; Boolean ≠ Long.
    assert!(!eq(Value::TRUE, Value::int(1)));
    assert!(!eq(Value::int(1), Value::TRUE));
}

#[test]
fn equiv_bool_bool() {
    init();
    assert!(eq(Value::TRUE, Value::TRUE));
    assert!(eq(Value::FALSE, Value::FALSE));
    assert!(!eq(Value::TRUE, Value::FALSE));
}

#[test]
fn equiv_float_pos_zero_neg_zero_is_true() {
    init();
    // `(= 0.0 -0.0)` is true in Clojure (Numbers.equal routes through
    // `==` on doubles, which says +0.0 == -0.0).
    assert!(eq(Value::float(0.0), Value::float(-0.0)));
}

#[test]
fn equiv_float_nan_nan_is_false() {
    init();
    // NaN ≠ NaN under Clojure's `=` (Numbers.equal uses `==` on
    // doubles, which says NaN != NaN). Note this *differs* from Java's
    // `Double.equals`, which uses bit equality and says NaN equals NaN.
    let nan = Value::float(f64::NAN);
    assert!(!eq(nan, nan));
}

#[test]
fn equiv_char_char() {
    init();
    assert!(eq(Value::char('a'), Value::char('a')));
    assert!(!eq(Value::char('a'), Value::char('b')));
    assert!(!eq(Value::char('a'), Value::int('a' as i64)));
}

#[test]
fn equiv_implies_hash_equal_contract() {
    init();
    // For every pair (a, b) where equiv is true, hasheq must agree.
    // We pick a representative set spanning every primitive type +
    // a couple of edge cases (zero, negative, sign-zero).
    let xs: &[Value] = &[
        Value::NIL,
        Value::TRUE,
        Value::FALSE,
        Value::int(0),
        Value::int(1),
        Value::int(-1),
        Value::int(i64::MAX),
        Value::int(i64::MIN),
        Value::float(0.0),
        Value::float(-0.0),    // equiv to 0.0 → hashes must match
        Value::float(1.0),
        Value::float(-1.0),
        Value::char('a'),
        Value::char('λ'),
    ];

    for &a in xs {
        for &b in xs {
            if eq(a, b) {
                let ha = rt::hash(a).as_int().unwrap();
                let hb = rt::hash(b).as_int().unwrap();
                assert_eq!(
                    ha, hb,
                    "contract violation: equiv true but hashes differ \
                     (a.tag={}, b.tag={})",
                    a.tag, b.tag,
                );
            }
        }
    }
}
