//! `ArrayChunk` — count, nth, drop_first, refcount discipline.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::array_chunk::ArrayChunk;

fn ints(xs: &[i64]) -> Vec<Value> { xs.iter().map(|&n| Value::int(n)).collect() }

#[test]
fn chunk_count_matches_input_length() {
    init();
    let c = ArrayChunk::from_vec(ints(&[1, 2, 3, 4]));
    assert_eq!(rt::count(c).as_int(), Some(4));
    drop_value(c);
}

#[test]
fn chunk_nth_reads_elements() {
    init();
    let c = ArrayChunk::from_vec(ints(&[10, 20, 30]));
    let r = rt::nth(c, Value::int(1));
    assert_eq!(r.as_int(), Some(20));
    drop_value(r);
    drop_value(c);
}

#[test]
fn chunk_nth_default_returns_default_for_oob() {
    init();
    let c = ArrayChunk::from_vec(ints(&[10, 20, 30]));
    let dflt = Value::int(-1);
    let r = rt::nth_default(c, Value::int(5), dflt);
    assert_eq!(r.as_int(), Some(-1));
    drop_value(c);
}

#[test]
fn drop_first_advances_and_count_decreases() {
    init();
    let c = ArrayChunk::from_vec(ints(&[1, 2, 3, 4]));
    let c2 = clojure_rt_macros::dispatch!(
        clojure_rt::protocols::chunk::IChunk::drop_first, &[c]
    );
    assert_eq!(rt::count(c2).as_int(), Some(3));
    let r = rt::nth(c2, Value::int(0));
    assert_eq!(r.as_int(), Some(2));
    drop_value(r);
    drop_value(c2);
    drop_value(c);
}

#[test]
fn drop_first_does_not_disturb_original() {
    init();
    let c = ArrayChunk::from_vec(ints(&[1, 2, 3, 4]));
    let _c2 = clojure_rt_macros::dispatch!(
        clojure_rt::protocols::chunk::IChunk::drop_first, &[c]
    );
    // Original chunk still holds 4 elements.
    assert_eq!(rt::count(c).as_int(), Some(4));
    let r0 = rt::nth(c, Value::int(0));
    assert_eq!(r0.as_int(), Some(1));
    drop_value(r0);
    drop_value(_c2);
    drop_value(c);
}
