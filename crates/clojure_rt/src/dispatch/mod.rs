//! Polymorphic dispatch: tier-1 IC, tier-2 per-type perfect-hash table,
//! tier-3 global stub cache, plus the slow-path resolver.

pub mod ic;
pub mod perfect_hash;
pub mod stub_cache;

use core::sync::atomic::Ordering;

use crate::error;
use crate::protocol::ProtocolMethod;
use crate::type_registry;
use crate::value::Value;

use ic::{ICSlot, want_key};

pub type MethodFn = unsafe extern "C" fn(args: *const Value, n: usize) -> Value;

/// Slow-path resolver. Called on IC miss. Walks tier-3 stub cache, then
/// the type's tier-2 perfect-hash table, then the method's fallback;
/// panics via `resolution_failure` on total miss.
#[cold]
#[inline(never)]
pub fn slow_path(
    ic: &ICSlot,
    value: Value,
    method: &ProtocolMethod,
    args: &[Value],
) -> Value {
    let type_id = value.tag;

    if let Some(f) = stub_cache::lookup(type_id, method.method_id) {
        let key = ICSlot::make_key(type_id, method.version.load(Ordering::Relaxed));
        ic.publish(key, f as *const ());
        return unsafe { f(args.as_ptr(), args.len()) };
    }

    if let Some(meta) = type_registry::try_get(type_id) {
        let table = meta.table.load();
        if let Some(f) = table.lookup(method.method_id) {
            stub_cache::insert(type_id, method.method_id, f as *const ());
            let key = ICSlot::make_key(type_id, method.version.load(Ordering::Relaxed));
            ic.publish(key, f as *const ());
            return unsafe { f(args.as_ptr(), args.len()) };
        }
    }

    if let Some(f) = method.fallback {
        return unsafe { f(args.as_ptr(), args.len()) };
    }

    error::resolution_failure(method, type_id);
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
    let want = want_key(value, method);
    if let Some(f) = ic.read(want) {
        return unsafe { f(args.as_ptr(), args.len()) };
    }
    slow_path(ic, value, method, args)
}
