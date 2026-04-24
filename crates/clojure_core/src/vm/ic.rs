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

use crate::protocol_fn::InvokeFns;
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

    /// Fast-path lookup: returns `Some(fns)` iff the entry matches
    /// `(target_type, current_epoch)`. The returned `Arc<InvokeFns>` is a
    /// cheap refcount bump.
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

    #[inline]
    pub fn install(&self, entry: ICEntry) {
        self.slot.store(Arc::new(Some(entry)));
    }
}

impl Default for CachedInvoke {
    fn default() -> Self { Self::new() }
}
