//! Loom: IC publish/read race. Validate that a reader either sees a
//! consistent (key, fn_ptr) pair or falls through cleanly — never gets
//! a torn pair.

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

const FP_OLD: usize = 0xAA;
const FP_NEW: usize = 0xBB;
const KEY_OLD: u64 = 0x10000_0001;
const KEY_NEW: u64 = 0x20000_0001;

struct IC { key: AtomicU64, fp: AtomicPtr<()> }

fn publish(ic: &IC, key: u64, fp: *const ()) {
    ic.key.store(0, Ordering::Release);
    ic.fp.store(fp as *mut _, Ordering::Release);
    ic.key.store(key, Ordering::Release);
}

fn read(ic: &IC, want: u64) -> Option<*mut ()> {
    let k1 = ic.key.load(Ordering::Acquire);
    if k1 != want { return None; }
    let f  = ic.fp.load(Ordering::Acquire);
    let k2 = ic.key.load(Ordering::Acquire);
    if k1 == k2 { Some(f) } else { None }
}

#[test]
fn double_read_never_returns_torn_pair() {
    loom::model(|| {
        let ic = Arc::new(IC {
            key: AtomicU64::new(KEY_OLD),
            fp:  AtomicPtr::new(FP_OLD as *mut ()),
        });
        let ic_w = ic.clone();
        let ic_r = ic.clone();

        let writer = thread::spawn(move || {
            publish(&ic_w, KEY_NEW, FP_NEW as *const ());
        });
        let reader = thread::spawn(move || {
            // Reader looking for the OLD key. Must either see (KEY_OLD, FP_OLD)
            // or fall through; must never see (KEY_OLD, FP_NEW).
            if let Some(fp) = read(&ic_r, KEY_OLD) {
                assert_eq!(fp as usize, FP_OLD,
                           "torn read: matched OLD key but observed NEW fp");
            }
        });
        writer.join().unwrap();
        reader.join().unwrap();
    });
}
