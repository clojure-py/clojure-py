//! Per-call-site inline cache. 16-byte slot, 4 ICs per cache line.

use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use crate::dispatch::MethodFn;
use crate::protocol::ProtocolMethod;
use crate::value::Value;

#[repr(C, align(16))]
pub struct ICSlot {
    pub key:    AtomicU64,
    pub fn_ptr: AtomicPtr<()>,
}

impl ICSlot {
    pub const EMPTY: ICSlot = ICSlot {
        key:    AtomicU64::new(0),
        fn_ptr: AtomicPtr::new(null_mut()),
    };

    /// Pack (type_id, version) into a u64 key.
    #[inline(always)]
    pub fn make_key(type_id: u32, version: u32) -> u64 {
        ((type_id as u64) << 32) | (version as u64)
    }

    /// Slow-path update protocol: invalidate, publish fn, publish key.
    /// Three Release stores; readers' double-read detects in-flight updates.
    #[inline]
    pub fn publish(&self, key: u64, fn_ptr: *const ()) {
        self.key.store(0, Ordering::Release);
        self.fn_ptr.store(fn_ptr as *mut _, Ordering::Release);
        self.key.store(key, Ordering::Release);
    }

    /// Fast-path read with double-key verification. Returns Some(fn_ptr)
    /// only if `want` matched both reads of `key`.
    #[inline(always)]
    pub fn read(&self, want: u64) -> Option<MethodFn> {
        let k1 = self.key.load(Ordering::Acquire);
        if k1 != want { return None; }
        let f  = self.fn_ptr.load(Ordering::Acquire);
        let k2 = self.key.load(Ordering::Acquire);
        if k1 == k2 {
            // SAFETY: on x86-64/aarch64 (the only supported targets),
            // `*const ()` and `unsafe extern "C" fn` are the same width
            // and ABI-compatible. Cranelift hardening tracked in
            // doc/deferred-work.md.
            Some(unsafe { core::mem::transmute::<*const (), MethodFn>(f as *const ()) })
        } else { None }
    }
}

/// Compute the IC `want` key for a given Value + ProtocolMethod.
#[inline(always)]
pub fn want_key(value: Value, method: &ProtocolMethod) -> u64 {
    ICSlot::make_key(value.tag, method.version.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn fake(_: *const Value, _: usize) -> Value { Value::NIL }
    fn fp() -> *const () { fake as *const () }

    #[test]
    fn empty_ic_misses() {
        let ic = ICSlot::EMPTY;
        assert!(ic.read(ICSlot::make_key(16, 1)).is_none());
    }

    #[test]
    fn publish_then_hit() {
        let ic = ICSlot::EMPTY;
        let key = ICSlot::make_key(16, 1);
        ic.publish(key, fp());
        assert!(ic.read(key).is_some());
        assert!(ic.read(ICSlot::make_key(17, 1)).is_none());
        assert!(ic.read(ICSlot::make_key(16, 2)).is_none());
    }
}
