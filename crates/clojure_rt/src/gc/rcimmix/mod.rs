//! RCImmix allocator — thread-local-bump on 32 KB blocks with 128 B lines,
//! intrusive cross-thread free lists, and large-object fall-through.
//!
//! See `doc/rcimmix-allocator.md` for the design summary and
//! `docs/superpowers/specs/2026-04-27-rcimmix-allocator-foundation-design.md`
//! for the full spec (gitignored, local).

pub mod block;
pub mod drain;
pub mod large;
pub mod os;
pub mod pool;
pub mod tid;
pub mod tlab;

/// Block size in bytes. 32 KB = 8 OS pages on Linux.
pub const BLOCK_SIZE: usize = 32 * 1024;

/// Block alignment in bytes. Equal to BLOCK_SIZE so that
/// `ptr & !(BLOCK_SIZE - 1)` recovers the block address.
pub const BLOCK_ALIGN: usize = BLOCK_SIZE;

/// Line size in bytes. 128 B = 2 cache lines on x86-64.
pub const LINE_SIZE: usize = 128;

/// Number of lines per block.
pub const LINES_PER_BLOCK: usize = BLOCK_SIZE / LINE_SIZE;

/// Lines reserved at the start of the block for `BlockHeader`.
pub const RESERVED_LINES: usize = 3;

/// First byte offset within a block where allocation may begin.
pub const BUMP_START: usize = RESERVED_LINES * LINE_SIZE;

/// Heap object header size (matches `crate::header::Header`).
pub const HEADER_SIZE: usize = 16;

/// Object body sizes above this threshold go through the large-object path.
/// 8 KB = quarter-block.
pub const LARGE_THRESHOLD: usize = 8 * 1024;

/// Maximum blocks held in `empty_pool` before excess is returned to OS.
pub const EMPTY_POOL_CAP: usize = 16;

/// Slab batch size for fresh OS allocation. 8 × 32 KB = 256 KB per syscall.
pub const SLAB_BATCH: usize = 8;
