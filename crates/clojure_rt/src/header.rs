//! Heap object header. 16 bytes, aligned 16. Body data follows.

use core::sync::atomic::AtomicI32;

use crate::value::TypeId;

#[repr(C, align(16))]
pub struct Header {
    pub type_id: TypeId,
    pub flags:   u32,
    pub rc:      AtomicI32,
    pub _pad:    u32,
}

impl Header {
    /// Initial state for a freshly allocated heap object: biased mode, count=1.
    pub const INITIAL_RC: i32 = -1;
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
