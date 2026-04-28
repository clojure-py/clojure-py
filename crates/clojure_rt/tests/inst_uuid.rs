//! `InstObj` and `UUIDObj` tests — round-trip, equality on
//! underlying value, hash determinism, parse-error paths.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::inst::InstObj;
use clojure_rt::types::uuid::UUIDObj;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

// --- Inst ---------------------------------------------------------------

#[test]
fn inst_round_trip_via_millis() {
    init();
    let v = rt::inst_from_millis(1_700_000_000_000);
    assert_eq!(InstObj::millis(v), 1_700_000_000_000);
    drop_value(v);
}

#[test]
fn inst_round_trip_via_rfc3339() {
    init();
    // 2024-01-01T00:00:00Z is exactly this ms-since-epoch.
    let v = rt::inst_from_rfc3339("2024-01-01T00:00:00Z");
    assert_eq!(InstObj::millis(v), 1_704_067_200_000);
    drop_value(v);
}

#[test]
fn inst_equiv_same_millis_distinct_objects() {
    init();
    let a = rt::inst_from_millis(1_704_067_200_000);
    let b = rt::inst_from_rfc3339("2024-01-01T00:00:00Z");
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

#[test]
fn inst_equiv_with_timezone_offset() {
    // The same instant expressed in different timezones must
    // compare equal.
    init();
    let a = rt::inst_from_rfc3339("2024-01-01T02:00:00+02:00");
    let b = rt::inst_from_rfc3339("2024-01-01T00:00:00Z");
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

#[test]
fn inst_distinct_millis_not_equiv() {
    init();
    let a = rt::inst_from_millis(1);
    let b = rt::inst_from_millis(2);
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn inst_hash_consistent_for_equal_instants() {
    init();
    let a = rt::inst_from_rfc3339("2024-01-01T02:00:00+02:00");
    let b = rt::inst_from_rfc3339("2024-01-01T00:00:00Z");
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

#[test]
fn inst_invalid_string_returns_exception() {
    init();
    let v = rt::inst_from_rfc3339("not a timestamp");
    assert!(v.is_exception());
    drop_value(v);
}

// --- UUID ---------------------------------------------------------------

#[test]
fn uuid_round_trip() {
    init();
    let s = "550e8400-e29b-41d4-a716-446655440000";
    let a = rt::uuid_from_str(s);
    let b = rt::uuid_from_str(s);
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    assert_eq!(UUIDObj::as_uuid(a), UUIDObj::as_uuid(b));
    drop_all(&[a, b]);
}

#[test]
fn uuid_distinct_values_not_equiv() {
    init();
    let a = rt::uuid_from_str("00000000-0000-0000-0000-000000000001");
    let b = rt::uuid_from_str("00000000-0000-0000-0000-000000000002");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    drop_all(&[a, b]);
}

#[test]
fn uuid_hash_consistent() {
    init();
    let s = "550e8400-e29b-41d4-a716-446655440000";
    let a = rt::uuid_from_str(s);
    let b = rt::uuid_from_str(s);
    assert_eq!(rt::hash(a).as_int(), rt::hash(b).as_int());
    drop_all(&[a, b]);
}

#[test]
fn uuid_invalid_string_returns_exception() {
    init();
    let v = rt::uuid_from_str("definitely-not-a-uuid");
    assert!(v.is_exception());
    drop_value(v);
}

// --- Cross-type ---------------------------------------------------------

#[test]
fn inst_and_uuid_distinct_under_strict_equiv() {
    init();
    let i = rt::inst_from_millis(0);
    let u = rt::uuid_from_str("00000000-0000-0000-0000-000000000000");
    assert_eq!(rt::equiv(i, u).as_bool(), Some(false));
    drop_all(&[i, u]);
}
