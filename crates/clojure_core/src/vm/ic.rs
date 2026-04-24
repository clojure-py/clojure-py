//! Inline cache for `Op::InvokeVar`.
//!
//! Each fused call site gets its own `CachedInvoke` slot in `FnPool.ic_slots`.
//! A slot records the last observed `(target_type, protocol_epoch, InvokeFns)`
//! tuple for that call site. On a repeat call with the same target type (and
//! no protocol-epoch bump since install), dispatch skips `ProtocolFn::resolve`
//! and jumps straight into the typed `invoke_N` fn pointer.
//!
//! Thread safety: `ArcSwap<Option<ICEntry>>` — lock-free reads, lock-free
//! writes, deferred Arc reclamation handled by `arc-swap`. Fine for the
//! shared-bytecode-across-threads model (multiple threads execute the same
//! `Arc<FnPool>` concurrently).

use crate::protocol_fn::{InvokeFn0, InvokeFn1, InvokeFn2, InvokeFns};
use arc_swap::ArcSwap;
use std::sync::Arc;

/// One IC entry. Comparison keys: `type_ptr` (erased PyType pointer) and
/// `epoch` (snapshot of the backing `ProtocolFn.epoch` at install time).
pub struct ICEntry {
    pub type_ptr: usize,
    pub epoch: u64,
    pub fns: Arc<InvokeFns>,
}

pub struct CachedInvoke {
    slot: ArcSwap<Option<ICEntry>>,
}

impl CachedInvoke {
    pub fn new() -> Self {
        Self { slot: ArcSwap::from_pointee(None) }
    }

    /// Fast-path lookup: returns the cached `Arc<InvokeFns>` iff the entry
    /// matches `(target_type, current_epoch)`. Used by the generic
    /// `Op::InvokeVar` (arity ≥ 3) handler. Hot-path `Arc::clone` is one
    /// atomic increment.
    #[inline]
    pub fn lookup(&self, type_ptr: usize, current_epoch: u64) -> Option<Arc<InvokeFns>> {
        let guard = self.slot.load();
        let entry = (&**guard).as_ref()?;
        if entry.type_ptr == type_ptr && entry.epoch == current_epoch {
            Some(Arc::clone(&entry.fns))
        } else {
            None
        }
    }

    /// Arity-0 typed lookup. Returns the `invoke0` fn pointer directly —
    /// no `Arc::clone` on the hot path. Used by `Op::InvokeVar0`.
    #[inline]
    pub fn lookup_invoke0(&self, type_ptr: usize, current_epoch: u64) -> Option<InvokeFn0> {
        let guard = self.slot.load();
        let entry = (&**guard).as_ref()?;
        if entry.type_ptr == type_ptr && entry.epoch == current_epoch {
            entry.fns.invoke0
        } else {
            None
        }
    }

    /// Arity-1 typed lookup.
    #[inline]
    pub fn lookup_invoke1(&self, type_ptr: usize, current_epoch: u64) -> Option<InvokeFn1> {
        let guard = self.slot.load();
        let entry = (&**guard).as_ref()?;
        if entry.type_ptr == type_ptr && entry.epoch == current_epoch {
            entry.fns.invoke1
        } else {
            None
        }
    }

    /// Arity-2 typed lookup.
    #[inline]
    pub fn lookup_invoke2(&self, type_ptr: usize, current_epoch: u64) -> Option<InvokeFn2> {
        let guard = self.slot.load();
        let entry = (&**guard).as_ref()?;
        if entry.type_ptr == type_ptr && entry.epoch == current_epoch {
            entry.fns.invoke2
        } else {
            None
        }
    }

    #[inline]
    pub fn install(&self, entry: ICEntry) {
        self.slot.store(Arc::new(Some(entry)));
    }
}

impl Default for CachedInvoke {
    fn default() -> Self { Self::new() }
}
