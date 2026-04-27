//! Regression: `rc::dup_heap` and `rc::drop_heap` debug-assert that
//! biased-mode mutations come from the owner thread, catching the
//! "publish a Value across threads without calling `rc::share()`" bug
//! class that caused the singleton heap corruption fixed in commit
//! a393ec7. Release builds compile the assertion out — these tests
//! only run under `cfg(debug_assertions)`.
//!
//! Construction: build a biased Header whose `owner_tid` is *not* the
//! test thread's tid (we simulate "owned by some other thread"). A
//! call to `dup_heap` / `drop_heap` from the test thread should
//! debug-panic — that's the safety net for missing `rc::share()`
//! before cross-thread publication.

#![cfg(debug_assertions)]

use std::sync::atomic::AtomicI32;

use clojure_rt::header::Header;
use clojure_rt::gc::rcimmix::tid::current_owner_tid;

fn biased_owned_by_someone_else() -> Box<Header> {
    let me = current_owner_tid();
    // Pick any tid that isn't ours. Adding 1 is enough.
    Box::new(Header {
        type_id: 16, flags: 0,
        rc: AtomicI32::new(Header::INITIAL_RC),
        owner_tid: me.wrapping_add(1),
    })
}

#[test]
#[should_panic(expected = "biased-mode mutation from non-owner thread")]
fn dup_from_non_owner_thread_panics() {
    let h = biased_owned_by_someone_else();
    unsafe { clojure_rt::rc::dup_heap(&*h); }
}

#[test]
#[should_panic(expected = "biased-mode mutation from non-owner thread")]
fn drop_from_non_owner_thread_panics() {
    let h = biased_owned_by_someone_else();
    unsafe { clojure_rt::rc::drop_heap(&*h); }
}

#[test]
fn share_clears_owner_so_subsequent_dup_is_fine() {
    // The non-buggy path: owner calls share() before publishing.
    // Subsequent dup/drop from any thread takes the atomic branch and
    // never consults owner_tid.
    let h = Box::new(Header {
        type_id: 16, flags: 0,
        rc: AtomicI32::new(Header::INITIAL_RC),
        owner_tid: current_owner_tid(),
    });
    unsafe {
        clojure_rt::rc::share_heap(&*h);
        // owner_tid was zeroed by share; the assertion path is now dead.
        clojure_rt::rc::dup_heap(&*h);
        clojure_rt::rc::drop_heap(&*h);
        clojure_rt::rc::drop_heap(&*h);
    }
    // After two drops on rc=2, the count is 0. We don't run the type
    // destructor here (no real type) — just verifying the assertion
    // doesn't fire.
    assert_eq!(h.rc.load(std::sync::atomic::Ordering::Relaxed), 0);
}
