//! Large object hatch. Objects with body > LARGE_THRESHOLD bypass the
//! line-and-block heap entirely and go through std::alloc. The pointer
//! is recorded in LARGE_OBJECTS so dealloc knows to take this path.

use core::alloc::Layout;
use core::sync::atomic::AtomicI32;
use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::gc::full_layout;
use crate::header::Header;
use crate::value::TypeId;

#[allow(clippy::type_complexity)]
static LARGE_OBJECTS: OnceLock<Mutex<HashMap<usize, Layout>>> = OnceLock::new();

fn large_objects() -> &'static Mutex<HashMap<usize, Layout>> {
    LARGE_OBJECTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocate a large object via std::alloc. Writes the Header.
pub unsafe fn alloc_large(body_layout: Layout, type_id: TypeId) -> *mut Header {
    let layout = full_layout(body_layout);
    let raw = unsafe { std::alloc::alloc(layout) };
    if raw.is_null() {
        panic!("clojure_rt: OOM allocating large object (Layout {:?})", layout);
    }
    let h = raw as *mut Header;
    unsafe {
        core::ptr::write(h, Header {
            type_id,
            flags: 0,
            rc: AtomicI32::new(Header::INITIAL_RC),
            _pad: 0,
        });
    }
    large_objects().lock().insert(h as usize, body_layout);
    h
}

/// Deallocate a large object. Returns true if the pointer was a large
/// object (and was deallocated); false otherwise (caller should take
/// the RCImmix dealloc path).
pub unsafe fn try_dealloc_large(ptr: *mut Header) -> bool {
    let mut map = large_objects().lock();
    if let Some(body_layout) = map.remove(&(ptr as usize)) {
        drop(map); // release lock before std::alloc::dealloc
        let layout = full_layout(body_layout);
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout); }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_then_try_dealloc_round_trip() {
        unsafe {
            // Pick a body_layout > LARGE_THRESHOLD.
            let body = Layout::from_size_align(16 * 1024, 8).unwrap();
            let h = alloc_large(body, 16);
            assert!(!h.is_null());
            assert_eq!((*h).type_id, 16);
            assert!(try_dealloc_large(h));
            // Second dealloc returns false (not in map).
            // Note: don't actually call try_dealloc_large twice on the same
            // pointer in real code — that would be a double-free check after
            // the alloc had been freed. We test the negative path with a
            // fresh non-registered pointer:
            let bogus = 0xdeadbeef as *mut Header;
            assert!(!try_dealloc_large(bogus));
        }
    }
}
