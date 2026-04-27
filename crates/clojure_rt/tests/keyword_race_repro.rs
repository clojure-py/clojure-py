//! Stress reproducer for the biased-refcount race in `KeywordObj::intern`.
//!
//! `intern` publishes the keyword (and its reachable Symbol + String
//! subgraph) into a global `KEYWORD_TABLE`, but doesn't first flip them
//! to shared-RC mode. Two threads doing `dup`/`drop` concurrently on a
//! biased-mode refcount race on a non-atomic store, eventually
//! producing a torn refcount, which can drop to 0 prematurely and
//! free the keyword while another thread is reading it.
//!
//! This test hammers the contention pattern in a tight loop. Without
//! the fix, it crashes (panic, segfault, or assertion) within a few
//! seconds. With the fix, it completes cleanly.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use clojure_rt::{drop_value, init, rt};

#[test]
fn intern_and_hash_under_thread_contention() {
    init();

    const N_THREADS: usize = 8;
    const ITERS_PER_THREAD: usize = 5_000;

    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    for tid in 0..N_THREADS {
        let stop = stop.clone();
        handles.push(thread::spawn(move || {
            for i in 0..ITERS_PER_THREAD {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                // Mix of always-same and per-thread-ish names so we
                // exercise both the read-fast-path (high contention)
                // and the slow-path-allocate path. Most calls hit the
                // shared "foo" interned keyword.
                let kw = if i % 4 == 0 {
                    rt::keyword(None, &format!("k_{tid}_{i}"))
                } else {
                    rt::keyword(None, "foo")
                };
                let h = rt::hash(kw);
                // Force a use of the result so the optimizer can't
                // elide the dispatch.
                std::hint::black_box(h);
                drop_value(kw);
            }
        }));
    }

    for h in handles {
        h.join().expect("worker panicked");
    }
}
