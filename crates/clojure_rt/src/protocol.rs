//! Protocol method identity and per-method invalidation counter.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::dispatch::{foreign, stub_cache, MethodFn};
use crate::type_registry;
use crate::value::Value;

/// A single protocol method. `version` starts at 1 and only ever
/// increases; `key = 0` is therefore a reserved sentinel (never matches).
pub struct ProtocolMethod {
    pub method_id: AtomicU32,                 // patched at init
    pub proto_id:  AtomicU32,                 // patched at init
    pub name:      &'static str,
    pub version:   AtomicU32,
    pub fallback:  Option<MethodFn>,
}

impl ProtocolMethod {
    pub const fn new(name: &'static str) -> Self {
        Self {
            method_id: AtomicU32::new(0),     // patched at init
            proto_id:  AtomicU32::new(0),     // patched at init
            name,
            version:   AtomicU32::new(1),
            fallback:  None,
        }
    }

    pub const fn with_fallback(name: &'static str, fallback: MethodFn) -> Self {
        Self {
            method_id: AtomicU32::new(0),
            proto_id:  AtomicU32::new(0),
            name,
            version:   AtomicU32::new(1),
            fallback:  Some(fallback),
        }
    }
}

/// Does `value`'s effective type have an explicit impl of `method`?
///
/// Walks the same resolution path as dispatch — foreign-type resolver,
/// then stub cache, then per-type table — but returns a boolean instead
/// of calling the impl. Method-level fallbacks deliberately do *not*
/// count: `satisfies?` is a nominal question ("did someone explicitly
/// extend this protocol to this type?") and method fallbacks are a
/// dispatch-time fallthrough, not an extension.
#[inline]
pub fn satisfies(method: &ProtocolMethod, value: Value) -> bool {
    let type_id = foreign::resolve(value.tag, value.payload).unwrap_or(value.tag);
    let mid = method.method_id.load(Ordering::Relaxed);

    if stub_cache::lookup(type_id, mid).is_some() {
        return true;
    }

    if let Some(meta) = type_registry::try_get(type_id) {
        if meta.table.load().lookup(mid).is_some() {
            return true;
        }
    }

    false
}
