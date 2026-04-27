//! Per-thread allocation buffer (TLAB). Holds one block at a time.
//! Drop on thread exit returns the current block to the partial pool.

use core::cell::Cell;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use crate::gc::rcimmix::block::Block;
use crate::gc::rcimmix::pool;
use crate::gc::rcimmix::tid::current_tid;

/// Per-thread state. Wrapped in a thread_local cell so we can take it
/// out during Drop.
pub struct Tlab {
    /// Current TLAB block. None until first alloc on this thread.
    pub current: Cell<Option<NonNull<Block>>>,
}

impl Tlab {
    pub const fn new() -> Self {
        Self { current: Cell::new(None) }
    }
}

impl Default for Tlab {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Tlab {
    fn drop(&mut self) {
        if let Some(block) = self.current.get() {
            // Release block to partial_pool. Live objects in it become
            // orphans; future owners will skip occupied lines (v1
            // contract — see doc/rcimmix-allocator.md).
            unsafe { pool::release_partial(block); }
        }
    }
}

thread_local! {
    pub static TLAB: Tlab = const { Tlab::new() };
}

/// Ensure this thread has a current TLAB block. Acquires one from the
/// pools (or fresh slab) if needed, and CAS's `owner_tid` to this
/// thread's id.
/// # Safety
/// Must be called only from the thread's context (thread-local access
/// is safe). The returned block is owned by the calling thread.
#[inline]
pub unsafe fn ensure_tlab() -> NonNull<Block> {
    TLAB.with(|tlab| {
        if let Some(block) = tlab.current.get() {
            return block;
        }
        let block = unsafe { pool::acquire_block() };
        let header = unsafe { &block.as_ref().header };
        // CAS uncontested at this point: pool mutex serialized us.
        let tid = current_tid();
        let prev = header.owner_tid.swap(tid, Ordering::AcqRel);
        debug_assert_eq!(prev, 0, "acquired block must have been unowned");
        tlab.current.set(Some(block));
        block
    })
}

/// Replace the current TLAB block. The old block (if any) is released
/// to the partial pool. The caller passes the new block (already
/// owner-CAS'd).
/// # Safety
/// Must be called only from the thread's context (thread-local access
/// is safe). The `new_block` must already have been CAS'd to the calling
/// thread's owner_tid.
pub unsafe fn replace_tlab(new_block: NonNull<Block>) {
    TLAB.with(|tlab| {
        if let Some(old) = tlab.current.get() {
            unsafe { pool::release_partial(old); }
        }
        tlab.current.set(Some(new_block));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn ensure_tlab_creates_block_on_first_call() {
        unsafe {
            let block = ensure_tlab();
            let tid = current_tid();
            assert_eq!(block.as_ref().header.owner_tid.load(Ordering::Relaxed), tid);
        }
    }

    #[test]
    fn ensure_tlab_returns_same_block_on_repeated_calls() {
        unsafe {
            let a = ensure_tlab();
            let b = ensure_tlab();
            assert_eq!(a.as_ptr(), b.as_ptr());
        }
    }

    #[test]
    fn distinct_threads_get_distinct_tlabs() {
        let main_block = unsafe { ensure_tlab() };
        let main_tid = current_tid();
        let (other_block, other_tid) = thread::spawn(|| {
            unsafe { (ensure_tlab().as_ptr() as usize, current_tid()) }
        }).join().unwrap();
        assert_ne!(main_block.as_ptr() as usize, other_block);
        assert_ne!(main_tid, other_tid);
    }
}
