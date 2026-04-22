//! Model: two threads concurrently CAS-increment a shared atomic. Total
//! increments applied must equal total CAS attempts-that-succeeded. This
//! validates the shape of our alter-var-root retry loop.

#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::atomic::{AtomicI64, Ordering};
use loom::thread;

#[test]
fn alter_var_root_is_linearizable() {
    loom::model(|| {
        let v = Arc::new(AtomicI64::new(0));
        let v1 = Arc::clone(&v);
        let v2 = Arc::clone(&v);
        let t1 = thread::spawn(move || loop {
            let cur = v1.load(Ordering::Acquire);
            if v1.compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                break;
            }
        });
        let t2 = thread::spawn(move || loop {
            let cur = v2.load(Ordering::Acquire);
            if v2.compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                break;
            }
        });
        t1.join().unwrap();
        t2.join().unwrap();
        assert_eq!(v.load(Ordering::Acquire), 2);
    });
}
