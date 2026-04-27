//! Reference counting fast paths. Single signed counter:
//!   rc < 0  => biased   (magnitude is count, mutated non-atomically)
//!   rc > 0  => shared   (mutated atomically by any thread)
//!   rc == 0 => drop-to-zero
//!
//! Cross-thread sharing must go through `share_heap` BEFORE publication.

use core::sync::atomic::{fence, Ordering};

use crate::header::Header;

/// Increment the refcount of a heap object.
///
/// # Safety
/// `h` must point at a live `Header`. For biased mode (rc < 0), the caller
/// thread must be the owner thread.
#[inline]
pub unsafe fn dup_heap(h: *const Header) {
    let r = unsafe { (*h).rc.load(Ordering::Relaxed) };
    if r < 0 {
        // biased mode: non-atomic decrement (magnitude up by 1)
        unsafe { (*h).rc.store(r - 1, Ordering::Relaxed); }
    } else {
        unsafe { (*h).rc.fetch_add(1, Ordering::Relaxed); }
    }
}

/// Decrement the refcount of a heap object. Returns `true` if the object
/// reached zero and the caller is responsible for destruction + dealloc.
///
/// # Safety
/// `h` must point at a live `Header`. For biased mode (rc < 0), the caller
/// thread must be the owner thread.
#[inline]
pub unsafe fn drop_heap(h: *const Header) -> bool {
    let r = unsafe { (*h).rc.load(Ordering::Relaxed) };
    if r < 0 {
        let new = r + 1;
        unsafe { (*h).rc.store(new, Ordering::Relaxed); }
        new == 0
    } else {
        let prev = unsafe { (*h).rc.fetch_sub(1, Ordering::Release) };
        if prev == 1 {
            fence(Ordering::Acquire);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::Header;
    use core::sync::atomic::AtomicI32;

    fn fresh_header() -> Box<Header> {
        Box::new(Header {
            type_id: 16, flags: 0,
            rc: AtomicI32::new(Header::INITIAL_RC),
            _pad: 0,
        })
    }

    #[test]
    fn biased_dup_then_drop_balances() {
        let h = fresh_header();
        unsafe {
            dup_heap(&*h);                                  // rc: -1 -> -2
            assert_eq!(h.rc.load(Ordering::Relaxed), -2);
            assert!(!drop_heap(&*h));                       // rc: -2 -> -1
            assert_eq!(h.rc.load(Ordering::Relaxed), -1);
        }
    }

    #[test]
    fn biased_final_drop_returns_true() {
        let h = fresh_header();
        unsafe {
            assert!(drop_heap(&*h));                        // rc: -1 -> 0
            assert_eq!(h.rc.load(Ordering::Relaxed), 0);
        }
    }

    fn shared_header() -> Box<Header> {
        // Manually start in shared mode at count=1
        Box::new(Header {
            type_id: 16, flags: 0,
            rc: AtomicI32::new(1),
            _pad: 0,
        })
    }

    #[test]
    fn shared_dup_then_drop_balances() {
        let h = shared_header();
        unsafe {
            dup_heap(&*h);                                   // rc: 1 -> 2
            assert_eq!(h.rc.load(Ordering::Relaxed), 2);
            assert!(!drop_heap(&*h));                        // rc: 2 -> 1
            assert_eq!(h.rc.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn shared_final_drop_returns_true() {
        let h = shared_header();
        unsafe {
            assert!(drop_heap(&*h));                         // rc: 1 -> 0
            assert_eq!(h.rc.load(Ordering::Relaxed), 0);
        }
    }
}
