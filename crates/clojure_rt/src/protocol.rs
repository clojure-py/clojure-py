//! Protocol method identity and per-method invalidation counter.

use core::sync::atomic::AtomicU32;

use crate::dispatch::MethodFn;

/// A single protocol method. `version` starts at 1 and only ever
/// increases; `key = 0` is therefore a reserved sentinel (never matches).
pub struct ProtocolMethod {
    pub method_id: u32,
    pub proto_id:  u32,
    pub name:      &'static str,
    pub version:   AtomicU32,
    pub fallback:  Option<MethodFn>,
}

impl ProtocolMethod {
    pub const fn new(name: &'static str) -> Self {
        Self {
            method_id: 0,                     // patched at init
            proto_id:  0,                     // patched at init
            name,
            version:   AtomicU32::new(1),
            fallback:  None,
        }
    }
}
