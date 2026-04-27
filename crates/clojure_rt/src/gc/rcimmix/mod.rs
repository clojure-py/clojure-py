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
use crate::gc::rcimmix::block::{Block, inc_line_counts, dec_line_counts, find_next_hole, block_of};
use crate::gc::GcAllocator;

/// Round `x` up to a multiple of `align` (where `align` is a power of two).
#[inline(always)]
fn align_up(x: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (x + align - 1) & !(align - 1)
}

/// Compute the total in-block size for an object: HEADER_SIZE + body_size,
/// where body_size is rounded up to body_align (which must be ≤ 16, the
/// Header alignment).
///
/// Body size is forced to a minimum of 8 bytes because `dealloc_remote`
/// repurposes body bytes 0..8 as a `next: *mut Header` pointer in the
/// remote-free chain. Bodies smaller than 8 would clobber adjacent memory.
#[inline(always)]
fn total_size(body_layout: Layout) -> u32 {
    let body = body_layout.size().max(8); // see dealloc_remote: body bytes 0..8 repurposed for remote-free `next` pointer
    (HEADER_SIZE + body) as u32
}

/// Owner-thread alloc fast path. Returns null if the current hole is
/// too small (caller falls through to slow path).
///
/// # Safety
/// Caller must hold ownership of `block` (i.e. `block.header.owner_tid ==
/// current_tid()`). `body_layout.align()` must be ≤ 16.
#[inline(always)]
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
///
/// # Safety
/// Caller must hold ownership of `block` (i.e. `block.header.owner_tid ==
/// current_tid()`) and must have just observed `alloc_fast` returning null
/// on the same block. `body_layout.size()` must be ≤ `LARGE_THRESHOLD`.
#[cold]
#[inline(never)]
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

    // 3. Block exhausted: try to find a block that fits the request.
    //
    //    Strategy: first try ONE partial_pool block (draining its remote
    //    frees may open up holes). If it still can't fit, push it back to
    //    partial_pool (it belongs there — it has live objects) and then
    //    get a guaranteed-empty block from empty_pool or a fresh mmap slab.
    //
    //    We must NOT loop back to partial_pool after a failure because
    //    partial_pool is LIFO: we'd immediately re-pop the same block and
    //    spin forever. empty_pool / fresh blocks always have enough space
    //    for requests ≤ LARGE_THRESHOLD (asserted below).
    //
    //    Note: the dispatcher gates on body_layout.size() <= LARGE_THRESHOLD,
    //    not on total. For body sizes in (LARGE_THRESHOLD - HEADER_SIZE,
    //    LARGE_THRESHOLD], total > LARGE_THRESHOLD while the body is still
    //    within the RCImmix threshold. Assert on body size, not total.
    debug_assert!(body_layout.size() <= LARGE_THRESHOLD,
        "alloc_slow reached with body_size={} > LARGE_THRESHOLD={}; should have taken large path",
        body_layout.size(), LARGE_THRESHOLD);

    // 3a. Try a partial_pool block first (drain may reclaim space).
    if let Some(partial) = unsafe { crate::gc::rcimmix::pool::partial_pool().lock().pop() } {
        let partial_header = unsafe { &partial.as_ref().header };
        let tid = crate::gc::rcimmix::tid::current_tid();
        let prev = partial_header.owner_tid.swap(tid, core::sync::atomic::Ordering::AcqRel);
        debug_assert_eq!(prev, 0, "partial_pool block must be unowned");

        // Drain pending remote frees — may open holes in this block.
        unsafe { crate::gc::rcimmix::drain::drain_remote_frees(partial); }

        if let Some((start, end)) = find_next_hole(partial_header, BUMP_START as u32, total) {
            partial_header.bump_ptr.set(start);
            partial_header.bump_end.set(end);
            unsafe { crate::gc::rcimmix::tlab::replace_tlab(partial); }
            let h = unsafe { alloc_fast(partial, body_layout, type_id) };
            // alloc_fast cannot fail after find_next_hole returned a sufficient
            // hole. If it somehow does, that is a bug — catch it with the assert
            // rather than silently falling through (which would corrupt pool state
            // because replace_tlab has already transferred ownership to TLAB).
            debug_assert!(!h.is_null(),
                "alloc_fast must succeed after find_next_hole returned a sufficient hole");
            return h;
        }

        // find_next_hole found nothing: drain didn't free enough space.
        // Relinquish ownership and push back to partial_pool (live objects
        // remain). Do NOT retry from partial_pool — LIFO would give us the
        // same block again and spin.
        partial_header.owner_tid.store(0, core::sync::atomic::Ordering::Release);
        unsafe { crate::gc::rcimmix::pool::partial_pool().lock().push(partial); }
    }

    // 3b. Get a guaranteed-space block: empty_pool or fresh mmap.
    //     These blocks have no live objects so find_next_hole must succeed.
    let fresh = unsafe { crate::gc::rcimmix::pool::acquire_empty_or_fresh() };
    let fresh_header = unsafe { &fresh.as_ref().header };
    let tid = crate::gc::rcimmix::tid::current_tid();
    let prev = fresh_header.owner_tid.swap(tid, core::sync::atomic::Ordering::AcqRel);
    debug_assert_eq!(prev, 0, "empty/fresh block must be unowned");

    // Even a fresh block may have remote frees queued (unlikely but safe
    // to drain). Then scan the full block.
    unsafe { crate::gc::rcimmix::drain::drain_remote_frees(fresh); }

    if let Some((start, end)) = find_next_hole(fresh_header, BUMP_START as u32, total) {
        fresh_header.bump_ptr.set(start);
        fresh_header.bump_end.set(end);
        unsafe { crate::gc::rcimmix::tlab::replace_tlab(fresh); }
        let h = unsafe { alloc_fast(fresh, body_layout, type_id) };
        debug_assert!(!h.is_null(),
            "alloc_fast must succeed after find_next_hole returned a sufficient hole");
        return h;
    }
    // Should be unreachable: a fully-empty block always fits total ≤ LARGE_THRESHOLD.
    panic!(
        "clojure_rt: RCImmix can't fit object of body_layout {:?} in a fresh block \
         (BLOCK_SIZE={}, total={})",
        body_layout, BLOCK_SIZE, total
    );
}

/// Owner-thread dealloc: decrement line counts for the spanned range.
/// Caller has already verified that this thread is the owner.
///
/// # Safety
/// Caller must hold ownership of `block` (i.e. `block.header.owner_tid ==
/// current_tid()`). The destructor for the object at `ptr` must have
/// already run before this is called.
#[inline(always)]
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
///
/// # Safety
/// `ptr` must point to a heap-allocated object whose destructor has already
/// run (body bytes may be overwritten). Body size must be ≥ 8 bytes, which
/// is enforced by `total_size` in `alloc_fast` (bodies are padded to a
/// minimum of 8). `block` must be the block containing `ptr` (use
/// `block_of(ptr)` to recover it).
#[cold]
#[inline(never)]
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

/// The RCImmix allocator. Single global instance accessed via `RCIMMIX`.
pub struct RCImmixAllocator;

/// The single global RCImmix allocator instance, suitable for passing
/// to `gc::install_allocator`.
pub static RCIMMIX: RCImmixAllocator = RCImmixAllocator;

impl RCImmixAllocator {
    /// Concrete inline-able alloc. Macro-generated `Foo::alloc(...)`
    /// constructors call this directly to bypass the `dyn GcAllocator`
    /// vtable, letting LLVM inline through to the bump-pointer hot path.
    ///
    /// # Safety
    /// Same contract as `<Self as GcAllocator>::alloc`.
    #[inline(always)]
    pub unsafe fn alloc_inline(&self, body_layout: Layout, type_id: TypeId) -> *mut Header {
        // Large or over-aligned objects go through std::alloc.
        // Any body alignment > 16 also takes this path; the line-and-block
        // heap supports body alignment ≤ 16 (i.e. ≤ Header alignment).
        if body_layout.size() > LARGE_THRESHOLD || body_layout.align() > 16 {
            return unsafe { large::alloc_large(body_layout, type_id) };
        }

        // Owner alloc: ensure TLAB, fast path, fall through to slow path.
        let block = unsafe { crate::gc::rcimmix::tlab::ensure_tlab() };
        let h = unsafe { alloc_fast(block, body_layout, type_id) };
        if !h.is_null() {
            return h;
        }
        unsafe { alloc_slow(block, body_layout, type_id) }
    }

    /// Concrete inline-able dealloc. Called directly from
    /// `rc::destruct_and_dealloc` for the same reason as `alloc_inline`.
    ///
    /// # Safety
    /// Same contract as `<Self as GcAllocator>::dealloc`.
    #[inline(always)]
    pub unsafe fn dealloc_inline(&self, ptr: *mut Header, body_layout: Layout) {
        // Large or over-aligned objects took the std::alloc path on alloc.
        if body_layout.size() > LARGE_THRESHOLD || body_layout.align() > 16 {
            let was_large = unsafe { large::try_dealloc_large(ptr) };
            debug_assert!(was_large, "large body_layout but pointer not in LARGE_OBJECTS");
            return;
        }

        // RCImmix path: identify owner via block.owner_tid.
        let block = block_of(ptr);
        let block_nn = NonNull::new(block).expect("dealloc on null pointer");
        let header = unsafe { &block_nn.as_ref().header };
        let owner = header.owner_tid.load(core::sync::atomic::Ordering::Acquire);
        if owner == crate::gc::rcimmix::tid::current_tid() {
            unsafe { dealloc_owner(block_nn, ptr, body_layout); }
        } else {
            unsafe { dealloc_remote(block_nn, ptr); }
        }
    }
}

unsafe impl GcAllocator for RCImmixAllocator {
    unsafe fn alloc(&self, body_layout: Layout, type_id: TypeId) -> *mut Header {
        unsafe { self.alloc_inline(body_layout, type_id) }
    }

    unsafe fn dealloc(&self, ptr: *mut Header, body_layout: Layout) {
        unsafe { self.dealloc_inline(ptr, body_layout) }
    }
}
