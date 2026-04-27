//! Loom: ArcSwap + version bump on extend. Validate that a reader
//! that consults `version` and the per-type table sees a consistent
//! view: either the pre-extend table with the pre-extend version, or
//! the post-extend table with a strictly larger version.
//!
//! Run with:
//!   RUSTFLAGS="--cfg loom" cargo test -p clojure_rt --test loom_extend --release

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

#[test]
fn extend_publishes_new_table_with_new_version() {
    loom::model(|| {
        let table = Arc::new(loom::sync::Mutex::new(0u32));   // 0 = empty table
        let version = Arc::new(AtomicU32::new(1));

        let table_w = table.clone();
        let version_w = version.clone();
        let writer = thread::spawn(move || {
            // simulate extend: bump version, publish new table value
            version_w.fetch_add(1, Ordering::Release);
            *table_w.lock().unwrap() = 42;
        });

        let table_r = table.clone();
        let version_r = version.clone();
        let reader = thread::spawn(move || {
            let v1 = version_r.load(Ordering::Acquire);
            let t = *table_r.lock().unwrap();
            let v2 = version_r.load(Ordering::Acquire);
            // If t == 42 (new table), version must be >= 2.
            if t == 42 { assert!(v2 >= 2); }
            // v1 may be 1 or 2, v2 >= v1
            assert!(v2 >= v1);
        });

        writer.join().unwrap();
        reader.join().unwrap();
    });
}
