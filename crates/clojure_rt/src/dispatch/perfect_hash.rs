//! Per-type tier-2 dispatch table. Ducournau PN-and perfect hashing:
//! find the smallest mask `2^k - 1` such that all live method_ids
//! `m & mask` are pairwise distinct.

use core::ptr::null;

use crate::dispatch::MethodFn;

#[repr(C, align(16))]
#[derive(Copy, Clone)]
pub struct Slot {
    pub method_id: u32,        // 0 = empty
    pub _pad:      u32,
    pub fn_ptr:    *const (),
}

unsafe impl Send for Slot {}
unsafe impl Sync for Slot {}

impl Slot {
    pub const EMPTY: Slot = Slot { method_id: 0, _pad: 0, fn_ptr: null() };
}

pub struct PerTypeTable {
    pub mask:  u32,
    pub slots: Box<[Slot]>,
}

impl PerTypeTable {
    pub fn empty() -> Self {
        Self { mask: 0, slots: Box::new([Slot::EMPTY]) }
    }

    /// Build a table from an iterator of (method_id, fn_ptr) pairs.
    /// Panics if any method_id is 0 (the sentinel).
    pub fn build(impls: &[(u32, *const ())]) -> Self {
        for (mid, _) in impls {
            assert!(*mid != 0, "PerTypeTable: method_id 0 is reserved");
        }
        let n = impls.len() as u32;
        let mut k = (32 - n.leading_zeros()).max(1);   // smallest 2^k >= n
        loop {
            let cap = 1u32 << k;
            let mask = cap - 1;
            let mut slots: Vec<Slot> = vec![Slot::EMPTY; cap as usize];
            let mut collision = false;
            for &(mid, fp) in impls {
                let idx = (mid & mask) as usize;
                if slots[idx].method_id != 0 {
                    collision = true;
                    break;
                }
                slots[idx] = Slot { method_id: mid, _pad: 0, fn_ptr: fp };
            }
            if !collision {
                return PerTypeTable { mask, slots: slots.into_boxed_slice() };
            }
            k += 1;
            if k > 24 { panic!("PerTypeTable: failed to find a perfect hash"); }
        }
    }

    /// O(1) lookup: hash, single compare. Caller must pass `method_id != 0`
    /// (0 is the empty-slot sentinel; passing it would match an empty slot
    /// and transmute a null pointer).
    #[inline]
    pub fn lookup(&self, method_id: u32) -> Option<MethodFn> {
        debug_assert!(method_id != 0, "method_id 0 is reserved (empty-slot sentinel)");
        let s = self.slots[(method_id & self.mask) as usize];
        if s.method_id == method_id {
            // SAFETY: on x86-64/aarch64 (the only supported targets),
            // `*const ()` and `unsafe extern "C" fn` are the same width
            // and ABI-compatible. Cranelift hardening tracked in
            // doc/deferred-work.md.
            Some(unsafe { core::mem::transmute::<*const (), MethodFn>(s.fn_ptr) })
        } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn fake(_: *const crate::value::Value, _: usize) -> crate::value::Value {
        crate::value::Value::NIL
    }

    fn fp() -> *const () { fake as *const () }

    #[test]
    fn empty_table_misses_everything() {
        let t = PerTypeTable::empty();
        assert!(t.lookup(1).is_none());
        assert!(t.lookup(42).is_none());
    }

    #[test]
    fn single_entry_hits() {
        let t = PerTypeTable::build(&[(7, fp())]);
        assert!(t.lookup(7).is_some());
        assert!(t.lookup(8).is_none());
    }

    #[test]
    fn many_entries_no_collision() {
        let entries: Vec<_> = (1..=10).map(|m| (m, fp())).collect();
        let t = PerTypeTable::build(&entries);
        for m in 1..=10 { assert!(t.lookup(m).is_some(), "miss on {m}"); }
        assert!(t.lookup(11).is_none());
    }

    #[test]
    fn pathological_collision_input_still_resolves() {
        // method_ids that would all collide on a tiny mask
        let entries = vec![(0x100, fp()), (0x200, fp()), (0x300, fp()), (0x400, fp())];
        let t = PerTypeTable::build(&entries);
        for &(m, _) in &entries { assert!(t.lookup(m).is_some()); }
    }
}
