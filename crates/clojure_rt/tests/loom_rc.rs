//! Loom model-check: biased RC + escape op. Run with:
//!   RUSTFLAGS="--cfg loom" cargo test -p clojure_rt --test loom_rc

#![cfg(loom)]

use loom::sync::atomic::{AtomicI32, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Re-implementation of dup_heap / drop_heap / share_heap against
/// loom's atomic types. The kernel uses `core::sync::atomic`; loom
/// requires its own types, so this test mirrors the algorithm.
fn dup(rc: &AtomicI32) {
    let r = rc.load(Ordering::Relaxed);
    if r < 0 { rc.store(r - 1, Ordering::Relaxed); }
    else     { rc.fetch_add(1, Ordering::Relaxed); }
}

fn drop_op(rc: &AtomicI32) -> bool {
    let r = rc.load(Ordering::Relaxed);
    if r < 0 {
        let new = r + 1;
        rc.store(new, Ordering::Relaxed);
        new == 0
    } else {
        let prev = rc.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            loom::sync::atomic::fence(Ordering::Acquire);
            true
        } else { false }
    }
}

fn share(rc: &AtomicI32) {
    loop {
        let r = rc.load(Ordering::Relaxed);
        if r > 0 { return; }
        if rc.compare_exchange(r, -r, Ordering::Release, Ordering::Relaxed).is_ok() {
            return;
        }
    }
}

#[test]
fn escape_then_two_threads_drop_to_zero_exactly_once() {
    loom::model(|| {
        let rc = Arc::new(AtomicI32::new(-2));   // biased, count=2
        // Owner does the escape, then threads each drop once.
        let rc_owner = rc.clone();
        share(&rc_owner);                         // -2 -> +2
        let rc_a = rc.clone();
        let rc_b = rc.clone();
        let ja = thread::spawn(move || drop_op(&rc_a));
        let jb = thread::spawn(move || drop_op(&rc_b));
        let za = ja.join().unwrap();
        let zb = jb.join().unwrap();
        assert!(za ^ zb, "exactly one thread must observe drop-to-zero");
        assert_eq!(rc.load(Ordering::Relaxed), 0);
    });
}

#[test]
fn shared_dup_drop_balance() {
    loom::model(|| {
        let rc = Arc::new(AtomicI32::new(1));     // shared, count=1
        let rc_a = rc.clone();
        let rc_b = rc.clone();
        let ja = thread::spawn(move || { dup(&rc_a); drop_op(&rc_a); });
        let jb = thread::spawn(move || { dup(&rc_b); drop_op(&rc_b); });
        ja.join().unwrap();
        jb.join().unwrap();
        assert_eq!(rc.load(Ordering::Relaxed), 1);
    });
}
