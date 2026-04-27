//! Global pools of unowned blocks. `partial_pool` holds blocks with
//! some live objects still in them; `empty_pool` holds fully-empty
//! blocks (capped). Both are LIFO singly-linked via
//! `BlockHeader::next_in_pool`.

use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::gc::rcimmix::block::Block;
use crate::gc::rcimmix::EMPTY_POOL_CAP;
use crate::gc::rcimmix::os;

/// LIFO singly-linked list of unowned blocks.
pub struct Pool {
    head: Option<NonNull<Block>>,
    len: usize,
}

// SAFETY: Pool's pointer is to a Block which is Sync (BlockHeader is
// manually Sync); the Pool is always accessed under a Mutex.
unsafe impl Send for Pool {}

impl Pool {
    pub const fn new() -> Self {
        Self { head: None, len: 0 }
    }

    /// Push a block onto the head. Caller must have already CAS'd
    /// `owner_tid` to 0 (unowned).
    pub unsafe fn push(&mut self, block: NonNull<Block>) {
        let header = unsafe { &block.as_ref().header };
        debug_assert_eq!(header.owner_tid.load(Ordering::Relaxed), 0);
        header.next_in_pool.set(self.head.map(|h| h.as_ptr()).unwrap_or(core::ptr::null_mut()));
        self.head = Some(block);
        self.len += 1;
    }

    /// Pop the head block. Returns `None` if the pool is empty.
    pub unsafe fn pop(&mut self) -> Option<NonNull<Block>> {
        let block = self.head?;
        let header = unsafe { &block.as_ref().header };
        let next = header.next_in_pool.get();
        self.head = NonNull::new(next);
        self.len -= 1;
        Some(block)
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

static PARTIAL_POOL: OnceLock<Mutex<Pool>> = OnceLock::new();
static EMPTY_POOL: OnceLock<Mutex<Pool>> = OnceLock::new();

pub fn partial_pool() -> &'static Mutex<Pool> {
    PARTIAL_POOL.get_or_init(|| Mutex::new(Pool::new()))
}

pub fn empty_pool() -> &'static Mutex<Pool> {
    EMPTY_POOL.get_or_init(|| Mutex::new(Pool::new()))
}

/// Acquire an unowned block: prefer partial_pool, then empty_pool, then
/// alloc a fresh slab. The returned block has `owner_tid == 0`; the
/// caller is responsible for CAS'ing it to its own tid.
pub unsafe fn acquire_block() -> NonNull<Block> {
    if let Some(block) = unsafe { partial_pool().lock().pop() } {
        return block;
    }
    if let Some(block) = unsafe { empty_pool().lock().pop() } {
        return block;
    }
    // Slab path: alloc 8, keep 1, push 7 into empty_pool.
    let blocks = unsafe { os::alloc_slab() };
    let mut empty = empty_pool().lock();
    for &block in &blocks[1..] {
        if empty.len() < EMPTY_POOL_CAP {
            unsafe { empty.push(block); }
        } else {
            unsafe { os::release_block(block); }
        }
    }
    blocks[0]
}

/// Release a block back to the partial_pool (called when owner has live
/// objects in it but is moving on).
pub unsafe fn release_partial(block: NonNull<Block>) {
    let header = unsafe { &block.as_ref().header };
    header.owner_tid.store(0, Ordering::Release);
    unsafe { partial_pool().lock().push(block); }
}

/// Release a fully-empty block. If empty_pool is full, returns to OS.
pub unsafe fn release_empty(block: NonNull<Block>) {
    let header = unsafe { &block.as_ref().header };
    header.owner_tid.store(0, Ordering::Release);
    let mut empty = empty_pool().lock();
    if empty.len() < EMPTY_POOL_CAP {
        unsafe { empty.push(block); }
    } else {
        drop(empty);
        unsafe { os::release_block(block); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_returns_unowned_block() {
        unsafe {
            let block = acquire_block();
            let header = &block.as_ref().header;
            assert_eq!(header.owner_tid.load(Ordering::Relaxed), 0);
            assert_eq!(header.bump_ptr.get(), crate::gc::rcimmix::BUMP_START as u32);
            // Don't release; test leaks intentionally (slab can't be
            // returned anyway per SLAB-RELEASE-LIMITATION).
        }
    }

    #[test]
    fn release_partial_then_acquire_returns_same_block() {
        unsafe {
            let block_a = acquire_block();
            // Mark as owned, then release.
            (&block_a.as_ref().header).owner_tid.store(42, Ordering::Relaxed);
            release_partial(block_a);

            let block_b = acquire_block();
            // partial_pool returns LIFO, so we get block_a back first.
            assert_eq!(block_a.as_ptr(), block_b.as_ptr());
            assert_eq!((&block_b.as_ref().header).owner_tid.load(Ordering::Relaxed), 0);
        }
    }
}
