//! Heap object header. 16 bytes, aligned 16. Body data follows.

use core::sync::atomic::AtomicI32;

use crate::value::TypeId;

#[repr(C, align(16))]
pub struct Header {
    pub type_id: TypeId,
    pub flags:   u32,
    pub rc:      AtomicI32,
    /// Owner thread (low 32 bits of `tid::current_tid()`), set at
    /// alloc time, cleared by `rc::share_heap` when the rc flips to
    /// shared mode. Read **only** by debug-build assertions in
    /// `rc::{dup,drop}_heap` to catch missing `rc::share()` calls
    /// before cross-thread publication. Release builds never read or
    /// write this field after init (the alloc-time write is
    /// constant-folded to 0).
    pub owner_tid: u32,
}

impl Header {
    /// Initial state for a freshly allocated heap object: biased mode, count=1.
    pub const INITIAL_RC: i32 = -1;
    /// Sentinel for "no owner" — set by `share_heap` when biased→shared.
    pub const UNOWNED_TID: u32 = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_layout() {
        assert_eq!(size_of::<Header>(),  16);
        assert_eq!(align_of::<Header>(), 16);
    }
}
