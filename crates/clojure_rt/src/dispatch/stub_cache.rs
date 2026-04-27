//! Tier-3 global stub cache. 4096 entries, hash on (type_id ^ method_id).
//! Same publish-and-double-read protocol as the per-callsite IC.

use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use crate::dispatch::MethodFn;

pub const STUB_CACHE_SIZE: usize = 4096;
const STUB_MASK: u64 = (STUB_CACHE_SIZE - 1) as u64;

#[repr(C, align(64))]
pub struct StubEntry {
    pub key:    AtomicU64,        // packed (type_id, method_id)
    pub fn_ptr: AtomicPtr<()>,
}

impl StubEntry {
    /// Initializer template for `[StubEntry::EMPTY; N]`. Each array slot is
    /// independently constructed from this const expression.
    #[allow(clippy::declare_interior_mutable_const)]
    pub const EMPTY: StubEntry = StubEntry {
        key:    AtomicU64::new(0),
        fn_ptr: AtomicPtr::new(null_mut()),
    };
}

#[allow(clippy::declare_interior_mutable_const)]
const EMPTY: StubEntry = StubEntry::EMPTY;
static STUB_CACHE: [StubEntry; STUB_CACHE_SIZE] = [EMPTY; STUB_CACHE_SIZE];

#[inline(always)]
fn idx(type_id: u32, method_id: u32) -> usize {
    (((type_id ^ method_id) as u64) & STUB_MASK) as usize
}

#[inline(always)]
fn pack_key(type_id: u32, method_id: u32) -> u64 {
    ((type_id as u64) << 32) | (method_id as u64)
}

/// Lookup. Returns `Some` only if the entry matches both key reads.
#[inline]
pub fn lookup(type_id: u32, method_id: u32) -> Option<MethodFn> {
    let want = pack_key(type_id, method_id);
    let e = &STUB_CACHE[idx(type_id, method_id)];
    let k1 = e.key.load(Ordering::Acquire);
    if k1 != want { return None; }
    let f  = e.fn_ptr.load(Ordering::Acquire);
    let k2 = e.key.load(Ordering::Acquire);
    if k1 == k2 {
        // SAFETY: on x86-64/aarch64 (the only supported targets),
        // `*const ()` and `unsafe extern "C" fn` are the same width
        // and ABI-compatible. Cranelift hardening tracked in
        // doc/deferred-work.md.
        Some(unsafe { core::mem::transmute::<*const (), MethodFn>(f as *const ()) })
    } else { None }
}

/// Insert/overwrite. Same publish protocol as IC.
#[inline]
pub fn insert(type_id: u32, method_id: u32, f: *const ()) {
    let key = pack_key(type_id, method_id);
    let e = &STUB_CACHE[idx(type_id, method_id)];
    e.key.store(0, Ordering::Release);
    e.fn_ptr.store(f as *mut _, Ordering::Release);
    e.key.store(key, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    unsafe extern "C" fn fake(_: *const Value, _: usize) -> Value { Value::NIL }
    fn fp() -> *const () { fake as *const () }

    #[test]
    fn miss_then_hit() {
        // Use type_ids high enough not to collide with other tests.
        let t = 0x4001;
        let m = 0xDEAD;
        assert!(lookup(t, m).is_none());
        insert(t, m, fp());
        assert!(lookup(t, m).is_some());
        assert!(lookup(t + 1, m).is_none());
    }
}
