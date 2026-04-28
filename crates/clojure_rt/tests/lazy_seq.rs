//! `LazySeq` tests — realization, caching, lazy chain unwrap,
//! concurrent realization.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::cons::Cons;
use clojure_rt::types::lazy_seq::LazySeq;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn lazy_seq_realizes_on_first_access() {
    init();
    let l = LazySeq::from_fn(Box::new(|| {
        Cons::new(Value::int(42), Value::NIL)
    }));
    let f = rt::first(l);
    assert_eq!(f.as_int(), Some(42));
    drop_value(f);
    drop_value(l);
}

#[test]
fn lazy_seq_caches_realization() {
    init();
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let l = LazySeq::from_fn(Box::new(move || {
        counter_clone.fetch_add(1, Ordering::Relaxed);
        Cons::new(Value::int(7), Value::NIL)
    }));

    // Force realization three times.
    let f1 = rt::first(l);
    let f2 = rt::first(l);
    let f3 = rt::first(l);
    assert_eq!(f1.as_int(), Some(7));
    assert_eq!(f2.as_int(), Some(7));
    assert_eq!(f3.as_int(), Some(7));

    // Thunk should have been invoked exactly once.
    assert_eq!(counter.load(Ordering::Relaxed), 1);

    drop_all(&[f1, f2, f3, l]);
}

#[test]
fn lazy_seq_returning_nil_is_empty() {
    init();
    let l = LazySeq::from_fn(Box::new(|| Value::NIL));
    assert!(rt::seq(l).is_nil());
    let r = rt::rest(l);
    assert!(rt::seq(r).is_nil());
    drop_value(r);
    drop_value(l);
}

#[test]
fn lazy_seq_chains_unwrap_via_sval() {
    // Thunk returns another LazySeq, whose thunk returns the actual cons.
    init();
    let l = LazySeq::from_fn(Box::new(|| {
        LazySeq::from_fn(Box::new(|| {
            Cons::new(Value::int(99), Value::NIL)
        }))
    }));
    let f = rt::first(l);
    assert_eq!(f.as_int(), Some(99));
    drop_value(f);
    drop_value(l);
}

#[test]
fn lazy_seq_walked_via_first_next() {
    // Build (1 2 3) lazily: outer LazySeq → Cons(1, inner LazySeq).
    init();
    let l = LazySeq::from_fn(Box::new(|| {
        let inner = LazySeq::from_fn(Box::new(|| {
            let inner2 = LazySeq::from_fn(Box::new(|| {
                Cons::new(Value::int(3), Value::NIL)
            }));
            let r = Cons::new(Value::int(2), inner2);
            drop_value(inner2);
            r
        }));
        let r = Cons::new(Value::int(1), inner);
        drop_value(inner);
        r
    }));

    let mut collected: Vec<i64> = Vec::new();
    let mut walking = rt::seq(l);
    while !walking.is_nil() {
        let f = rt::first(walking);
        collected.push(f.as_int().unwrap());
        drop_value(f);
        let n = rt::next(walking);
        drop_value(walking);
        walking = n;
    }
    assert_eq!(collected, vec![1, 2, 3]);
    drop_value(l);
}

#[test]
fn lazy_seq_hash_matches_equivalent_list() {
    init();
    let l = LazySeq::from_fn(Box::new(|| {
        Cons::new(Value::int(1),
            Cons::new(Value::int(2),
                Cons::new(Value::int(3), Value::NIL)))
    }));
    let plist = rt::list(&[Value::int(1), Value::int(2), Value::int(3)]);
    assert_eq!(rt::hash(l).as_int(), rt::hash(plist).as_int());
    drop_all(&[l, plist]);
}

#[test]
fn lazy_seq_equiv_with_other_lazy_seq() {
    init();
    let a = LazySeq::from_fn(Box::new(|| {
        Cons::new(Value::int(1), Cons::new(Value::int(2), Value::NIL))
    }));
    let b = LazySeq::from_fn(Box::new(|| {
        Cons::new(Value::int(1), Cons::new(Value::int(2), Value::NIL))
    }));
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    drop_all(&[a, b]);
}

// **Cross-thread realize note**: the parking_lot::Mutex inside
// LazySeq guarantees the *thunk* runs at most once even under
// concurrent first/seq/rest from multiple threads. What's NOT yet
// safe is *sharing the realized chain across threads* — the
// allocated Cons cells inside the thunk are biased to the realizing
// thread, and a non-owner thread reading them trips the
// owner-tid debug assertion. A future deep-share pass on realize
// would unlock cross-thread reads; until then, lazy seqs are
// single-thread-realized, single-thread-walked. Tests covering
// concurrent realize land alongside that work.

