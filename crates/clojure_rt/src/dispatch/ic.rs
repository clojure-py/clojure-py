//! Per-call-site inline cache.
//!
//! ## Concurrency model
//!
//! **Reader fast path** — multiple concurrent readers, no
//! synchronization beyond `Acquire` loads. Two layered guards
//! detect any concurrent publish:
//! 1. `key`'s high bit is set during a publish (so a clean reader's
//!    `want` cannot match), guaranteeing a partially-written
//!    `(key, fn_ptr)` pair never satisfies the comparison.
//! 2. `seq` is bumped at the end of every publish. A reader compares
//!    `seq` before and after its `fn_ptr` load; if any publish
//!    completed between, `seq1 != seq2` and the reader falls
//!    through. This catches cross-publish windows where the reader
//!    saw `key_A` at T1, `fn_B` at T2 (during a publish), and
//!    `key_A` again at T3 (after a second publish restored it).
//!
//! **Writer slow path** — `publish` is invoked from `slow_path` on
//! IC miss, and may run concurrently from multiple threads on the
//! same call site. `PUBLISHING_BIT` (bit 63 of `key`) is a spin-CAS
//! lock that serializes writers. Inside the critical section the
//! writer stores `fn_ptr`, then stores the new clean `key`
//! (releasing the lock), then bumps `seq` (signaling completion to
//! any reader currently in their three-load window).
//!
//! ## Layout
//!
//! ```text
//! key (u64):
//!   bit 63:        PUBLISHING_BIT (1 during publish, 0 in stable state)
//!   bits 32..48:   type_id (16 bits, bounded by MAX_TYPES = 2^16)
//!   bits 0..32:    version (32 bits, monotonic)
//! fn_ptr: usize-sized function pointer
//! seq: u64, bumped at end of every publish
//! ```
//!
//! `seq` lives in its own atomic to keep the reader's `key`/`want`
//! comparison a plain equality with no mask — the hot path stays as
//! cheap as the broken predecessor protocol while gaining
//! correctness.

use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use crate::dispatch::MethodFn;
use crate::protocol::ProtocolMethod;
use crate::value::Value;

/// High bit of `key` — set during a publish, clear in stable state.
const PUBLISHING_BIT: u64 = 1 << 63;

#[repr(C, align(16))]
pub struct ICSlot {
    pub key:    AtomicU64,
    pub fn_ptr: AtomicPtr<()>,
    pub seq:    AtomicU64,
}

impl ICSlot {
    /// Initializer template for `[ICSlot::EMPTY; N]`. Each array slot is
    /// independently constructed from this const expression — the const
    /// itself is never used as a shared instance.
    #[allow(clippy::declare_interior_mutable_const)]
    pub const EMPTY: ICSlot = ICSlot {
        key:    AtomicU64::new(0),
        fn_ptr: AtomicPtr::new(null_mut()),
        seq:    AtomicU64::new(0),
    };

    /// Pack (type_id, version) into a u64 key.
    ///
    /// **Invariant**: bit 63 must be zero (`PUBLISHING_BIT` is the
    /// publish-lock sentinel). Holds because Type IDs are bounded by
    /// `MAX_TYPES = 2^16`. The `debug_assert` defends against future
    /// growth that would overflow the budget.
    #[inline(always)]
    pub fn make_key(type_id: u32, version: u32) -> u64 {
        let k = ((type_id as u64) << 32) | (version as u64);
        debug_assert!(
            k & PUBLISHING_BIT == 0,
            "make_key: type_id {type_id} sets reserved bit 63",
        );
        k
    }

    /// Slow-path update protocol — Linux-style seqlock with writer
    /// exclusion via `PUBLISHING_BIT`. The publish order is chosen
    /// so the lock-release `key.store` is the LAST operation: that
    /// way the prior writer's `seq.store(even)` is program-order
    /// before its lock release, and the next writer's CAS-Acquire
    /// on `key` synchronizes-with the entire publish (including the
    /// final seq update). A plain `Relaxed` seq load then suffices
    /// to read the latest committed value.
    ///
    /// 1. Spin-CAS to acquire `PUBLISHING_BIT` on `key`.
    /// 2. Read prior `seq` (Relaxed — synchronization came via the
    ///    CAS-Acquire on `key`).
    /// 3. Store `seq = s + 1` (odd, "publish in progress" —
    ///    synchronizes-with any reader who later observes any of
    ///    our subsequent writes).
    /// 4. Store new `fn_ptr`.
    /// 5. Store `seq = s + 2` (even, "publish complete").
    /// 6. Store new clean `key` — releases the lock.
    #[inline]
    pub fn publish(&self, key: u64, fn_ptr: *const ()) {
        debug_assert!(
            key & PUBLISHING_BIT == 0,
            "publish: caller's key sets reserved bit 63",
        );

        // Spin-CAS-acquire: set PUBLISHING_BIT.
        loop {
            let cur = self.key.load(Ordering::Relaxed);
            if cur & PUBLISHING_BIT != 0 {
                core::hint::spin_loop();
                continue;
            }
            if self.key.compare_exchange_weak(
                cur,
                cur | PUBLISHING_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                break;
            }
            core::hint::spin_loop();
        }

        // We hold the lock. The CAS-Acquire on key synchronized-
        // with the prior writer's lock-release key.store, which is
        // program-order AFTER their last seq.store(even) — so we
        // see the latest committed seq via a plain Relaxed load.
        let s = self.seq.load(Ordering::Relaxed);
        debug_assert!(s & 1 == 0, "seq odd while we hold the lock");

        self.seq.store(s.wrapping_add(1), Ordering::Release); // odd: in progress
        self.fn_ptr.store(fn_ptr as *mut _, Ordering::Release);
        self.seq.store(s.wrapping_add(2), Ordering::Release); // even: complete

        // Release the lock LAST. Any next writer's CAS-Acquire here
        // synchronizes-with all of our prior stores in program order.
        self.key.store(key, Ordering::Release);
    }

    /// Fast-path read. Returns `Some(fn_ptr)` only if the slot was
    /// in a stable state matching `want` for the entire read window.
    ///
    /// Standard seqlock pattern: read seq, read data, re-read seq,
    /// check unchanged + even.
    ///
    /// `seq` is even in stable state, odd while a publish is in
    /// progress. Two even values that differ → a publish completed
    /// in our window → torn. Same even value → stable. Odd at any
    /// observation → mid-publish.
    #[inline(always)]
    pub fn read(&self, want: u64) -> Option<MethodFn> {
        let seq1 = self.seq.load(Ordering::Acquire);
        if seq1 & 1 != 0 { return None; }
        let k    = self.key.load(Ordering::Acquire);
        if k != want { return None; }
        let f    = self.fn_ptr.load(Ordering::Acquire);
        let seq2 = self.seq.load(Ordering::Acquire);
        if seq1 == seq2 {
            // SAFETY: on x86-64/aarch64 (the only supported targets),
            // `*const ()` and `unsafe extern "C" fn` are the same width
            // and ABI-compatible. Cranelift hardening tracked in
            // doc/deferred-work.md.
            Some(unsafe { core::mem::transmute::<*const (), MethodFn>(f as *const ()) })
        } else { None }
    }
}

/// Compute the IC `want` key for a given Value + ProtocolMethod.
#[inline(always)]
pub fn want_key(value: Value, method: &ProtocolMethod) -> u64 {
    ICSlot::make_key(value.tag, method.version.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn fake(_: *const Value, _: usize) -> Value { Value::NIL }
    fn fp() -> *const () { fake as *const () }

    #[test]
    fn empty_ic_misses() {
        let ic = ICSlot::EMPTY;
        assert!(ic.read(ICSlot::make_key(16, 1)).is_none());
    }

    #[test]
    fn publish_then_hit() {
        let ic = ICSlot::EMPTY;
        let key = ICSlot::make_key(16, 1);
        ic.publish(key, fp());
        assert!(ic.read(key).is_some());
        assert!(ic.read(ICSlot::make_key(17, 1)).is_none());
        assert!(ic.read(ICSlot::make_key(16, 2)).is_none());
    }

    #[test]
    fn republish_overwrites_cleanly() {
        let ic = ICSlot::EMPTY;
        let key1 = ICSlot::make_key(16, 1);
        let key2 = ICSlot::make_key(17, 1);
        ic.publish(key1, fp());
        ic.publish(key2, fp());
        assert!(ic.read(key1).is_none());
        assert!(ic.read(key2).is_some());
    }

    #[test]
    fn seq_advances_each_publish() {
        let ic = ICSlot::EMPTY;
        let key = ICSlot::make_key(16, 1);
        let s0 = ic.seq.load(Ordering::Acquire);
        ic.publish(key, fp());
        let s1 = ic.seq.load(Ordering::Acquire);
        ic.publish(key, fp());
        let s2 = ic.seq.load(Ordering::Acquire);
        assert!(s1 > s0 && s2 > s1);
    }
}
