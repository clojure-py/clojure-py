//! Reference counting fast paths. Single signed counter:
//!   rc < 0  => biased   (magnitude is count, mutated non-atomically)
//!   rc > 0  => shared   (mutated atomically by any thread)
//!   rc == 0 => drop-to-zero
//!
//! Cross-thread sharing must go through `share_heap` BEFORE publication.

use core::sync::atomic::{fence, Ordering};

use crate::gc::allocator;
use crate::header::Header;
use crate::value::Value;
use crate::type_registry;

/// Increment the refcount of a heap object.
///
/// # Safety
/// `h` must point at a live `Header`. For biased mode (rc < 0), the caller
/// thread must be the owner thread.
#[inline]
pub unsafe fn dup_heap(h: *const Header) {
    let r = unsafe { (*h).rc.load(Ordering::Relaxed) };
    if r < 0 {
        unsafe { debug_assert_owner(h, "dup_heap"); }
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
        unsafe { debug_assert_owner(h, "drop_heap"); }
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

/// Debug-only check that the calling thread owns this biased-mode
/// header. Panics with a clear message if a thread other than the
/// owner mutates the rc — the symptom of a missing `rc::share()` call
/// before publishing the value across threads (the bug class behind
/// the singleton corruption fixed in commit a393ec7).
///
/// Compiled out entirely in release builds; the field read and the
/// `tid` lookup are also gated behind `cfg(debug_assertions)`.
#[inline(always)]
unsafe fn debug_assert_owner(_h: *const Header, _op: &'static str) {
    #[cfg(debug_assertions)]
    {
        let owner = unsafe { (*_h).owner_tid };
        let me = crate::gc::rcimmix::tid::current_tid() as u32;
        debug_assert_eq!(
            owner, me,
            "rc::{}: biased-mode mutation from non-owner thread \
             (owner_tid={}, current_tid={}) — did you forget to call \
             rc::share() before publishing this value across threads?",
            _op, owner, me
        );
    }
}

/// Mark a heap object as shared (atomic mode). Idempotent. Called only
/// by sharing primitives (atom/ref/channel) on the owner thread BEFORE
/// publication of the object to another thread.
///
/// # Safety
/// `h` must point at a live `Header`. Caller thread must currently be
/// the owner (i.e. the only thread mutating `rc`).
#[inline]
pub unsafe fn share_heap(h: *const Header) {
    loop {
        let r = unsafe { (*h).rc.load(Ordering::Relaxed) };
        if r > 0 {
            return; // already shared
        }
        unsafe { debug_assert_owner(h, "share_heap"); }
        // Biased mode: r < 0. Flip to +(-r) atomically.
        let new = -r;
        match unsafe {
            (*h).rc.compare_exchange(r, new, Ordering::Release, Ordering::Relaxed)
        } {
            Ok(_) => {
                // Clear owner_tid in debug builds — post-share the
                // header is shared by all threads, so the assertion
                // path no longer applies. Release builds skip this.
                #[cfg(debug_assertions)]
                unsafe {
                    (*(h as *mut Header)).owner_tid = Header::UNOWNED_TID;
                }
                return;
            }
            Err(_) => continue,
        }
    }
}

/// Increment refcount of a Value (no-op for primitives).
#[inline]
pub fn dup(v: Value) {
    if v.is_heap() {
        unsafe { dup_heap(v.payload as *const Header) };
    }
}

/// Decrement refcount of a Value (no-op for primitives). On drop-to-zero,
/// runs the type's destructor and deallocates.
#[inline]
pub fn drop_value(v: Value) {
    if !v.is_heap() { return; }
    let h = v.payload as *mut Header;
    let zeroed = unsafe { drop_heap(h) };
    if zeroed {
        unsafe { destruct_and_dealloc(h); }
    }
}

/// Mark a Value as shared (escape op). No-op for primitives.
#[inline]
pub fn share(v: Value) {
    if v.is_heap() {
        unsafe { share_heap(v.payload as *const Header) };
    }
}

unsafe fn destruct_and_dealloc(h: *mut Header) {
    let meta = type_registry::get(unsafe { (*h).type_id });
    unsafe { (meta.destruct)(h); }
    unsafe { allocator().dealloc(h, meta.layout); }
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
            owner_tid: crate::gc::rcimmix::tid::current_owner_tid(),
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
            owner_tid: Header::UNOWNED_TID,
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

    #[test]
    fn share_flips_biased_to_shared() {
        let h = fresh_header();                 // rc = -1
        unsafe {
            share_heap(&*h);
            assert_eq!(h.rc.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn share_is_idempotent_on_shared() {
        let h = shared_header();                // rc = 1
        unsafe {
            share_heap(&*h);
            assert_eq!(h.rc.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn share_preserves_count_magnitude() {
        let h = fresh_header();                 // rc = -1
        unsafe {
            dup_heap(&*h);                      // rc = -2
            dup_heap(&*h);                      // rc = -3
            share_heap(&*h);                    // rc = +3
            assert_eq!(h.rc.load(Ordering::Relaxed), 3);
        }
    }
}
