//! Polymorphic dispatch: tier-1 IC, tier-2 per-type perfect-hash table,
//! tier-3 global stub cache, plus the slow-path resolver.

pub mod foreign;
pub mod ic;
pub mod perfect_hash;
pub mod stub_cache;

use core::sync::atomic::Ordering;

use crate::error;
use crate::protocol::ProtocolMethod;
use crate::type_registry;
use crate::value::{TypeId, Value};

use ic::ICSlot;

pub type MethodFn = unsafe extern "C" fn(args: *const Value, n: usize) -> Value;

/// Slow-path resolver. Called on IC miss. Walks tier-3 stub cache, then
/// the type's tier-2 perfect-hash table, then the method's fallback;
/// returns a throwable exception Value via `resolution_failure` on
/// total miss.
#[cold]
#[inline(never)]
pub fn slow_path(
    ic: &ICSlot,
    type_id: TypeId,
    method: &ProtocolMethod,
    args: &[Value],
) -> Value {
    if let Some(f) = stub_cache::lookup(type_id, method.method_id.load(Ordering::Relaxed)) {
        let key = ICSlot::make_key(type_id, method.version.load(Ordering::Relaxed));
        ic.publish(key, f as *const ());
        return unsafe { f(args.as_ptr(), args.len()) };
    }

    if let Some(meta) = type_registry::try_get(type_id) {
        let table = meta.table.load();
        let mid = method.method_id.load(Ordering::Relaxed);
        if let Some(f) = table.lookup(mid) {
            stub_cache::insert(type_id, mid, f as *const ());
            let key = ICSlot::make_key(type_id, method.version.load(Ordering::Relaxed));
            ic.publish(key, f as *const ());
            return unsafe { f(args.as_ptr(), args.len()) };
        }
    }

    if let Some(f) = method.fallback {
        return unsafe { f(args.as_ptr(), args.len()) };
    }

    error::resolution_failure(method, type_id)
}

/// Top-level dispatch entry. The `dispatch!` macro inlines the fast path
/// and only falls through to this on miss; this fn is also useful for
/// uncached callers.
#[inline]
pub fn dispatch_fn(
    ic: &ICSlot,
    method: &ProtocolMethod,
    args: &[Value],
) -> Value {
    debug_assert!(!args.is_empty(), "dispatch_fn: empty args (no receiver)");
    let value = args[0];
    // Foreign embeddings (clojure_py, future bindings) hook a per-tag
    // resolver to map `(tag, payload)` to a finer-grained TypeId — e.g.
    // every Python class gets its own TypeId via lazy interning. For
    // Clojure-native tags this is a single AtomicPtr load + null-check,
    // a near-zero predictable branch.
    let type_id = foreign::resolve(value.tag, value.payload).unwrap_or(value.tag);
    let want = ICSlot::make_key(type_id, method.version.load(Ordering::Relaxed));
    if let Some(f) = ic.read(want) {
        return unsafe { f(args.as_ptr(), args.len()) };
    }
    slow_path(ic, type_id, method, args)
}
