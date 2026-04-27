//! Block layout. A `Block` is a 32 KB-aligned 32 KB region with a
//! `BlockHeader` at offset 0 (rounded up to 384 B = 3 reserved lines)
//! and an object area filling the rest.

use core::cell::Cell;
use core::sync::atomic::{AtomicPtr, AtomicU64};

use crate::gc::rcimmix::{BLOCK_SIZE, BLOCK_SIZE as _BS, BUMP_START, LINES_PER_BLOCK, LINE_SIZE};
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

/// Compute the block address from any pointer interior to it.
#[inline(always)]
pub fn block_of<T>(ptr: *const T) -> *mut Block {
    ((ptr as usize) & !(_BS - 1)) as *mut Block
}

/// Return the inclusive line index range `[line_start, line_end]` that
/// the byte range `[byte_start, byte_end_exclusive)` spans within a block.
#[inline]
pub fn line_range(byte_start: u32, byte_end_exclusive: u32) -> (usize, usize) {
    debug_assert!(byte_end_exclusive > byte_start);
    let line_start = (byte_start as usize) / LINE_SIZE;
    let line_end = ((byte_end_exclusive as usize) - 1) / LINE_SIZE;
    (line_start, line_end)
}

/// Increment line counts for the spanned range. Saturates at u8::MAX
/// to avoid overflow panic in release; debug-asserts no saturation
/// occurs in well-formed programs (max counter value is bounded by
/// objects-per-line, far below 255).
#[inline]
pub unsafe fn inc_line_counts(header: &BlockHeader, byte_start: u32, byte_end_exclusive: u32) {
    let (l0, l1) = line_range(byte_start, byte_end_exclusive);
    for line in l0..=l1 {
        let cell = &header.line_counts[line];
        let v = cell.get();
        debug_assert!(v < u8::MAX, "line count overflow at line {}", line);
        cell.set(v.saturating_add(1));
    }
}

/// Decrement line counts for the spanned range. Debug-asserts that no
/// underflow occurs (would indicate a double-free or accounting bug).
#[inline]
pub unsafe fn dec_line_counts(header: &BlockHeader, byte_start: u32, byte_end_exclusive: u32) {
    let (l0, l1) = line_range(byte_start, byte_end_exclusive);
    for line in l0..=l1 {
        let cell = &header.line_counts[line];
        let v = cell.get();
        debug_assert!(v > 0, "line count underflow at line {} (double free?)", line);
        cell.set(v.saturating_sub(1));
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

    #[test]
    fn line_range_single_line() {
        // Object [0..16) is in line 0.
        assert_eq!(line_range(0, 16), (0, 0));
        // Object [16..128) is in line 0 (exclusive end at line boundary).
        assert_eq!(line_range(16, 128), (0, 0));
        // Object [0..128) is in line 0 (exclusive end at line boundary).
        assert_eq!(line_range(0, 128), (0, 0));
    }

    #[test]
    fn line_range_two_lines() {
        // Object [0..129) spans lines 0 and 1.
        assert_eq!(line_range(0, 129), (0, 1));
        // Object [127..128) is in line 0 only.
        assert_eq!(line_range(127, 128), (0, 0));
        // Object [127..129) spans lines 0 and 1.
        assert_eq!(line_range(127, 129), (0, 1));
    }

    #[test]
    fn line_range_many_lines() {
        // Object spanning lines 5-7 inclusive: [5*128 .. 7*128 + 1) = [640, 897).
        assert_eq!(line_range(640, 897), (5, 7));
    }

    #[test]
    fn block_of_recovers_block_address() {
        // Construct a fake block address (must be BLOCK_ALIGN-aligned).
        let block_addr: usize = 0x1_0000_0000; // 4 GB, BLOCK_ALIGN-aligned
        let interior = block_addr + 384 + 47;
        assert_eq!(block_of(interior as *const u8) as usize, block_addr);
    }
}
