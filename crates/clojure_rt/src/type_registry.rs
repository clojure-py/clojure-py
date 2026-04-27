//! Global type registry: u32 type-id interning + per-type metadata.

use core::alloc::Layout;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::header::Header;
use crate::value::{TypeId, FIRST_HEAP_TYPE};
pub use crate::dispatch::perfect_hash::PerTypeTable;

/// Maximum simultaneous types. v1 simplification — adjust later.
pub const MAX_TYPES: usize = 1 << 16;

/// Per-type metadata. Tier-2 dispatch table swappable via ArcSwap.
pub struct TypeMeta {
    pub type_id:  TypeId,
    pub name:     &'static str,
    pub layout:   Layout,
    pub destruct: unsafe fn(*mut Header),
    pub table:    ArcSwap<PerTypeTable>,
}

#[allow(clippy::declare_interior_mutable_const)]
const NULL_META: AtomicPtr<TypeMeta> = AtomicPtr::new(null_mut());
static TYPES: [AtomicPtr<TypeMeta>; MAX_TYPES] = [NULL_META; MAX_TYPES];
static NEXT_ID: AtomicU32 = AtomicU32::new(FIRST_HEAP_TYPE);

fn register_type_inner(
    name: &'static str,
    layout: Layout,
    destruct: unsafe fn(*mut Header),
) -> TypeId {
    let id = NEXT_ID.fetch_add(1, Ordering::AcqRel);
    assert!((id as usize) < MAX_TYPES, "clojure_rt: type id space exhausted");
    let meta = Box::leak(Box::new(TypeMeta {
        type_id: id, name, layout, destruct,
        table: ArcSwap::from(Arc::new(PerTypeTable::empty())),
    }));
    TYPES[id as usize].store(meta as *mut _, Ordering::Release);
    id
}

pub fn register_static_type(
    name: &'static str,
    layout: Layout,
    destruct: unsafe fn(*mut Header),
) -> TypeId { register_type_inner(name, layout, destruct) }

pub fn register_dynamic_type(
    name: &'static str,
    layout: Layout,
    destruct: unsafe fn(*mut Header),
) -> TypeId { register_type_inner(name, layout, destruct) }

/// Look up a type by id. Panics if the id has no registered meta.
pub fn get(id: TypeId) -> &'static TypeMeta {
    try_get(id).unwrap_or_else(|| panic!("clojure_rt: no TypeMeta for type_id {id}"))
}

pub fn try_get(id: TypeId) -> Option<&'static TypeMeta> {
    let p = TYPES[id as usize].load(Ordering::Acquire);
    if p.is_null() { None } else { unsafe { Some(&*p) } }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn noop_destruct(_: *mut Header) {}

    #[test]
    fn register_then_lookup() {
        let layout = Layout::from_size_align(8, 8).unwrap();
        let id = register_static_type("TestA", layout, noop_destruct);
        assert!(id >= FIRST_HEAP_TYPE);
        let meta = get(id);
        assert_eq!(meta.type_id, id);
        assert_eq!(meta.name, "TestA");
        assert_eq!(meta.layout, layout);
    }

    #[test]
    fn ids_are_unique_and_monotonic() {
        let layout = Layout::from_size_align(8, 8).unwrap();
        let a = register_static_type("UniqA", layout, noop_destruct);
        let b = register_static_type("UniqB", layout, noop_destruct);
        let c = register_static_type("UniqC", layout, noop_destruct);
        assert!(a < b && b < c);
    }

    #[test]
    fn try_get_unregistered_returns_none() {
        // Pick a type id well above any plausible registration in this
        // test binary. Using MAX_TYPES - 1 is safe because we never
        // register that high in tests.
        assert!(try_get((MAX_TYPES - 1) as u32).is_none());
    }
}
