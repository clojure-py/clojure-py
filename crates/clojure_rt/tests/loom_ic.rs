//! Loom: IC publish/read race. Validates that a reader either sees a
//! consistent (key, fn_ptr) pair or falls through cleanly — never gets
//! a torn pair.
//!
//! The protocol mirrors `crate::dispatch::ic::ICSlot`:
//! - `PUBLISHING_BIT` (bit 63 of `key`) serializes writers via
//!   spin-CAS, and makes `key` non-matching while a publish is in
//!   flight.
//! - `seq` (separate atomic) bumps at the END of every publish; the
//!   reader's `seq1 == seq2` check rejects any window spanning a
//!   completed publish — even cross-publish windows where `key`
//!   happens to land on the same value before and after.

#![cfg(loom)]

use loom::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

const FP_OLD: usize = 0xAA;
const FP_NEW_A: usize = 0xBA;
const FP_NEW_B: usize = 0xBB;
const KEY_OLD: u64 = 0x10000_0001;
const KEY_NEW_A: u64 = 0x20000_0001;
const KEY_NEW_B: u64 = 0x30000_0001;

const PUBLISHING_BIT: u64 = 1 << 63;

struct IC {
    key: AtomicU64,
    fp: AtomicPtr<()>,
    seq: AtomicU64,
}

fn publish(ic: &IC, key: u64, fp: *const ()) {
    loop {
        let cur = ic.key.load(Ordering::Relaxed);
        if cur & PUBLISHING_BIT != 0 {
            loom::hint::spin_loop();
            continue;
        }
        if ic.key.compare_exchange_weak(
            cur,
            cur | PUBLISHING_BIT,
            Ordering::Acquire,
            Ordering::Relaxed,
        ).is_ok() {
            break;
        }
        loom::hint::spin_loop();
    }
    let s = ic.seq.load(Ordering::Relaxed);
    ic.seq.store(s.wrapping_add(1), Ordering::Release);  // odd
    ic.fp.store(fp as *mut _, Ordering::Release);
    ic.seq.store(s.wrapping_add(2), Ordering::Release);  // even
    ic.key.store(key, Ordering::Release);                 // release lock LAST
}

fn read(ic: &IC, want: u64) -> Option<*mut ()> {
    let seq1 = ic.seq.load(Ordering::Acquire);
    if seq1 & 1 != 0 { return None; }
    let k = ic.key.load(Ordering::Acquire);
    if k != want { return None; }
    let f = ic.fp.load(Ordering::Acquire);
    let seq2 = ic.seq.load(Ordering::Acquire);
    if seq1 == seq2 { Some(f) } else { None }
}

#[test]
fn double_read_never_returns_torn_pair() {
    loom::model(|| {
        let ic = Arc::new(IC {
            key: AtomicU64::new(KEY_OLD),
            fp:  AtomicPtr::new(FP_OLD as *mut ()),
            seq: AtomicU64::new(0),
        });
        let ic_w = ic.clone();
        let ic_r = ic.clone();

        let writer = thread::spawn(move || {
            publish(&ic_w, KEY_NEW_A, FP_NEW_A as *const ());
        });
        let reader = thread::spawn(move || {
            // Reader looking for the OLD key. Must either see (KEY_OLD, FP_OLD)
            // or fall through; must never see (KEY_OLD, FP_NEW_A).
            if let Some(fp) = read(&ic_r, KEY_OLD) {
                assert_eq!(fp as usize, FP_OLD,
                           "torn read: matched OLD key but observed NEW fp");
            }
        });
        writer.join().unwrap();
        reader.join().unwrap();
    });
}

#[test]
fn two_writers_one_reader_no_torn_pair() {
    loom::model(|| {
        let ic = Arc::new(IC {
            key: AtomicU64::new(KEY_OLD),
            fp:  AtomicPtr::new(FP_OLD as *mut ()),
            seq: AtomicU64::new(0),
        });
        let ic_w_a = ic.clone();
        let ic_w_b = ic.clone();
        let ic_r   = ic.clone();

        let writer_a = thread::spawn(move || {
            publish(&ic_w_a, KEY_NEW_A, FP_NEW_A as *const ());
        });
        let writer_b = thread::spawn(move || {
            publish(&ic_w_b, KEY_NEW_B, FP_NEW_B as *const ());
        });
        let reader = thread::spawn(move || {
            // Reader probes each plausible key. Whatever it gets back
            // must be the fp paired with that key — no cross-pair.
            for &(want, expected_fp) in &[
                (KEY_OLD,   FP_OLD),
                (KEY_NEW_A, FP_NEW_A),
                (KEY_NEW_B, FP_NEW_B),
            ] {
                if let Some(fp) = read(&ic_r, want) {
                    assert_eq!(
                        fp as usize, expected_fp,
                        "torn read: matched key {want:#x} but observed fp {:#x}",
                        fp as usize,
                    );
                }
            }
        });
        writer_a.join().unwrap();
        writer_b.join().unwrap();
        reader.join().unwrap();
    });
}
