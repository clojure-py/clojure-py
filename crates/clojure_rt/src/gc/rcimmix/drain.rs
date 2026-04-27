//! Remote-free drain. Owner thread atomically swaps the head of the
//! remote-free list to null, then walks the chain decrementing line
//! counts for each freed object.

use core::ptr::NonNull;
use core::sync::atomic::Ordering;

use crate::gc::rcimmix::block::{Block, dec_line_counts};
use crate::gc::rcimmix::HEADER_SIZE;
use crate::header::Header;
use crate::type_registry;

/// Drain remote frees on this block. Called by the owner during slow
/// path; safe to call repeatedly.
/// # Safety
/// Must be called only by the owning thread of the block. The block's
/// remote_free_head will be atomically swapped to null and the chain
/// walked to decrement line counts. Subsequent remote frees to this
/// block will prepend to the new null head (safe concurrent operation).
pub unsafe fn drain_remote_frees(block: NonNull<Block>) {
    let header = unsafe { &block.as_ref().header };
    let mut head = header.remote_free_head.swap(core::ptr::null_mut(), Ordering::AcqRel);
    let block_addr = block.as_ptr() as usize;
    while !head.is_null() {
        // Read `next` pointer from body bytes 0..8 (stored by remote
        // dealloc; see mod.rs::dealloc_remote).
        let body = (head as *mut u8).wrapping_add(HEADER_SIZE) as *mut *mut Header;
        let next = unsafe { body.read() };

        // Recover body_layout from the type registry to compute spanned lines.
        let type_id = unsafe { (*head).type_id };
        let meta = type_registry::get(type_id);

        let offset = (head as usize - block_addr) as u32;
        let total = (HEADER_SIZE + meta.layout.size()) as u32;
        unsafe { dec_line_counts(header, offset, offset + total); }

        head = next;
    }
}
