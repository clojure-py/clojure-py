//! Internal HAMT nodes for PersistentVector.
//!
//! Port of the Node inner class from clojure/lang/PersistentVector.java.
//! Each node has 32 slots holding either a child Node or a leaf PyObject.
//!
//! Transient variant (Phase 6C) adds an edit token to this struct; in this
//! task's persistent-only world the `edit` field is always None.

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

pub(crate) type PyObject = Py<PyAny>;

/// A node in the vector's trie. `array` has exactly 32 slots; each slot is
/// either None (empty), a boxed Arc<VNode> (interior branch), or a boxed
/// PyObject (leaf). We use a single type erased via an enum to minimize
/// allocations and keep layout stable.
pub(crate) enum VSlot {
    Empty,
    Branch(Arc<VNode>),
    Leaf(PyObject),
}

impl Clone for VSlot {
    fn clone(&self) -> Self {
        match self {
            VSlot::Empty => VSlot::Empty,
            VSlot::Branch(b) => VSlot::Branch(Arc::clone(b)),
            VSlot::Leaf(l) => {
                // We need a Python GIL to clone_ref. Use a temporary: this method
                // is only called inside code that holds the GIL; we can attach to it.
                Python::attach(|py| VSlot::Leaf(l.clone_ref(py)))
            }
        }
    }
}

pub(crate) struct VNode {
    /// `edit` = None in persistent nodes. Populated during transient use (Phase 6C).
    pub edit: Option<Arc<AtomicUsize>>,
    /// 32-slot array. Boxed via Mutex for interior mutability during transient
    /// edits (Phase 6C); persistent ops always clone the whole array.
    pub array: Mutex<[VSlot; 32]>,
}

impl VNode {
    pub fn empty() -> Arc<Self> {
        Arc::new(VNode {
            edit: None,
            array: Mutex::new(std::array::from_fn(|_| VSlot::Empty)),
        })
    }

    /// Deep-clone this node's array slots (structural-sharing does not extend
    /// to the 32-slot vector itself — branches inside it ARE shared).
    pub fn clone_array(&self) -> [VSlot; 32] {
        let g = self.array.lock();
        std::array::from_fn(|i| g[i].clone())
    }
}
