//! Naive `std::alloc`-backed allocator. v1 default; RCImmix replaces this
//! later behind the same `GcAllocator` trait.

use core::alloc::Layout;
use core::ptr;
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc};

use crate::gc::{full_layout, GcAllocator};
use crate::header::Header;
use crate::value::TypeId;

pub struct NaiveAllocator {
    /// Live object count, for leak detection in tests.
    pub live: AtomicUsize,
}

impl NaiveAllocator {
    pub const fn new() -> Self { Self { live: AtomicUsize::new(0) } }

    pub fn live_count(&self) -> usize { self.live.load(Ordering::Relaxed) }
}

impl Default for NaiveAllocator { fn default() -> Self { Self::new() } }

unsafe impl GcAllocator for NaiveAllocator {
    unsafe fn alloc(&self, body_layout: Layout, type_id: TypeId) -> *mut Header {
        let layout = full_layout(body_layout);
        let raw = unsafe { alloc(layout) };
        if raw.is_null() {
            panic!("clojure_rt: OOM (Layout {:?})", layout);
        }
        let h = raw as *mut Header;
        unsafe {
            ptr::write(h, Header {
                type_id,
                flags: 0,
                rc: AtomicI32::new(Header::INITIAL_RC),
                _pad: 0,
            });
        }
        self.live.fetch_add(1, Ordering::Relaxed);
        h
    }

    unsafe fn dealloc(&self, ptr: *mut Header, body_layout: Layout) {
        let layout = full_layout(body_layout);
        unsafe { dealloc(ptr as *mut u8, layout); }
        self.live.fetch_sub(1, Ordering::Relaxed);
    }
}

pub static NAIVE: NaiveAllocator = NaiveAllocator::new();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gc::install_allocator;
    use core::sync::atomic::Ordering;

    /// Tests for the allocator are run in a single dedicated test that
    /// installs `NAIVE` as the global allocator. Use a `OnceLock` guard.
    fn ensure_installed() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| install_allocator(&NAIVE));
    }

    #[test]
    fn alloc_and_dealloc_balance_live_counter() {
        ensure_installed();
        let before = NAIVE.live_count();
        let body = Layout::from_size_align(16, 8).unwrap();
        unsafe {
            let h = NAIVE.alloc(body, 16);
            assert_eq!((*h).type_id, 16);
            assert_eq!((*h).rc.load(Ordering::Relaxed), Header::INITIAL_RC);
            assert_eq!(NAIVE.live_count(), before + 1);
            NAIVE.dealloc(h, body);
            assert_eq!(NAIVE.live_count(), before);
        }
    }
}
