//! Runtime-internal monotonic thread id. Cached in TLS on first read.
//! 0 is reserved as the "unowned" sentinel.

use core::sync::atomic::{AtomicU64, Ordering};

static THREAD_COUNTER: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static MY_TID: u64 = THREAD_COUNTER.fetch_add(1, Ordering::Relaxed);
}

/// Return this thread's runtime-internal id. First call on a given
/// thread allocates a fresh id; subsequent calls return the cached value.
#[inline]
pub fn current_tid() -> u64 {
    MY_TID.with(|t| *t)
}

/// Owner-tid value to stamp into a freshly-allocated `Header`. In
/// debug builds this is the low 32 bits of `current_tid()` so that
/// `rc::{dup,drop}_heap` can debug-assert biased-mode mutations come
/// from the owner thread. In release builds this collapses to a
/// constant `0` — no TLS access, no overhead. Collisions across the
/// 32-bit truncation are negligible (would require 4B distinct
/// threads in one process).
#[inline]
pub fn current_owner_tid() -> u32 {
    if cfg!(debug_assertions) {
        current_tid() as u32
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn tid_is_monotonic_within_a_thread() {
        let a = current_tid();
        let b = current_tid();
        assert_eq!(a, b, "same thread must return same tid across calls");
        assert!(a >= 1, "tid 0 is reserved as the unowned sentinel");
    }

    #[test]
    fn distinct_threads_get_distinct_tids() {
        let main_tid = current_tid();
        let other_tid = thread::spawn(current_tid).join().unwrap();
        assert_ne!(main_tid, other_tid);
    }
}
