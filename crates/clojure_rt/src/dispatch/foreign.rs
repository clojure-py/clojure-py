//! Per-primitive-tag *foreign type resolver* hook.
//!
//! Some primitive Value tags (notably `TYPE_PYOBJECT`) are buckets for
//! values whose effective type is determined at runtime by inspecting
//! the payload — a Python list, dict, str, and custom class all share
//! the `TYPE_PYOBJECT` tag, but each is a distinct *class* from the
//! perspective of protocol dispatch. The resolver hook lets a foreign
//! embedding (`clojure_py` for Python; future bindings for other
//! languages) install a callback that maps `payload -> TypeId`,
//! producing the effective `TypeId` used for IC keying and per-type
//! table lookup.
//!
//! Clojure-native tags (no resolver registered) take a single
//! `AtomicPtr` load on the hot path and proceed unchanged.

use core::sync::atomic::{AtomicPtr, Ordering};

use crate::value::{TypeId, FIRST_HEAP_TYPE};

/// Foreign-type resolver: receives the Value's payload, returns the
/// effective `TypeId` to use for dispatch. The function must be safe
/// to call concurrently from any thread that's running dispatch.
pub type ForeignTypeResolver = unsafe fn(payload: u64) -> TypeId;

#[allow(clippy::declare_interior_mutable_const)]
const NULL_RESOLVER: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// One slot per primitive tag (heap-tagged values never need a resolver
/// — `value.tag` is already their `TypeId`).
static FOREIGN_RESOLVERS: [AtomicPtr<()>; FIRST_HEAP_TYPE as usize] =
    [NULL_RESOLVER; FIRST_HEAP_TYPE as usize];

/// Install a resolver for a given primitive tag. Idempotent in the
/// sense of "last writer wins"; in practice each foreign embedding
/// registers exactly once during its `init()`.
pub fn register_foreign_resolver(tag: TypeId, resolver: ForeignTypeResolver) {
    assert!(
        tag < FIRST_HEAP_TYPE,
        "register_foreign_resolver: tag {tag} is a heap type, not a primitive",
    );
    FOREIGN_RESOLVERS[tag as usize].store(resolver as *mut (), Ordering::Release);
}

/// If a resolver is registered for `tag`, call it on `payload` to get
/// the effective `TypeId`. Returns `None` for the common case of an
/// unhooked Clojure-native tag.
#[inline(always)]
pub fn resolve(tag: TypeId, payload: u64) -> Option<TypeId> {
    if tag >= FIRST_HEAP_TYPE {
        return None;
    }
    let raw = FOREIGN_RESOLVERS[tag as usize].load(Ordering::Acquire);
    if raw.is_null() {
        return None;
    }
    let resolver: ForeignTypeResolver = unsafe {
        core::mem::transmute::<*mut (), ForeignTypeResolver>(raw)
    };
    Some(unsafe { resolver(payload) })
}
