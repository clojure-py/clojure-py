//! Heap allocator API. The naive impl in `naive.rs` is the v1 default;
//! RCImmix and friends slot in behind this trait without touching clients.

pub mod naive;

use core::alloc::Layout;
use core::ptr::null;
use std::sync::OnceLock;

use crate::header::Header;
use crate::value::TypeId;

/// # Safety
///
/// Implementations must uphold:
/// - `alloc` returns a pointer to a properly initialized `Header` with
///   the requested `type_id`, followed by `body_layout` bytes of allocated
///   (uninitialized) space.
/// - `dealloc` accepts only pointers previously returned by `alloc` of the
///   same allocator with matching `body_layout`.
pub unsafe trait GcAllocator: Send + Sync {
    /// Allocate space for a heap object of the given body layout with the
    /// given type_id. Returns a pointer to the (initialized) `Header`; body
    /// follows the header and is left uninitialized for the caller.
    ///
    /// # Safety
    /// Caller must initialize the body before any other thread observes
    /// the pointer.
    unsafe fn alloc(&self, body_layout: Layout, type_id: TypeId) -> *mut Header;

    /// Deallocate a heap object whose `rc` has reached zero and whose
    /// destructor has already run.
    ///
    /// # Safety
    /// `ptr` must have been returned by a prior `alloc` of this allocator
    /// with the same `body_layout`.
    unsafe fn dealloc(&self, ptr: *mut Header, body_layout: Layout);
}

static ALLOCATOR: OnceLock<&'static dyn GcAllocator> = OnceLock::new();

pub fn install_allocator(a: &'static dyn GcAllocator) {
    ALLOCATOR.set(a).map_err(|_| ()).expect("allocator already installed");
}

pub fn allocator() -> &'static dyn GcAllocator {
    *ALLOCATOR.get().expect("clojure_rt: allocator not installed (call clojure_rt::init())")
}

#[allow(dead_code)]
pub(crate) fn allocator_uninstalled() -> bool {
    ALLOCATOR.get().is_none()
}

/// Compute the combined `Layout` of `Header` followed by `body_layout`.
pub fn full_layout(body_layout: Layout) -> Layout {
    Layout::new::<Header>()
        .extend(body_layout)
        .expect("layout overflow")
        .0
        .pad_to_align()
}

#[allow(dead_code)]
fn _ensure_dyn_object_safe(_a: &dyn GcAllocator) {
    let _ = null::<Header>();
}
