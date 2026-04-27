//! Stress reproducer for the IC publish race under multiple
//! concurrent writers.
//!
//! The publish protocol is `key.store(0); fn_ptr.store(); key.store()`.
//! Two threads racing through this can interleave such that thread
//! A's `key` is visible while thread B's `fn_ptr` is the published
//! one — a torn cross-pair. The reader's double-key check accepts
//! the cross-pair because both `key` reads return A's value.
//!
//! This test puts two writers and a swarm of readers on the same
//! `ICSlot`. Each `(key, fn)` pair is internally consistent (the fn
//! pointer encodes the key as its low bits). A reader that sees
//! `Some(f)` must satisfy `decode(f) == key` — otherwise the IC
//! produced a torn pair.

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use clojure_rt::dispatch::ic::ICSlot;
use clojure_rt::dispatch::MethodFn;
use clojure_rt::Value;

// Two distinct (key, fn) pairs that share a deterministic encoding:
// the low byte of the fn-ptr address encodes which writer it came
// from. We can't actually use real fn pointers cheaply since the
// payload encoded in the fn pointer would need to be a real function;
// instead, we cast a tagged usize as a *const (), and the read-side
// just checks the tag matches the expected one for each key. We do
// NOT call the fn pointer (it's never invoked).

const KEY_A: u64 = 0x0000_0010_0000_0001;
const KEY_B: u64 = 0x0000_0020_0000_0001;
const FN_A_TAG: usize = 0xAAAA_AAAA;
const FN_B_TAG: usize = 0xBBBB_BBBB;

// Map the canonical fn-ptr we publish for each key. Stored as
// addresses; never dereferenced.
fn fn_for(key: u64) -> *const () {
    if key == KEY_A { FN_A_TAG as *const () }
    else if key == KEY_B { FN_B_TAG as *const () }
    else { panic!("unexpected key 0x{key:x}") }
}

#[test]
fn no_torn_pair_under_two_writers_many_readers() {
    let ic = Arc::new(ICSlot::EMPTY);
    let writers_done = Arc::new(AtomicBool::new(false));
    let torn_observed = Arc::new(AtomicU64::new(0));

    const WRITER_ITERS: usize = 200_000;
    const N_READERS: usize = 4;

    let mut writer_handles = Vec::new();
    let mut reader_handles = Vec::new();

    // Writer A.
    {
        let ic = ic.clone();
        writer_handles.push(thread::spawn(move || {
            for _ in 0..WRITER_ITERS {
                ic.publish(KEY_A, fn_for(KEY_A));
            }
        }));
    }

    // Writer B.
    {
        let ic = ic.clone();
        writer_handles.push(thread::spawn(move || {
            for _ in 0..WRITER_ITERS {
                ic.publish(KEY_B, fn_for(KEY_B));
            }
        }));
    }

    // Readers — loop until writers signal done; assert every Some(f)
    // observation is internally consistent.
    for _ in 0..N_READERS {
        let ic = ic.clone();
        let writers_done = writers_done.clone();
        let torn_observed = torn_observed.clone();
        reader_handles.push(thread::spawn(move || {
            let keys = [KEY_A, KEY_B];
            let mut i = 0usize;
            while !writers_done.load(Ordering::Relaxed) {
                let want = keys[i & 1];
                if let Some(f) = ic.read(want) {
                    let f_addr = f as *const () as usize;
                    let expected = fn_for(want) as usize;
                    if f_addr != expected {
                        torn_observed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                black_box(want);
                i = i.wrapping_add(1);
            }
        }));
    }

    // Wait for both writers to finish, then signal readers to exit.
    for h in writer_handles {
        h.join().expect("writer panicked");
    }
    writers_done.store(true, Ordering::Release);
    for h in reader_handles {
        h.join().expect("reader panicked");
    }

    let n = torn_observed.load(Ordering::Relaxed);
    assert_eq!(
        n, 0,
        "torn (key, fn) pair observed {n} times — IC publish race is unfixed",
    );
}

// Suppress unused warning — MethodFn / Value imports used only for type docs.
#[allow(dead_code)]
const _DOC_REFS: (Option<MethodFn>, Value) = (None, Value::NIL);
