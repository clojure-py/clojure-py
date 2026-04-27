//! Block layout. A `Block` is a 32 KB-aligned 32 KB region with a
//! `BlockHeader` at offset 0 (rounded up to 384 B = 3 reserved lines)
//! and an object area filling the rest.

use core::cell::Cell;
use core::sync::atomic::{AtomicPtr, AtomicU64};

use crate::gc::rcimmix::{BLOCK_SIZE, BUMP_START, LINES_PER_BLOCK};
use crate::header::Header;

/// Per-block bookkeeping. Sits in-band at the start of every block.
#[repr(C, align(16))]
pub struct BlockHeader {
    /// Current owning thread, encoded as a runtime-internal monotonic
    /// thread id (see `tid::current_tid`). 0 means "unowned / in pool".
    pub owner_tid: AtomicU64,

    /// Per-line live-object count. u8 is plenty (max ~8 minimum-sized
    /// objects per line). Mutated only by the owning thread (no atomics).
    /// Lines [0..RESERVED_LINES] always read 0; the bump pointer never
    /// places objects there.
    pub line_counts: [Cell<u8>; LINES_PER_BLOCK],

    /// Current bump pointer (byte offset within block). Owner-only.
    pub bump_ptr: Cell<u32>,
    /// Exclusive end of current hole. Owner-only.
    pub bump_end: Cell<u32>,

    /// Head of intrusive remote-free list. Cross-thread frees CAS-prepend
    /// here; owner atomically swaps to null during drain.
    pub remote_free_head: AtomicPtr<Header>,

    /// Pool linkage. Touched only under `partial_pool` / `empty_pool` mutex.
    pub next_in_pool: Cell<*mut Block>,
}

// SAFETY: AtomicU64/AtomicPtr are Sync by construction. The `Cell<_>`
// fields are owner-only — the safety argument lives in the docs (see
// pool.rs, tlab.rs ownership rules); manual Sync is required because
// `Cell` is not auto-Sync.
unsafe impl Sync for BlockHeader {}

/// A whole block. The `BlockHeader` is at the start; the rest is
/// object area. The total size is exactly `BLOCK_SIZE`.
///
/// We use a transparent wrapper rather than `[u8; BLOCK_SIZE]` so that
/// `*mut Block` is a stable type for ownership transfer.
#[repr(C, align(32768))]
pub struct Block {
    pub header: BlockHeader,
    /// Padding + object area. Never read directly through this field;
    /// allocations are computed via `bump_ptr` offsets from the block
    /// base.
    pub _body: [u8; BLOCK_SIZE - core::mem::size_of::<BlockHeader>()],
}

impl BlockHeader {
    /// Initialize a freshly-mmap'd block to the unowned-empty state.
    /// Caller must ensure the memory was zeroed (mmap MAP_ANONYMOUS does this).
    pub unsafe fn init_empty(_block: *mut Block) {
        // Zero-init from mmap suffices; all atomic/cell fields default
        // to 0/null which matches the unowned-empty state. Bump fields:
        let header = unsafe { &(*_block).header };
        header.bump_ptr.set(BUMP_START as u32);
        header.bump_end.set(BLOCK_SIZE as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gc::rcimmix::BLOCK_ALIGN;

    #[test]
    fn block_layout() {
        assert_eq!(size_of::<Block>(), BLOCK_SIZE);
        assert_eq!(align_of::<Block>(), BLOCK_ALIGN);
        // BlockHeader must fit within RESERVED_LINES * LINE_SIZE bytes.
        assert!(size_of::<BlockHeader>() <= BUMP_START);
    }

    #[test]
    fn const_pad_field_size_is_positive() {
        // Ensures BlockHeader didn't accidentally grow to consume the
        // entire block.
        const _: () = assert!(BLOCK_SIZE > size_of::<BlockHeader>());
    }
}
