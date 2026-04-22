//! Model: Thread A inserts a method table entry then bumps the epoch.
//! Thread B loads (epoch, entry) and asserts: if it observes the entry, it
//! also observes epoch ≥ the bumped value.
//!
//! This exercises the `bump_epoch` + `entries.insert` ordering our production
//! `MethodCache` uses. We replicate the pattern with loom's atomics.

#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::Mutex;
use loom::thread;

#[test]
fn concurrent_extend_and_dispatch_sees_consistent_cache() {
    loom::model(|| {
        let epoch = Arc::new(AtomicU64::new(0));
        let entry: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

        // Writer: install entry then bump epoch (mirrors our extend_type order).
        let e1 = Arc::clone(&epoch);
        let en1 = Arc::clone(&entry);
        let t1 = thread::spawn(move || {
            *en1.lock().unwrap() = Some(42);
            e1.fetch_add(1, Ordering::Release);
        });

        // Reader: loads epoch (Acquire), then entry. Invariant: if epoch was
        // bumped (≥ 1), the entry write before the Release is visible.
        let e2 = Arc::clone(&epoch);
        let en2 = Arc::clone(&entry);
        let t2 = thread::spawn(move || {
            // Release/Acquire pairing: if the reader observes the bumped epoch, the
            // writer's prior stores (including the entry insertion under the Mutex)
            // must be visible on the subsequent mutex-protected read.
            let ep = e2.load(Ordering::Acquire);
            if ep >= 1 {
                let v = *en2.lock().unwrap();
                assert!(v.is_some(), "epoch bumped but entry not yet visible");
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();
    });
}
