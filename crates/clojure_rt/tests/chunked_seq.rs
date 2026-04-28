//! `IChunkedSeq` walking on `VecSeq`: chunked-first / chunked-rest /
//! chunked-next stitched together should reproduce the full element
//! sequence, with chunks bounded by 32-element trie blocks (or
//! tail-length for the final one).

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::protocols::chunked_seq::IChunkedSeq;
use clojure_rt::protocols::counted::ICounted;
use clojure_rt::protocols::indexed::IIndexed;

fn ints(xs: &[i64]) -> Vec<Value> { xs.iter().map(|&n| Value::int(n)).collect() }

/// Walk a chunked seq via the chunked API (chunked_first / next),
/// flattening into a Vec<i64> for comparison. Drops every Value
/// it allocates.
fn walk_chunked(mut s: Value) -> Vec<i64> {
    let mut out = Vec::new();
    while !s.is_nil() {
        let chunk = clojure_rt_macros::dispatch!(IChunkedSeq::chunked_first, &[s]);
        let cnt = clojure_rt_macros::dispatch!(ICounted::count, &[chunk])
            .as_int().unwrap();
        for i in 0..cnt {
            let x = clojure_rt_macros::dispatch!(IIndexed::nth, &[chunk, Value::int(i)]);
            out.push(x.as_int().unwrap());
            drop_value(x);
        }
        drop_value(chunk);
        let next = clojure_rt_macros::dispatch!(IChunkedSeq::chunked_next, &[s]);
        drop_value(s);
        s = next;
    }
    drop_value(s);
    out
}

#[test]
fn small_vector_yields_one_chunk_for_the_tail() {
    init();
    let v = rt::vector(&ints(&[1, 2, 3]));
    let s = rt::seq(v);
    assert_eq!(walk_chunked(s), vec![1, 2, 3]);
    drop_value(v);
}

#[test]
fn one_full_block_walks_in_one_trie_chunk_plus_one_tail_chunk() {
    init();
    // 33 elements = one full leaf-block in the trie + 1-element tail.
    let xs: Vec<i64> = (0..33).collect();
    let v = rt::vector(&ints(&xs));
    let s = rt::seq(v);
    assert_eq!(walk_chunked(s), xs);
    drop_value(v);
}

#[test]
fn medium_vector_walks_correctly_across_blocks() {
    init();
    let xs: Vec<i64> = (0..100).collect();
    let v = rt::vector(&ints(&xs));
    let s = rt::seq(v);
    assert_eq!(walk_chunked(s), xs);
    drop_value(v);
}

#[test]
fn root_grown_vector_walks_correctly() {
    init();
    // 2049 elements forces shift = 10 (root grown once).
    let xs: Vec<i64> = (0..2049).collect();
    let v = rt::vector(&ints(&xs));
    let s = rt::seq(v);
    assert_eq!(walk_chunked(s), xs);
    drop_value(v);
}

#[test]
fn chunked_walk_matches_element_walk() {
    init();
    let xs: Vec<i64> = (0..200).collect();
    let v = rt::vector(&ints(&xs));

    // Element-by-element walk via first/next.
    let mut element_collected = Vec::new();
    let mut s = rt::seq(v);
    while !s.is_nil() {
        let x = rt::first(s);
        element_collected.push(x.as_int().unwrap());
        drop_value(x);
        let n = rt::next(s);
        drop_value(s);
        s = n;
    }
    drop_value(s);

    // Chunked walk.
    let s2 = rt::seq(v);
    let chunked_collected = walk_chunked(s2);

    assert_eq!(element_collected, chunked_collected);
    assert_eq!(element_collected, xs);
    drop_value(v);
}

#[test]
fn empty_vector_seq_is_nil() {
    init();
    let v = rt::vector(&[]);
    assert!(rt::seq(v).is_nil());
    drop_value(v);
}
