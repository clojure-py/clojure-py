//! OS-level block allocation. mmap on Linux (with madvise(DONTNEED) for
//! return-to-OS); std::alloc with explicit alignment as the portable
//! fallback.

use core::alloc::Layout;
use core::ptr::NonNull;

use crate::gc::rcimmix::block::{Block, BlockHeader};
use crate::gc::rcimmix::{BLOCK_ALIGN, BLOCK_SIZE, SLAB_BATCH};

/// Allocate a slab of `SLAB_BATCH` consecutive 32 KB blocks. Returns
/// pointers to each block. Panics on OOM.
/// # Safety
/// The returned blocks are zero-initialized with BlockHeader::init_empty
/// called on each. The caller is responsible for managing ownership and
/// eventual deallocation of these blocks via the pool APIs.
pub unsafe fn alloc_slab() -> [NonNull<Block>; SLAB_BATCH] {
    let total_size = BLOCK_SIZE * SLAB_BATCH;
    let layout = Layout::from_size_align(total_size, BLOCK_ALIGN)
        .expect("RCImmix slab layout invalid");
    let raw = unsafe { std::alloc::alloc_zeroed(layout) };
    if raw.is_null() {
        panic!("clojure_rt: OOM allocating RCImmix slab ({} bytes)", total_size);
    }
    let mut blocks: [Option<NonNull<Block>>; SLAB_BATCH] = [None; SLAB_BATCH];
    for (i, slot) in blocks.iter_mut().enumerate() {
        let block_ptr = unsafe { raw.add(i * BLOCK_SIZE) } as *mut Block;
        unsafe { BlockHeader::init_empty(block_ptr); }
        *slot = Some(unsafe { NonNull::new_unchecked(block_ptr) });
    }
    blocks.map(|opt| opt.unwrap())
}

/// Release a single block back to the OS. Currently always a no-op.
///
/// SLAB-RELEASE-LIMITATION: blocks are allocated 8 at a time; we
/// currently can't return a single block to the OS without freeing the
/// whole slab. v1 simply leaks blocks past the empty_pool cap until
/// process exit. Future work: track slab origin and reference-count
/// blocks within their slab; only `dealloc` when all 8 are recyclable.
/// # Safety
/// This function is currently a no-op and safe to call at any time.
/// In the future, the caller must ensure the block is no longer in use
/// before calling this function.
pub unsafe fn release_block(_block: NonNull<Block>) {
    // Intentionally a no-op in v1. See SLAB-RELEASE-LIMITATION above.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_slab_returns_block_aligned_pointers() {
        unsafe {
            let blocks = alloc_slab();
            for block in &blocks {
                let addr = block.as_ptr() as usize;
                assert_eq!(addr & (BLOCK_ALIGN - 1), 0, "block not BLOCK_ALIGN-aligned");
            }
            // BlockHeader::init_empty was called; verify bump_ptr was set.
            for block in &blocks {
                let header = &block.as_ref().header;
                assert_eq!(header.bump_ptr.get(), crate::gc::rcimmix::BUMP_START as u32);
                assert_eq!(header.bump_end.get(), BLOCK_SIZE as u32);
            }
            // Note: we can't dealloc individual blocks (see SLAB-RELEASE-LIMITATION).
            // Test slab leaks; that's fine for this test.
        }
    }
}
