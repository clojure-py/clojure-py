//! Remote-free drain. Stub for now; implemented in Task 13.

use core::ptr::NonNull;

use crate::gc::rcimmix::block::Block;

/// Drain remote frees on this block (decrement line counts for each
/// remote-freed object). Stub — real impl in Task 13.
#[allow(dead_code)]
pub unsafe fn drain_remote_frees(_block: NonNull<Block>) {
    // No-op. Will be implemented when remote dealloc lands (T12+T13).
}
