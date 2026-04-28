//! `Delay` direct tests — single realization, caching, realized?,
//! exception during realization leaves the delay un-realized for
//! retry.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use clojure_rt::{drop_value, init, rt, Value};

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn deref_runs_thunk_once() {
    init();
    let counter = Arc::new(AtomicI64::new(0));
    let counter_clone = Arc::clone(&counter);
    let d = rt::delay(Box::new(move || {
        counter_clone.fetch_add(1, Ordering::Relaxed);
        Value::int(42)
    }));
    // First deref realizes — counter becomes 1.
    let v1 = rt::deref(d);
    assert_eq!(v1.as_int(), Some(42));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    // Second deref returns the cached value — counter stays at 1.
    let v2 = rt::deref(d);
    assert_eq!(v2.as_int(), Some(42));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    // Force is the same path (alias for deref on delays).
    let v3 = rt::force(d);
    assert_eq!(v3.as_int(), Some(42));
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    drop_all(&[v1, v2, v3, d]);
}

#[test]
fn realized_query_flips_after_first_force() {
    init();
    let d = rt::delay(Box::new(|| Value::int(7)));
    assert_eq!(rt::is_realized(d).as_bool(), Some(false));
    let v = rt::deref(d);
    drop_value(v);
    assert_eq!(rt::is_realized(d).as_bool(), Some(true));
    drop_value(d);
}

#[test]
fn delay_caches_heap_value_with_proper_refcount() {
    init();
    // The delay produces a Value that carries a heap allocation
    // (an interned keyword). Repeated derefs should each return a
    // fresh +1 ref without leaking or under-counting.
    let kw = rt::keyword(None, "answer");
    let kw_for_delay = kw;
    let d = rt::delay(Box::new(move || {
        clojure_rt::rc::dup(kw_for_delay);
        kw_for_delay
    }));
    let a = rt::deref(d);
    let b = rt::deref(d);
    let c = rt::deref(d);
    // All three should equal the original keyword (interning makes
    // the heap pointer identical too, but we only check equality).
    assert!(rt::equiv(a, kw).as_bool().unwrap_or(false));
    assert!(rt::equiv(b, kw).as_bool().unwrap_or(false));
    assert!(rt::equiv(c, kw).as_bool().unwrap_or(false));
    drop_all(&[a, b, c, d, kw]);
}

#[test]
fn exception_during_realization_does_not_cache() {
    init();
    let counter = Arc::new(AtomicI64::new(0));
    let counter_clone = Arc::clone(&counter);
    let d = rt::delay(Box::new(move || {
        let n = counter_clone.fetch_add(1, Ordering::Relaxed);
        if n == 0 {
            // First call returns an exception value — the delay
            // should not cache and should rerun the thunk on the
            // next access.
            clojure_rt::exception::make_foreign("boom".to_string())
        } else {
            Value::int(99)
        }
    }));
    let v1 = rt::deref(d);
    assert!(v1.is_exception());
    drop_value(v1);
    // Not realized — the exception path should leave the thunk
    // intact so a retry can succeed.
    assert_eq!(rt::is_realized(d).as_bool(), Some(false));
    let v2 = rt::deref(d);
    assert_eq!(v2.as_int(), Some(99));
    assert_eq!(rt::is_realized(d).as_bool(), Some(true));
    drop_all(&[v2, d]);
}
