//! The BINDING_STACK itself is thread-local so cross-thread contention on
//! the stack doesn't exist. What DOES happen cross-thread is: `bound-fn*`
//! captures an Arc-shared snapshot and threads it to another thread.
//! Loom validates that Arc-sharing a read-only snapshot between threads is
//! race-free.

#![cfg(loom)]

use loom::sync::Arc;
use loom::thread;

#[test]
fn bound_fn_snapshot_is_safe_cross_thread() {
    loom::model(|| {
        let snap = Arc::new(vec![1, 2, 3]);
        let s1 = Arc::clone(&snap);
        let s2 = Arc::clone(&snap);
        let t1 = thread::spawn(move || s1.iter().sum::<i32>());
        let t2 = thread::spawn(move || s2.iter().sum::<i32>());
        assert_eq!(t1.join().unwrap(), 6);
        assert_eq!(t2.join().unwrap(), 6);
    });
}
