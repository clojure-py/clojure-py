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

use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::AtomicI32;

use crate::header::Header;
use crate::value::TypeId;
use crate::gc::rcimmix::block::{Block, inc_line_counts, dec_line_counts, find_next_hole};

/// Round `x` up to a multiple of `align` (where `align` is a power of two).
#[inline(always)]
#[allow(dead_code)]
fn align_up(x: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (x + align - 1) & !(align - 1)
}

/// Compute the total in-block size for an object: HEADER_SIZE + body_size,
/// where body_size is rounded up to body_align (which must be ≤ 16, the
/// Header alignment).
#[inline(always)]
#[allow(dead_code)]
fn total_size(body_layout: Layout) -> u32 {
    let body = body_layout.size();
    (HEADER_SIZE + body) as u32
}

/// Owner-thread alloc fast path. Returns null if the current hole is
/// too small (caller falls through to slow path).
#[inline]
#[allow(dead_code)]
unsafe fn alloc_fast(block: NonNull<Block>, body_layout: Layout, type_id: TypeId) -> *mut Header {
    let header = unsafe { &block.as_ref().header };
    let bump = header.bump_ptr.get();
    // Header alignment is 16; body alignment must be ≤ 16 (asserted on
    // the slow path). Header sits at offset `aligned`; body follows.
    let aligned = align_up(bump, 16);
    let total = total_size(body_layout);
    let new_bump = aligned + total;
    if new_bump > header.bump_end.get() {
        return core::ptr::null_mut();
    }
    // Compute object pointer: block_addr + aligned.
    let block_addr = block.as_ptr() as usize;
    let h = (block_addr + aligned as usize) as *mut Header;
    // Initialize Header.
    unsafe {
        core::ptr::write(h, Header {
            type_id,
            flags: 0,
            rc: AtomicI32::new(Header::INITIAL_RC),
            _pad: 0,
        });
    }
    header.bump_ptr.set(new_bump);
    // Update line counts.
    unsafe { inc_line_counts(header, aligned, new_bump); }
    h
}

/// Owner-thread alloc slow path. Drains remote frees, finds next hole
/// in the current block, and transitions to a new block if needed.
#[cold]
#[inline(never)]
#[allow(dead_code)]
unsafe fn alloc_slow(block: NonNull<Block>, body_layout: Layout, type_id: TypeId) -> *mut Header {
    debug_assert!(body_layout.align() <= 16,
        "body alignment > 16 not yet supported (large path needed)");

    let header = unsafe { &block.as_ref().header };

    // 1. Drain remote frees — may free up holes.
    unsafe { crate::gc::rcimmix::drain::drain_remote_frees(block); }

    // 2. Find next hole in this block.
    let total = total_size(body_layout) as usize;
    if let Some((start, end)) = find_next_hole(header, header.bump_end.get(), total) {
        header.bump_ptr.set(start);
        header.bump_end.set(end);
        return unsafe { alloc_fast(block, body_layout, type_id) };
    }

    // 3. Block exhausted: get a new one.
    let new_block = unsafe { crate::gc::rcimmix::pool::acquire_block() };
    let new_header = unsafe { &new_block.as_ref().header };
    let tid = crate::gc::rcimmix::tid::current_tid();
    let prev = new_header.owner_tid.swap(tid, core::sync::atomic::Ordering::AcqRel);
    debug_assert_eq!(prev, 0);
    unsafe { crate::gc::rcimmix::tlab::replace_tlab(new_block); }

    // 4. Retry alloc on the new block.
    let h = unsafe { alloc_fast(new_block, body_layout, type_id) };
    if h.is_null() {
        // The fresh block doesn't have a hole big enough? Only possible
        // if total > BLOCK_SIZE - BUMP_START. That's the >8 KB case
        // (LARGE_THRESHOLD), which should have been routed to large.rs.
        panic!("clojure_rt: RCImmix can't fit object of body_layout {:?} in a fresh block", body_layout);
    }
    h
}

/// Owner-thread dealloc: decrement line counts for the spanned range.
/// Caller has already verified that this thread is the owner.
#[inline]
#[allow(dead_code)]
unsafe fn dealloc_owner(block: NonNull<Block>, ptr: *mut Header, body_layout: Layout) {
    let header = unsafe { &block.as_ref().header };
    let block_addr = block.as_ptr() as usize;
    let offset = (ptr as usize - block_addr) as u32;
    let total = total_size(body_layout);
    unsafe { dec_line_counts(header, offset, offset + total); }
}

/// Non-owner thread dealloc: CAS-prepend onto block.remote_free_head.
/// The destructor has already run (in `rc::destruct_and_dealloc`), so
/// the body bytes are garbage. Repurpose body bytes 0..8 as a `next:
/// *mut Header` pointer.
#[cold]
#[inline(never)]
#[allow(dead_code)]
unsafe fn dealloc_remote(block: NonNull<Block>, ptr: *mut Header) {
    let header = unsafe { &block.as_ref().header };
    // body starts at HEADER_SIZE bytes after the Header pointer
    let body = (ptr as *mut u8).wrapping_add(HEADER_SIZE) as *mut *mut Header;
    loop {
        let head = header.remote_free_head.load(core::sync::atomic::Ordering::Acquire);
        unsafe { body.write(head); }
        match header.remote_free_head.compare_exchange(
            head, ptr, core::sync::atomic::Ordering::Release, core::sync::atomic::Ordering::Acquire) {
            Ok(_) => return,
            Err(_) => continue, // retry
        }
    }
}
