//! `PersistentHashMap` direct tests + cross-type round-trip with
//! `PersistentArrayMap`.

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::hash_map::PersistentHashMap;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn empty_hash_map_count_zero() {
    init();
    let m = PersistentHashMap::from_kvs(&[]);
    assert_eq!(rt::count(m).as_int(), Some(0));
    drop_value(m);
}

#[test]
fn assoc_then_lookup_round_trips() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m0 = PersistentHashMap::from_kvs(&[]);
    let m1 = PersistentHashMap::assoc_kv(m0, ka, Value::int(1));
    let m2 = PersistentHashMap::assoc_kv(m1, kb, Value::int(2));
    assert_eq!(rt::count(m2).as_int(), Some(2));
    assert_eq!(rt::get(m2, ka).as_int(), Some(1));
    assert_eq!(rt::get(m2, kb).as_int(), Some(2));
    drop_all(&[m0, m1, m2, ka, kb]);
}

#[test]
fn assoc_replaces_existing_value() {
    init();
    let ka = rt::keyword(None, "a");
    let m1 = PersistentHashMap::from_kvs(&[ka, Value::int(1)]);
    let m2 = PersistentHashMap::assoc_kv(m1, ka, Value::int(99));
    assert_eq!(rt::count(m2).as_int(), Some(1));
    assert_eq!(rt::get(m2, ka).as_int(), Some(99));
    // Original unchanged.
    assert_eq!(rt::get(m1, ka).as_int(), Some(1));
    drop_all(&[m1, m2, ka]);
}

#[test]
fn dissoc_removes_existing() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(1), kb, Value::int(2)]);
    let m2 = PersistentHashMap::dissoc_k(m, ka);
    assert_eq!(rt::count(m2).as_int(), Some(1));
    assert!(rt::get(m2, ka).is_nil());
    assert_eq!(rt::get(m2, kb).as_int(), Some(2));
    drop_all(&[m, m2, ka, kb]);
}

#[test]
fn lookup_default_on_miss() {
    init();
    let ka = rt::keyword(None, "a");
    let kc = rt::keyword(None, "c");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(1)]);
    let dflt = Value::int(-1);
    assert_eq!(rt::get_default(m, kc, dflt).as_int(), Some(-1));
    drop_all(&[m, ka, kc]);
}

#[test]
fn contains_key_present_and_missing() {
    init();
    let ka = rt::keyword(None, "a");
    let kc = rt::keyword(None, "c");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(1)]);
    assert_eq!(rt::contains_key(m, ka).as_bool(), Some(true));
    assert_eq!(rt::contains_key(m, kc).as_bool(), Some(false));
    drop_all(&[m, ka, kc]);
}

#[test]
fn find_returns_real_map_entry() {
    init();
    let ka = rt::keyword(None, "a");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(7)]);
    let e = rt::find(m, ka);
    let k = rt::key(e);
    let v = rt::val(e);
    assert!(rt::equiv(k, ka).as_bool().unwrap_or(false));
    assert_eq!(v.as_int(), Some(7));
    drop_all(&[k, v, e, m, ka]);
}

#[test]
fn lookup_distinguishes_present_nil_from_missing() {
    init();
    let ka = rt::keyword(None, "a");
    let m = PersistentHashMap::from_kvs(&[ka, Value::NIL]);
    assert!(rt::get(m, ka).is_nil());
    assert_eq!(rt::contains_key(m, ka).as_bool(), Some(true));
    let dflt = Value::int(-1);
    assert!(rt::get_default(m, ka, dflt).is_nil());
    drop_all(&[m, ka]);
}

#[test]
fn many_entries_drives_trie_through_multiple_levels() {
    // 100 distinct integer keys force the HAMT to develop multiple
    // levels (single-level fits ~32 entries before the bitmap fills).
    init();
    let mut m = PersistentHashMap::from_kvs(&[]);
    for i in 0..100i64 {
        let nm = PersistentHashMap::assoc_kv(m, Value::int(i), Value::int(i * 10));
        drop_value(m);
        m = nm;
    }
    assert_eq!(rt::count(m).as_int(), Some(100));
    for i in 0..100i64 {
        let r = rt::get(m, Value::int(i));
        assert_eq!(r.as_int(), Some(i * 10), "lookup {i}");
        drop_value(r);
    }
    drop_value(m);
}

#[test]
fn dissoc_through_trie_levels() {
    init();
    let mut m = PersistentHashMap::from_kvs(&[]);
    for i in 0..100i64 {
        let nm = PersistentHashMap::assoc_kv(m, Value::int(i), Value::int(i));
        drop_value(m);
        m = nm;
    }
    // Remove every other.
    for i in (0..100i64).step_by(2) {
        let nm = PersistentHashMap::dissoc_k(m, Value::int(i));
        drop_value(m);
        m = nm;
    }
    assert_eq!(rt::count(m).as_int(), Some(50));
    for i in 0..100i64 {
        let r = rt::get(m, Value::int(i));
        if i % 2 == 0 {
            assert!(r.is_nil(), "expected miss at {i}");
        } else {
            assert_eq!(r.as_int(), Some(i));
            drop_value(r);
        }
    }
    drop_value(m);
}

#[test]
fn seq_walks_all_entries() {
    init();
    let mut m = PersistentHashMap::from_kvs(&[]);
    for i in 0..50i64 {
        let nm = PersistentHashMap::assoc_kv(m, Value::int(i), Value::int(i * 2));
        drop_value(m);
        m = nm;
    }
    let mut s = rt::seq(m);
    let mut collected: Vec<(i64, i64)> = Vec::new();
    while !s.is_nil() {
        let e = rt::first(s);
        let k = rt::key(e);
        let v = rt::val(e);
        collected.push((k.as_int().unwrap(), v.as_int().unwrap()));
        drop_value(k); drop_value(v); drop_value(e);
        let n = rt::next(s);
        drop_value(s);
        s = n;
    }
    drop_value(s);
    drop_value(m);
    collected.sort();
    let expected: Vec<(i64, i64)> = (0..50).map(|i| (i, i * 2)).collect();
    assert_eq!(collected, expected);
}

#[test]
fn promotion_at_eight_entries() {
    // 8 entries fit ArrayMap; a 9th-key insert triggers promotion.
    init();
    let kws: Vec<Value> = (0..9)
        .map(|i| rt::keyword(None, &format!("k{i}")))
        .collect();

    let mut am = rt::array_map(&[]);
    for i in 0..8 {
        let nm = rt::assoc(am, kws[i], Value::int(i as i64));
        drop_value(am);
        am = nm;
    }
    // Eighth-add (already 8 entries, ninth key) → promotion.
    let promoted = rt::assoc(am, kws[8], Value::int(8));
    let am_id = clojure_rt::types::array_map::PERSISTENTARRAYMAP_TYPE_ID
        .get().copied().unwrap_or(0);
    let hm_id = clojure_rt::types::hash_map::PERSISTENTHASHMAP_TYPE_ID
        .get().copied().unwrap_or(0);
    assert_eq!(am.tag, am_id, "pre-promotion am");
    assert_eq!(promoted.tag, hm_id, "post-promotion is hash map");
    assert_eq!(rt::count(promoted).as_int(), Some(9));
    for (i, k) in kws.iter().enumerate() {
        assert_eq!(rt::get(promoted, *k).as_int(), Some(i as i64));
    }
    drop_value(am);
    drop_value(promoted);
    for k in kws.into_iter() { drop_value(k); }
}

#[test]
fn cross_type_equiv_array_map_vs_hash_map() {
    init();
    let kws: Vec<Value> = (0..10)
        .map(|i| rt::keyword(None, &format!("k{i}")))
        .collect();

    // Build an HM with 10 entries (forced via promotion path).
    let mut am = rt::array_map(&[]);
    for i in 0..10 {
        let nm = rt::assoc(am, kws[i], Value::int(i as i64));
        drop_value(am);
        am = nm;
    }
    // Build a fresh HM directly with the same entries.
    let mut hm = PersistentHashMap::from_kvs(&[]);
    for i in 0..10 {
        let nm = PersistentHashMap::assoc_kv(hm, kws[i], Value::int(i as i64));
        drop_value(hm);
        hm = nm;
    }
    // am here is a PHM (promoted). hm is a directly-built PHM. They
    // should equiv by walking-and-looking-up.
    assert!(rt::equiv(am, hm).as_bool().unwrap_or(false));

    // Now make a small AM with 3 entries, and the matching HM with 3
    // entries (built via from_kvs without promotion).
    let small_am = rt::array_map(&[
        kws[0], Value::int(0), kws[1], Value::int(1), kws[2], Value::int(2),
    ]);
    let small_hm = PersistentHashMap::from_kvs(&[
        kws[0], Value::int(0), kws[1], Value::int(1), kws[2], Value::int(2),
    ]);
    assert!(rt::equiv(small_am, small_hm).as_bool().unwrap_or(false));
    assert!(rt::equiv(small_hm, small_am).as_bool().unwrap_or(false));

    drop_value(am); drop_value(hm);
    drop_value(small_am); drop_value(small_hm);
    for k in kws.into_iter() { drop_value(k); }
}

#[test]
fn hash_same_for_equal_maps() {
    init();
    let ka = rt::keyword(None, "a");
    let kb = rt::keyword(None, "b");
    let m1 = PersistentHashMap::from_kvs(&[ka, Value::int(1), kb, Value::int(2)]);
    let m2 = PersistentHashMap::from_kvs(&[kb, Value::int(2), ka, Value::int(1)]);
    assert_eq!(rt::hash(m1).as_int(), rt::hash(m2).as_int());
    drop_all(&[m1, m2, ka, kb]);
}

#[test]
fn empty_collection_returns_canonical_empty_hash_map() {
    init();
    let ka = rt::keyword(None, "a");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(1)]);
    let e = rt::empty(m);
    assert_eq!(rt::count(e).as_int(), Some(0));
    drop_all(&[m, e, ka]);
}

#[test]
fn with_meta_preserves_entries() {
    init();
    let ka = rt::keyword(None, "a");
    let m = PersistentHashMap::from_kvs(&[ka, Value::int(1)]);
    let meta = rt::array_map(&[rt::keyword(None, "tag"), Value::int(99)]);
    let m2 = rt::with_meta(m, meta);
    assert!(rt::equiv(m, m2).as_bool().unwrap_or(false));
    drop_all(&[m, m2, meta, ka]);
}
