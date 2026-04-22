//! Internal HAMT nodes for PersistentHashMap.
//!
//! Port of `INode` / `BitmapIndexedNode` / `ArrayNode` / `HashCollisionNode`
//! nested classes from clojure/lang/PersistentHashMap.java.
//!
//! Three node variants:
//!   - `Bitmap`: sparse node; 32-bit `bitmap` + `Vec<MEntry>` packed by popcount.
//!   - `Array`:  dense node; 32-slot `Vec<Option<Arc<MNode>>>` (promoted at ≥16).
//!   - `Collision`: all entries share a folded hash; linear-scanned.
//!
//! Mutable inner data is wrapped in `parking_lot::Mutex` so transient ops
//! (Phase 8C) can mutate in-place when the node's `edit` token matches the
//! transient's. Persistent ops still always produce fresh nodes.

use parking_lot::Mutex;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

pub(crate) type PyObject = Py<PyAny>;

/// Fold a 64-bit Clojure `hash_eq` result down to 32 bits (matching Java's
/// `int` hash width for HAMT shift arithmetic). Same fold used by
/// `clojure.core/hash`.
pub fn fold_hash_i64(h: i64) -> i32 {
    let u = h as u64;
    ((u ^ (u >> 32)) as u32) as i32
}

/// `mask(hash, shift) = (hash >>> shift) & 0x01f`
#[inline]
pub fn mask(hash: i32, shift: u32) -> u32 {
    ((hash as u32) >> shift) & 0x01f
}

/// `bitpos(hash, shift) = 1 << mask(hash, shift)`
#[inline]
pub fn bitpos(hash: i32, shift: u32) -> u32 {
    1u32 << mask(hash, shift)
}

/// `index(bitmap, bit) = bitCount(bitmap & (bit - 1))`
#[inline]
pub fn bitmap_index(bitmap: u32, bit: u32) -> usize {
    (bitmap & (bit - 1)).count_ones() as usize
}

/// Entry in a `BitmapIndexedNode`: either a key/value leaf or a subtree slot.
pub enum MEntry {
    KV { key: PyObject, val: PyObject },
    Child { node: Arc<MNode> },
}

impl MEntry {
    fn clone_entry(&self, py: Python<'_>) -> MEntry {
        match self {
            MEntry::KV { key, val } => MEntry::KV {
                key: key.clone_ref(py),
                val: val.clone_ref(py),
            },
            MEntry::Child { node } => MEntry::Child {
                node: Arc::clone(node),
            },
        }
    }
}

pub struct BitmapIndexedInner {
    pub bitmap: u32,
    pub array: Vec<MEntry>,
}

pub struct BitmapIndexedNode {
    pub inner: Mutex<BitmapIndexedInner>,
    pub edit: Option<Arc<AtomicUsize>>,
}

pub struct ArrayInner {
    pub count: u32,
    pub array: Vec<Option<Arc<MNode>>>, // length == 32
}

pub struct ArrayNode {
    pub inner: Mutex<ArrayInner>,
    pub edit: Option<Arc<AtomicUsize>>,
}

pub struct CollisionInner {
    pub hash: i32,
    pub entries: Vec<(PyObject, PyObject)>,
}

pub struct HashCollisionNode {
    pub inner: Mutex<CollisionInner>,
    pub edit: Option<Arc<AtomicUsize>>,
}

/// `INode` variants. Each method dispatches to the variant implementation.
pub enum MNode {
    Bitmap(BitmapIndexedNode),
    Array(ArrayNode),
    Collision(HashCollisionNode),
}

impl MNode {
    /// Empty BitmapIndexedNode — start of every fresh HAMT.
    pub fn empty_bitmap() -> Arc<MNode> {
        Arc::new(MNode::Bitmap(BitmapIndexedNode {
            inner: Mutex::new(BitmapIndexedInner {
                bitmap: 0,
                array: Vec::new(),
            }),
            edit: None,
        }))
    }

    /// First-insert shortcut: build a one-entry bitmap node directly.
    pub fn create_leaf(
        py: Python<'_>,
        shift: u32,
        hash: i32,
        key: PyObject,
        val: PyObject,
    ) -> PyResult<(Arc<MNode>, bool)> {
        MNode::empty_bitmap().assoc(py, shift, hash, key, val)
    }

    /// Return this node's edit token (if any).
    pub fn edit_token(&self) -> Option<&Arc<AtomicUsize>> {
        match self {
            MNode::Bitmap(n) => n.edit.as_ref(),
            MNode::Array(n) => n.edit.as_ref(),
            MNode::Collision(n) => n.edit.as_ref(),
        }
    }

    /// Return self (wrapped in Arc::clone) if this node's edit token matches
    /// `edit`; otherwise produce a fresh clone stamped with `edit`.
    pub fn ensure_editable(self: &Arc<MNode>, edit: &Arc<AtomicUsize>) -> Arc<MNode> {
        if let Some(e) = self.edit_token() {
            if Arc::ptr_eq(e, edit) {
                return Arc::clone(self);
            }
        }
        // Clone stamped with edit.
        Python::attach(|py| self.editable_clone(py, edit))
    }

    fn editable_clone(&self, py: Python<'_>, edit: &Arc<AtomicUsize>) -> Arc<MNode> {
        match self {
            MNode::Bitmap(n) => {
                let g = n.inner.lock();
                let arr: Vec<MEntry> = g.array.iter().map(|e| e.clone_entry(py)).collect();
                Arc::new(MNode::Bitmap(BitmapIndexedNode {
                    inner: Mutex::new(BitmapIndexedInner {
                        bitmap: g.bitmap,
                        array: arr,
                    }),
                    edit: Some(Arc::clone(edit)),
                }))
            }
            MNode::Array(n) => {
                let g = n.inner.lock();
                let arr: Vec<Option<Arc<MNode>>> =
                    g.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect();
                Arc::new(MNode::Array(ArrayNode {
                    inner: Mutex::new(ArrayInner {
                        count: g.count,
                        array: arr,
                    }),
                    edit: Some(Arc::clone(edit)),
                }))
            }
            MNode::Collision(n) => {
                let g = n.inner.lock();
                let entries: Vec<(PyObject, PyObject)> =
                    g.entries.iter().map(|(k, v)| (k.clone_ref(py), v.clone_ref(py))).collect();
                Arc::new(MNode::Collision(HashCollisionNode {
                    inner: Mutex::new(CollisionInner {
                        hash: g.hash,
                        entries,
                    }),
                    edit: Some(Arc::clone(edit)),
                }))
            }
        }
    }

    // --- find ---

    pub fn find(
        &self,
        py: Python<'_>,
        shift: u32,
        hash: i32,
        key: PyObject,
    ) -> PyResult<Option<PyObject>> {
        match self {
            MNode::Bitmap(n) => bitmap_find(n, py, shift, hash, key),
            MNode::Array(n) => array_find(n, py, shift, hash, key),
            MNode::Collision(n) => collision_find(n, py, key),
        }
    }

    pub fn find_or_default(
        &self,
        py: Python<'_>,
        shift: u32,
        hash: i32,
        key: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        match self.find(py, shift, hash, key)? {
            Some(v) => Ok(v),
            None => Ok(default),
        }
    }

    /// True if `key` is present anywhere under this node.
    pub fn contains_key(
        &self,
        py: Python<'_>,
        shift: u32,
        hash: i32,
        key: PyObject,
    ) -> PyResult<bool> {
        Ok(self.find(py, shift, hash, key)?.is_some())
    }

    // --- assoc (returns (new_node, added_leaf)) ---

    pub fn assoc(
        self: &Arc<MNode>,
        py: Python<'_>,
        shift: u32,
        hash: i32,
        key: PyObject,
        val: PyObject,
    ) -> PyResult<(Arc<MNode>, bool)> {
        match &**self {
            MNode::Bitmap(n) => bitmap_assoc(n, py, shift, hash, key, val),
            MNode::Array(n) => array_assoc(n, py, shift, hash, key, val),
            MNode::Collision(n) => collision_assoc(n, py, shift, hash, key, val),
        }
    }

    // --- without (returns (Option<new_node>, removed)): None == empty ---

    pub fn without(
        self: &Arc<MNode>,
        py: Python<'_>,
        shift: u32,
        hash: i32,
        key: PyObject,
    ) -> PyResult<(Option<Arc<MNode>>, bool)> {
        match &**self {
            MNode::Bitmap(n) => bitmap_without(n, py, shift, hash, key),
            MNode::Array(n) => array_without(n, py, shift, hash, key),
            MNode::Collision(n) => collision_without(n, py, key),
        }
    }

    // --- assoc_editable (transient in-place mutation) ---

    pub fn assoc_editable(
        self: &Arc<MNode>,
        py: Python<'_>,
        edit: &Arc<AtomicUsize>,
        shift: u32,
        hash: i32,
        key: PyObject,
        val: PyObject,
    ) -> PyResult<(Arc<MNode>, bool)> {
        match &**self {
            MNode::Bitmap(_) => bitmap_assoc_editable(self, py, edit, shift, hash, key, val),
            MNode::Array(_) => array_assoc_editable(self, py, edit, shift, hash, key, val),
            MNode::Collision(_) => collision_assoc_editable(self, py, edit, shift, hash, key, val),
        }
    }

    // --- without_editable (transient in-place mutation) ---

    pub fn without_editable(
        self: &Arc<MNode>,
        py: Python<'_>,
        edit: &Arc<AtomicUsize>,
        shift: u32,
        hash: i32,
        key: PyObject,
    ) -> PyResult<Option<Arc<MNode>>> {
        match &**self {
            MNode::Bitmap(_) => bitmap_without_editable(self, py, edit, shift, hash, key),
            MNode::Array(_) => array_without_editable(self, py, edit, shift, hash, key),
            MNode::Collision(_) => collision_without_editable(self, py, edit, key),
        }
    }

    /// Walk the node, pushing every (key, value) pair into `out`.
    pub fn collect_entries(&self, py: Python<'_>, out: &mut Vec<(PyObject, PyObject)>) {
        match self {
            MNode::Bitmap(n) => {
                let g = n.inner.lock();
                for e in &g.array {
                    match e {
                        MEntry::KV { key, val } => {
                            out.push((key.clone_ref(py), val.clone_ref(py)))
                        }
                        MEntry::Child { node } => node.collect_entries(py, out),
                    }
                }
            }
            MNode::Array(n) => {
                let g = n.inner.lock();
                for slot in &g.array {
                    if let Some(child) = slot {
                        child.collect_entries(py, out);
                    }
                }
            }
            MNode::Collision(n) => {
                let g = n.inner.lock();
                for (k, v) in &g.entries {
                    out.push((k.clone_ref(py), v.clone_ref(py)));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BitmapIndexedNode ops
// ---------------------------------------------------------------------------

fn bitmap_find(
    n: &BitmapIndexedNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<Option<PyObject>> {
    let bit = bitpos(hash, shift);
    let g = n.inner.lock();
    if (g.bitmap & bit) == 0 {
        return Ok(None);
    }
    let idx = bitmap_index(g.bitmap, bit);
    match &g.array[idx] {
        MEntry::Child { node } => {
            let child = Arc::clone(node);
            drop(g);
            child.find(py, shift + 5, hash, key)
        }
        MEntry::KV { key: k, val } => {
            let k_cl = k.clone_ref(py);
            let v_cl = val.clone_ref(py);
            drop(g);
            if crate::rt::equiv(py, k_cl, key)? {
                Ok(Some(v_cl))
            } else {
                Ok(None)
            }
        }
    }
}

fn bitmap_assoc(
    n: &BitmapIndexedNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<MNode>, bool)> {
    let bit = bitpos(hash, shift);
    let (bitmap, idx, slot_set, slot_variant) = {
        let g = n.inner.lock();
        let bitmap = g.bitmap;
        let idx = bitmap_index(bitmap, bit);
        let slot_set = (bitmap & bit) != 0;
        let slot_variant = if slot_set {
            Some(match &g.array[idx] {
                MEntry::KV { key: k, val: v } => SlotVariant::KV(k.clone_ref(py), v.clone_ref(py)),
                MEntry::Child { node } => SlotVariant::Child(Arc::clone(node)),
            })
        } else {
            None
        };
        (bitmap, idx, slot_set, slot_variant)
    };
    if slot_set {
        match slot_variant.unwrap() {
            SlotVariant::Child(child) => {
                let (new_child, added) = child.assoc(py, shift + 5, hash, key, val)?;
                if Arc::ptr_eq(&new_child, &child) {
                    return Ok((clone_bitmap_arc(n, py), added));
                }
                let new_array = {
                    let g = n.inner.lock();
                    clone_and_set_bitmap_entry(
                        &g.array,
                        py,
                        idx,
                        MEntry::Child { node: new_child },
                    )
                };
                Ok((
                    Arc::new(MNode::Bitmap(BitmapIndexedNode {
                        inner: Mutex::new(BitmapIndexedInner {
                            bitmap,
                            array: new_array,
                        }),
                        edit: None,
                    })),
                    added,
                ))
            }
            SlotVariant::KV(existing_key, existing_val) => {
                if crate::rt::equiv(py, existing_key.clone_ref(py), key.clone_ref(py))? {
                    let new_array = {
                        let g = n.inner.lock();
                        clone_and_set_bitmap_entry(
                            &g.array,
                            py,
                            idx,
                            MEntry::KV {
                                key: existing_key,
                                val,
                            },
                        )
                    };
                    let _ = existing_val;
                    Ok((
                        Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            inner: Mutex::new(BitmapIndexedInner {
                                bitmap,
                                array: new_array,
                            }),
                            edit: None,
                        })),
                        false,
                    ))
                } else {
                    let subtree = create_node_from_two(
                        py,
                        shift + 5,
                        existing_key,
                        existing_val,
                        hash,
                        key,
                        val,
                    )?;
                    let new_array = {
                        let g = n.inner.lock();
                        clone_and_set_bitmap_entry(
                            &g.array,
                            py,
                            idx,
                            MEntry::Child { node: subtree },
                        )
                    };
                    Ok((
                        Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            inner: Mutex::new(BitmapIndexedInner {
                                bitmap,
                                array: new_array,
                            }),
                            edit: None,
                        })),
                        true,
                    ))
                }
            }
        }
    } else {
        // slot empty; insert
        let n_entries = bitmap.count_ones() as usize;
        if n_entries >= 16 {
            // promote to ArrayNode — snapshot entries first to avoid holding
            // the lock while dispatching back into Python for hashing.
            let entries_snapshot: Vec<MEntry> = {
                let g = n.inner.lock();
                g.array.iter().map(|e| e.clone_entry(py)).collect()
            };
            let mut nodes: Vec<Option<Arc<MNode>>> = (0..32).map(|_| None).collect();
            let jdx = mask(hash, shift) as usize;
            let (leaf_node, _added) =
                MNode::empty_bitmap().assoc(py, shift + 5, hash, key, val)?;
            nodes[jdx] = Some(leaf_node);
            let mut j = 0usize;
            for i in 0..32u32 {
                if ((bitmap >> i) & 1) != 0 {
                    match &entries_snapshot[j] {
                        MEntry::Child { node } => {
                            nodes[i as usize] = Some(Arc::clone(node));
                        }
                        MEntry::KV { key: k, val: v } => {
                            let kh =
                                fold_hash_i64(crate::rt::hash_eq(py, k.clone_ref(py))?);
                            let (subnode, _) = MNode::empty_bitmap().assoc(
                                py,
                                shift + 5,
                                kh,
                                k.clone_ref(py),
                                v.clone_ref(py),
                            )?;
                            nodes[i as usize] = Some(subnode);
                        }
                    }
                    j += 1;
                }
            }
            Ok((
                Arc::new(MNode::Array(ArrayNode {
                    inner: Mutex::new(ArrayInner {
                        count: (n_entries + 1) as u32,
                        array: nodes,
                    }),
                    edit: None,
                })),
                true,
            ))
        } else {
            // insert in bitmap, grow vec by one.
            let g = n.inner.lock();
            let mut new_array: Vec<MEntry> = Vec::with_capacity(n_entries + 1);
            for i in 0..idx {
                new_array.push(g.array[i].clone_entry(py));
            }
            new_array.push(MEntry::KV { key, val });
            for i in idx..n_entries {
                new_array.push(g.array[i].clone_entry(py));
            }
            Ok((
                Arc::new(MNode::Bitmap(BitmapIndexedNode {
                    inner: Mutex::new(BitmapIndexedInner {
                        bitmap: bitmap | bit,
                        array: new_array,
                    }),
                    edit: None,
                })),
                true,
            ))
        }
    }
}

enum SlotVariant {
    KV(PyObject, PyObject),
    Child(Arc<MNode>),
}

fn bitmap_without(
    n: &BitmapIndexedNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<(Option<Arc<MNode>>, bool)> {
    let bit = bitpos(hash, shift);
    let (bitmap, idx, slot_variant): (u32, usize, Option<SlotVariant>) = {
        let g = n.inner.lock();
        let bitmap = g.bitmap;
        if (bitmap & bit) == 0 {
            (bitmap, 0, None)
        } else {
            let idx = bitmap_index(bitmap, bit);
            let variant = match &g.array[idx] {
                MEntry::KV { key: k, val: v } => SlotVariant::KV(k.clone_ref(py), v.clone_ref(py)),
                MEntry::Child { node } => SlotVariant::Child(Arc::clone(node)),
            };
            (bitmap, idx, Some(variant))
        }
    };
    let Some(slot_variant) = slot_variant else {
        return Ok((Some(clone_bitmap_arc(n, py)), false));
    };
    match slot_variant {
        SlotVariant::Child(child) => {
            let (new_child_opt, removed) = child.without(py, shift + 5, hash, key)?;
            match new_child_opt {
                Some(new_child) if Arc::ptr_eq(&new_child, &child) => {
                    Ok((Some(clone_bitmap_arc(n, py)), false))
                }
                Some(new_child) => {
                    let new_array = {
                        let g = n.inner.lock();
                        clone_and_set_bitmap_entry(
                            &g.array,
                            py,
                            idx,
                            MEntry::Child { node: new_child },
                        )
                    };
                    Ok((
                        Some(Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            inner: Mutex::new(BitmapIndexedInner {
                                bitmap,
                                array: new_array,
                            }),
                            edit: None,
                        }))),
                        removed,
                    ))
                }
                None => {
                    if bitmap == bit {
                        return Ok((None, removed));
                    }
                    let new_array = {
                        let g = n.inner.lock();
                        remove_bitmap_entry(&g.array, py, idx)
                    };
                    Ok((
                        Some(Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            inner: Mutex::new(BitmapIndexedInner {
                                bitmap: bitmap ^ bit,
                                array: new_array,
                            }),
                            edit: None,
                        }))),
                        removed,
                    ))
                }
            }
        }
        SlotVariant::KV(k, _v) => {
            if crate::rt::equiv(py, k, key)? {
                if bitmap == bit {
                    return Ok((None, true));
                }
                let new_array = {
                    let g = n.inner.lock();
                    remove_bitmap_entry(&g.array, py, idx)
                };
                Ok((
                    Some(Arc::new(MNode::Bitmap(BitmapIndexedNode {
                        inner: Mutex::new(BitmapIndexedInner {
                            bitmap: bitmap ^ bit,
                            array: new_array,
                        }),
                        edit: None,
                    }))),
                    true,
                ))
            } else {
                Ok((Some(clone_bitmap_arc(n, py)), false))
            }
        }
    }
}

fn clone_bitmap_arc(n: &BitmapIndexedNode, py: Python<'_>) -> Arc<MNode> {
    let g = n.inner.lock();
    Arc::new(MNode::Bitmap(BitmapIndexedNode {
        inner: Mutex::new(BitmapIndexedInner {
            bitmap: g.bitmap,
            array: g.array.iter().map(|e| e.clone_entry(py)).collect(),
        }),
        edit: None,
    }))
}

fn clone_and_set_bitmap_entry(
    array: &[MEntry],
    py: Python<'_>,
    idx: usize,
    new_entry: MEntry,
) -> Vec<MEntry> {
    let mut out: Vec<MEntry> = Vec::with_capacity(array.len());
    for e in array.iter() {
        out.push(e.clone_entry(py));
    }
    out[idx] = new_entry;
    out
}

fn remove_bitmap_entry(array: &[MEntry], py: Python<'_>, idx: usize) -> Vec<MEntry> {
    let mut out: Vec<MEntry> = Vec::with_capacity(array.len() - 1);
    for (i, e) in array.iter().enumerate() {
        if i == idx {
            continue;
        }
        out.push(e.clone_entry(py));
    }
    out
}

/// `createNode(shift, key1, val1, key2hash, key2, val2)` — build a node that
/// holds two entries. Mirrors the Java helper.
fn create_node_from_two(
    py: Python<'_>,
    shift: u32,
    key1: PyObject,
    val1: PyObject,
    key2hash: i32,
    key2: PyObject,
    val2: PyObject,
) -> PyResult<Arc<MNode>> {
    let key1hash = fold_hash_i64(crate::rt::hash_eq(py, key1.clone_ref(py))?);
    if key1hash == key2hash {
        return Ok(Arc::new(MNode::Collision(HashCollisionNode {
            inner: Mutex::new(CollisionInner {
                hash: key1hash,
                entries: vec![(key1, val1), (key2, val2)],
            }),
            edit: None,
        })));
    }
    let (n1, _) = MNode::empty_bitmap().assoc(py, shift, key1hash, key1, val1)?;
    let (n2, _) = n1.assoc(py, shift, key2hash, key2, val2)?;
    Ok(n2)
}

// ---------------------------------------------------------------------------
// ArrayNode ops
// ---------------------------------------------------------------------------

fn array_find(
    n: &ArrayNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<Option<PyObject>> {
    let idx = mask(hash, shift) as usize;
    let child_opt: Option<Arc<MNode>> = {
        let g = n.inner.lock();
        g.array[idx].as_ref().map(Arc::clone)
    };
    match child_opt {
        None => Ok(None),
        Some(child) => child.find(py, shift + 5, hash, key),
    }
}

fn array_assoc(
    n: &ArrayNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<MNode>, bool)> {
    let idx = mask(hash, shift) as usize;
    let (count, child_opt, slots_cloned) = {
        let g = n.inner.lock();
        let slots: Vec<Option<Arc<MNode>>> =
            g.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect();
        (g.count, g.array[idx].as_ref().map(Arc::clone), slots)
    };
    match child_opt {
        None => {
            let (new_child, _added) =
                MNode::empty_bitmap().assoc(py, shift + 5, hash, key, val)?;
            let mut new_array = slots_cloned;
            new_array[idx] = Some(new_child);
            Ok((
                Arc::new(MNode::Array(ArrayNode {
                    inner: Mutex::new(ArrayInner {
                        count: count + 1,
                        array: new_array,
                    }),
                    edit: None,
                })),
                true,
            ))
        }
        Some(child) => {
            let (new_child, added) = child.assoc(py, shift + 5, hash, key, val)?;
            if Arc::ptr_eq(&new_child, &child) {
                return Ok((clone_array_arc(n), added));
            }
            let mut new_array = slots_cloned;
            new_array[idx] = Some(new_child);
            Ok((
                Arc::new(MNode::Array(ArrayNode {
                    inner: Mutex::new(ArrayInner {
                        count,
                        array: new_array,
                    }),
                    edit: None,
                })),
                added,
            ))
        }
    }
}

fn array_without(
    n: &ArrayNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<(Option<Arc<MNode>>, bool)> {
    let idx = mask(hash, shift) as usize;
    let (count, child_opt) = {
        let g = n.inner.lock();
        (g.count, g.array[idx].as_ref().map(Arc::clone))
    };
    match child_opt {
        None => Ok((Some(clone_array_arc(n)), false)),
        Some(child) => {
            let (new_child_opt, removed) = child.without(py, shift + 5, hash, key)?;
            match new_child_opt {
                Some(new_child) if Arc::ptr_eq(&new_child, &child) => {
                    Ok((Some(clone_array_arc(n)), false))
                }
                Some(new_child) => {
                    let mut new_array: Vec<Option<Arc<MNode>>> = {
                        let g = n.inner.lock();
                        g.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect()
                    };
                    new_array[idx] = Some(new_child);
                    Ok((
                        Some(Arc::new(MNode::Array(ArrayNode {
                            inner: Mutex::new(ArrayInner {
                                count,
                                array: new_array,
                            }),
                            edit: None,
                        }))),
                        removed,
                    ))
                }
                None => {
                    if count <= 8 {
                        Ok((Some(pack_array(n, py, idx)?), removed))
                    } else {
                        let mut new_array: Vec<Option<Arc<MNode>>> = {
                            let g = n.inner.lock();
                            g.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect()
                        };
                        new_array[idx] = None;
                        Ok((
                            Some(Arc::new(MNode::Array(ArrayNode {
                                inner: Mutex::new(ArrayInner {
                                    count: count - 1,
                                    array: new_array,
                                }),
                                edit: None,
                            }))),
                            removed,
                        ))
                    }
                }
            }
        }
    }
}

fn clone_array_arc(n: &ArrayNode) -> Arc<MNode> {
    let g = n.inner.lock();
    Arc::new(MNode::Array(ArrayNode {
        inner: Mutex::new(ArrayInner {
            count: g.count,
            array: g.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect(),
        }),
        edit: None,
    }))
}

/// `pack(edit, idx)` — ArrayNode → BitmapIndexedNode after shrinking below 9
/// entries. The subtree at `idx` is the one being removed; it becomes a
/// hole. The new bitmap carries every other non-empty slot's index.
fn pack_array(n: &ArrayNode, _py: Python<'_>, idx: usize) -> PyResult<Arc<MNode>> {
    let g = n.inner.lock();
    let mut bitmap: u32 = 0;
    let mut new_array: Vec<MEntry> = Vec::with_capacity(g.count as usize - 1);
    // pre-idx
    for i in 0..idx {
        if let Some(child) = &g.array[i] {
            new_array.push(MEntry::Child {
                node: Arc::clone(child),
            });
            bitmap |= 1u32 << i;
        }
    }
    // post-idx
    for i in (idx + 1)..g.array.len() {
        if let Some(child) = &g.array[i] {
            new_array.push(MEntry::Child {
                node: Arc::clone(child),
            });
            bitmap |= 1u32 << i;
        }
    }
    Ok(Arc::new(MNode::Bitmap(BitmapIndexedNode {
        inner: Mutex::new(BitmapIndexedInner {
            bitmap,
            array: new_array,
        }),
        edit: None,
    })))
}

// ---------------------------------------------------------------------------
// HashCollisionNode ops
// ---------------------------------------------------------------------------

fn collision_find(
    n: &HashCollisionNode,
    py: Python<'_>,
    key: PyObject,
) -> PyResult<Option<PyObject>> {
    let entries: Vec<(PyObject, PyObject)> = {
        let g = n.inner.lock();
        g.entries.iter().map(|(k, v)| (k.clone_ref(py), v.clone_ref(py))).collect()
    };
    for (k, v) in entries {
        if crate::rt::equiv(py, k, key.clone_ref(py))? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

fn collision_find_index(
    entries: &[(PyObject, PyObject)],
    py: Python<'_>,
    key: &PyObject,
) -> PyResult<Option<usize>> {
    for (i, (k, _)) in entries.iter().enumerate() {
        if crate::rt::equiv(py, k.clone_ref(py), key.clone_ref(py))? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

fn collision_assoc(
    n: &HashCollisionNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<MNode>, bool)> {
    let (n_hash, entries_snapshot) = {
        let g = n.inner.lock();
        (
            g.hash,
            g.entries.iter().map(|(k, v)| (k.clone_ref(py), v.clone_ref(py))).collect::<Vec<_>>(),
        )
    };
    if hash == n_hash {
        if let Some(idx) = collision_find_index(&entries_snapshot, py, &key)? {
            let mut new_entries: Vec<(PyObject, PyObject)> = entries_snapshot
                .iter()
                .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
                .collect();
            new_entries[idx] = (entries_snapshot[idx].0.clone_ref(py), val);
            return Ok((
                Arc::new(MNode::Collision(HashCollisionNode {
                    inner: Mutex::new(CollisionInner {
                        hash: n_hash,
                        entries: new_entries,
                    }),
                    edit: None,
                })),
                false,
            ));
        }
        let mut new_entries: Vec<(PyObject, PyObject)> = entries_snapshot
            .iter()
            .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
            .collect();
        new_entries.push((key, val));
        return Ok((
            Arc::new(MNode::Collision(HashCollisionNode {
                inner: Mutex::new(CollisionInner {
                    hash: n_hash,
                    entries: new_entries,
                }),
                edit: None,
            })),
            true,
        ));
    }
    // Different hash — nest this collision node inside a bitmap node at the
    // parent's slot, then assoc the new k/v into that.
    let bit = bitpos(n_hash, shift);
    let wrapped: Arc<MNode> = Arc::new(MNode::Collision(HashCollisionNode {
        inner: Mutex::new(CollisionInner {
            hash: n_hash,
            entries: entries_snapshot,
        }),
        edit: None,
    }));
    let parent = Arc::new(MNode::Bitmap(BitmapIndexedNode {
        inner: Mutex::new(BitmapIndexedInner {
            bitmap: bit,
            array: vec![MEntry::Child { node: wrapped }],
        }),
        edit: None,
    }));
    parent.assoc(py, shift, hash, key, val)
}

fn collision_without(
    n: &HashCollisionNode,
    py: Python<'_>,
    key: PyObject,
) -> PyResult<(Option<Arc<MNode>>, bool)> {
    let (n_hash, entries_snapshot) = {
        let g = n.inner.lock();
        (
            g.hash,
            g.entries.iter().map(|(k, v)| (k.clone_ref(py), v.clone_ref(py))).collect::<Vec<_>>(),
        )
    };
    match collision_find_index(&entries_snapshot, py, &key)? {
        None => Ok((
            Some(Arc::new(MNode::Collision(HashCollisionNode {
                inner: Mutex::new(CollisionInner {
                    hash: n_hash,
                    entries: entries_snapshot,
                }),
                edit: None,
            }))),
            false,
        )),
        Some(idx) => {
            if entries_snapshot.len() == 1 {
                return Ok((None, true));
            }
            let mut new_entries: Vec<(PyObject, PyObject)> =
                Vec::with_capacity(entries_snapshot.len() - 1);
            for (i, (k, v)) in entries_snapshot.iter().enumerate() {
                if i == idx {
                    continue;
                }
                new_entries.push((k.clone_ref(py), v.clone_ref(py)));
            }
            Ok((
                Some(Arc::new(MNode::Collision(HashCollisionNode {
                    inner: Mutex::new(CollisionInner {
                        hash: n_hash,
                        entries: new_entries,
                    }),
                    edit: None,
                }))),
                true,
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Editable (transient) variants
// ---------------------------------------------------------------------------

/// Port of Java's `BitmapIndexedNode.assoc(AtomicReference<Thread> edit, ...)`.
/// Mutates the bitmap node in place when `self.edit == edit`, else clones.
fn bitmap_assoc_editable(
    this: &Arc<MNode>,
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    shift: u32,
    hash: i32,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<MNode>, bool)> {
    let editable = this.ensure_editable(edit);
    let MNode::Bitmap(n) = &*editable else { unreachable!() };
    let bit = bitpos(hash, shift);
    // Snapshot the slot contents without holding the lock during recursive calls.
    let (bitmap, idx, slot_set, slot_variant) = {
        let g = n.inner.lock();
        let bitmap = g.bitmap;
        let idx = bitmap_index(bitmap, bit);
        let slot_set = (bitmap & bit) != 0;
        let slot_variant = if slot_set {
            Some(match &g.array[idx] {
                MEntry::KV { key: k, val: v } => SlotVariant::KV(k.clone_ref(py), v.clone_ref(py)),
                MEntry::Child { node } => SlotVariant::Child(Arc::clone(node)),
            })
        } else {
            None
        };
        (bitmap, idx, slot_set, slot_variant)
    };
    if slot_set {
        match slot_variant.unwrap() {
            SlotVariant::Child(child) => {
                let (new_child, added) =
                    child.assoc_editable(py, edit, shift + 5, hash, key, val)?;
                if Arc::ptr_eq(&new_child, &child) {
                    return Ok((editable, added));
                }
                {
                    let mut g = n.inner.lock();
                    g.array[idx] = MEntry::Child { node: new_child };
                }
                Ok((editable, added))
            }
            SlotVariant::KV(existing_key, existing_val) => {
                if crate::rt::equiv(py, existing_key.clone_ref(py), key.clone_ref(py))? {
                    // same key — in-place replace value
                    {
                        let mut g = n.inner.lock();
                        g.array[idx] = MEntry::KV {
                            key: existing_key,
                            val,
                        };
                    }
                    let _ = existing_val;
                    Ok((editable, false))
                } else {
                    // hash collision at this slot — split
                    let subtree = create_node_from_two_editable(
                        py,
                        edit,
                        shift + 5,
                        existing_key,
                        existing_val,
                        hash,
                        key,
                        val,
                    )?;
                    {
                        let mut g = n.inner.lock();
                        g.array[idx] = MEntry::Child { node: subtree };
                    }
                    Ok((editable, true))
                }
            }
        }
    } else {
        // Slot empty — insert
        let n_entries = bitmap.count_ones() as usize;
        if n_entries >= 16 {
            // Promote to ArrayNode.
            let mut nodes: Vec<Option<Arc<MNode>>> = (0..32).map(|_| None).collect();
            let jdx = mask(hash, shift) as usize;
            let (leaf_node, _added) = MNode::empty_bitmap().assoc_editable(
                py,
                edit,
                shift + 5,
                hash,
                key,
                val,
            )?;
            nodes[jdx] = Some(leaf_node);
            {
                let g = n.inner.lock();
                let mut j = 0usize;
                for i in 0..32u32 {
                    if ((bitmap >> i) & 1) != 0 {
                        match &g.array[j] {
                            MEntry::Child { node } => {
                                nodes[i as usize] = Some(Arc::clone(node));
                            }
                            MEntry::KV { key: k, val: v } => {
                                let kh = fold_hash_i64(
                                    crate::rt::hash_eq(py, k.clone_ref(py))?,
                                );
                                let (subnode, _) = MNode::empty_bitmap().assoc_editable(
                                    py,
                                    edit,
                                    shift + 5,
                                    kh,
                                    k.clone_ref(py),
                                    v.clone_ref(py),
                                )?;
                                nodes[i as usize] = Some(subnode);
                            }
                        }
                        j += 1;
                    }
                }
            }
            Ok((
                Arc::new(MNode::Array(ArrayNode {
                    inner: Mutex::new(ArrayInner {
                        count: (n_entries + 1) as u32,
                        array: nodes,
                    }),
                    edit: Some(Arc::clone(edit)),
                })),
                true,
            ))
        } else {
            // In-place insert in the editable bitmap node.
            let mut g = n.inner.lock();
            g.array.insert(idx, MEntry::KV { key, val });
            g.bitmap = bitmap | bit;
            drop(g);
            Ok((editable, true))
        }
    }
}

/// Port of Java's `BitmapIndexedNode.without(AtomicReference<Thread> edit, ...)`.
fn bitmap_without_editable(
    this: &Arc<MNode>,
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<Option<Arc<MNode>>> {
    // Before taking the editable clone, check if key is even present at this node.
    let bit = bitpos(hash, shift);
    let MNode::Bitmap(n_ro) = &**this else { unreachable!() };
    let (bitmap, idx, slot_set, slot_variant) = {
        let g = n_ro.inner.lock();
        let bitmap = g.bitmap;
        if (bitmap & bit) == 0 {
            return Ok(Some(Arc::clone(this)));
        }
        let idx = bitmap_index(bitmap, bit);
        let variant = match &g.array[idx] {
            MEntry::KV { key: k, val: v } => SlotVariant::KV(k.clone_ref(py), v.clone_ref(py)),
            MEntry::Child { node } => SlotVariant::Child(Arc::clone(node)),
        };
        (bitmap, idx, true, variant)
    };
    let _ = slot_set;
    match slot_variant {
        SlotVariant::Child(child) => {
            let new_child_opt = child.without_editable(py, edit, shift + 5, hash, key)?;
            match new_child_opt {
                Some(new_child) if Arc::ptr_eq(&new_child, &child) => Ok(Some(Arc::clone(this))),
                Some(new_child) => {
                    let editable = this.ensure_editable(edit);
                    let MNode::Bitmap(n) = &*editable else { unreachable!() };
                    let mut g = n.inner.lock();
                    g.array[idx] = MEntry::Child { node: new_child };
                    drop(g);
                    Ok(Some(editable))
                }
                None => {
                    if bitmap == bit {
                        return Ok(None);
                    }
                    let editable = this.ensure_editable(edit);
                    let MNode::Bitmap(n) = &*editable else { unreachable!() };
                    let mut g = n.inner.lock();
                    g.array.remove(idx);
                    g.bitmap = bitmap ^ bit;
                    drop(g);
                    Ok(Some(editable))
                }
            }
        }
        SlotVariant::KV(k, _v) => {
            if crate::rt::equiv(py, k, key)? {
                if bitmap == bit {
                    return Ok(None);
                }
                let editable = this.ensure_editable(edit);
                let MNode::Bitmap(n) = &*editable else { unreachable!() };
                let mut g = n.inner.lock();
                g.array.remove(idx);
                g.bitmap = bitmap ^ bit;
                drop(g);
                Ok(Some(editable))
            } else {
                Ok(Some(Arc::clone(this)))
            }
        }
    }
}

fn array_assoc_editable(
    this: &Arc<MNode>,
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    shift: u32,
    hash: i32,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<MNode>, bool)> {
    let editable = this.ensure_editable(edit);
    let MNode::Array(n) = &*editable else { unreachable!() };
    let idx = mask(hash, shift) as usize;
    let child_opt: Option<Arc<MNode>> = {
        let g = n.inner.lock();
        g.array[idx].as_ref().map(Arc::clone)
    };
    match child_opt {
        None => {
            let (new_child, _added) = MNode::empty_bitmap().assoc_editable(
                py,
                edit,
                shift + 5,
                hash,
                key,
                val,
            )?;
            let mut g = n.inner.lock();
            g.array[idx] = Some(new_child);
            g.count += 1;
            drop(g);
            Ok((editable, true))
        }
        Some(child) => {
            let (new_child, added) =
                child.assoc_editable(py, edit, shift + 5, hash, key, val)?;
            if Arc::ptr_eq(&new_child, &child) {
                return Ok((editable, added));
            }
            let mut g = n.inner.lock();
            g.array[idx] = Some(new_child);
            drop(g);
            Ok((editable, added))
        }
    }
}

fn array_without_editable(
    this: &Arc<MNode>,
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<Option<Arc<MNode>>> {
    let MNode::Array(n_ro) = &**this else { unreachable!() };
    let idx = mask(hash, shift) as usize;
    let (count, child_opt) = {
        let g = n_ro.inner.lock();
        (g.count, g.array[idx].as_ref().map(Arc::clone))
    };
    let Some(child) = child_opt else { return Ok(Some(Arc::clone(this))); };
    let new_child_opt = child.without_editable(py, edit, shift + 5, hash, key)?;
    match new_child_opt {
        Some(new_child) if Arc::ptr_eq(&new_child, &child) => Ok(Some(Arc::clone(this))),
        Some(new_child) => {
            let editable = this.ensure_editable(edit);
            let MNode::Array(n) = &*editable else { unreachable!() };
            let mut g = n.inner.lock();
            g.array[idx] = Some(new_child);
            drop(g);
            Ok(Some(editable))
        }
        None => {
            if count <= 8 {
                // Pack — build a fresh bitmap node; edit-stamped.
                Ok(Some(pack_array_editable(n_ro, py, edit, idx)?))
            } else {
                let editable = this.ensure_editable(edit);
                let MNode::Array(n) = &*editable else { unreachable!() };
                let mut g = n.inner.lock();
                g.array[idx] = None;
                g.count = count - 1;
                drop(g);
                Ok(Some(editable))
            }
        }
    }
}

fn pack_array_editable(
    n: &ArrayNode,
    _py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    idx: usize,
) -> PyResult<Arc<MNode>> {
    let g = n.inner.lock();
    let mut bitmap: u32 = 0;
    let mut new_array: Vec<MEntry> = Vec::with_capacity(g.count as usize - 1);
    for i in 0..idx {
        if let Some(child) = &g.array[i] {
            new_array.push(MEntry::Child { node: Arc::clone(child) });
            bitmap |= 1u32 << i;
        }
    }
    for i in (idx + 1)..g.array.len() {
        if let Some(child) = &g.array[i] {
            new_array.push(MEntry::Child { node: Arc::clone(child) });
            bitmap |= 1u32 << i;
        }
    }
    Ok(Arc::new(MNode::Bitmap(BitmapIndexedNode {
        inner: Mutex::new(BitmapIndexedInner {
            bitmap,
            array: new_array,
        }),
        edit: Some(Arc::clone(edit)),
    })))
}

fn collision_assoc_editable(
    this: &Arc<MNode>,
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    shift: u32,
    hash: i32,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<MNode>, bool)> {
    let MNode::Collision(n_ro) = &**this else { unreachable!() };
    let (n_hash, entries_snapshot) = {
        let g = n_ro.inner.lock();
        (
            g.hash,
            g.entries.iter().map(|(k, v)| (k.clone_ref(py), v.clone_ref(py))).collect::<Vec<_>>(),
        )
    };
    if hash == n_hash {
        if let Some(idx) = collision_find_index(&entries_snapshot, py, &key)? {
            // Same key — in-place value update on editable node.
            let editable = this.ensure_editable(edit);
            let MNode::Collision(n) = &*editable else { unreachable!() };
            let mut g = n.inner.lock();
            g.entries[idx] = (entries_snapshot[idx].0.clone_ref(py), val);
            drop(g);
            Ok((editable, false))
        } else {
            // New colliding entry — append on editable node.
            let editable = this.ensure_editable(edit);
            let MNode::Collision(n) = &*editable else { unreachable!() };
            let mut g = n.inner.lock();
            g.entries.push((key, val));
            drop(g);
            Ok((editable, true))
        }
    } else {
        // Different hash — nest under a new bitmap node and recurse.
        let bit = bitpos(n_hash, shift);
        let wrapped: Arc<MNode> = Arc::new(MNode::Collision(HashCollisionNode {
            inner: Mutex::new(CollisionInner {
                hash: n_hash,
                entries: entries_snapshot,
            }),
            edit: Some(Arc::clone(edit)),
        }));
        let parent = Arc::new(MNode::Bitmap(BitmapIndexedNode {
            inner: Mutex::new(BitmapIndexedInner {
                bitmap: bit,
                array: vec![MEntry::Child { node: wrapped }],
            }),
            edit: Some(Arc::clone(edit)),
        }));
        parent.assoc_editable(py, edit, shift, hash, key, val)
    }
}

fn collision_without_editable(
    this: &Arc<MNode>,
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    key: PyObject,
) -> PyResult<Option<Arc<MNode>>> {
    let MNode::Collision(n_ro) = &**this else { unreachable!() };
    let entries_snapshot: Vec<(PyObject, PyObject)> = {
        let g = n_ro.inner.lock();
        g.entries.iter().map(|(k, v)| (k.clone_ref(py), v.clone_ref(py))).collect()
    };
    match collision_find_index(&entries_snapshot, py, &key)? {
        None => Ok(Some(Arc::clone(this))),
        Some(idx) => {
            if entries_snapshot.len() == 1 {
                return Ok(None);
            }
            let editable = this.ensure_editable(edit);
            let MNode::Collision(n) = &*editable else { unreachable!() };
            let mut g = n.inner.lock();
            g.entries.remove(idx);
            drop(g);
            Ok(Some(editable))
        }
    }
}

/// Like `create_node_from_two`, but stamps the resulting nodes with `edit`.
fn create_node_from_two_editable(
    py: Python<'_>,
    edit: &Arc<AtomicUsize>,
    shift: u32,
    key1: PyObject,
    val1: PyObject,
    key2hash: i32,
    key2: PyObject,
    val2: PyObject,
) -> PyResult<Arc<MNode>> {
    let key1hash = fold_hash_i64(crate::rt::hash_eq(py, key1.clone_ref(py))?);
    if key1hash == key2hash {
        return Ok(Arc::new(MNode::Collision(HashCollisionNode {
            inner: Mutex::new(CollisionInner {
                hash: key1hash,
                entries: vec![(key1, val1), (key2, val2)],
            }),
            edit: Some(Arc::clone(edit)),
        })));
    }
    // Empty editable bitmap, then two assoc_editable calls.
    let empty = Arc::new(MNode::Bitmap(BitmapIndexedNode {
        inner: Mutex::new(BitmapIndexedInner {
            bitmap: 0,
            array: Vec::new(),
        }),
        edit: Some(Arc::clone(edit)),
    }));
    let (n1, _) = empty.assoc_editable(py, edit, shift, key1hash, key1, val1)?;
    let (n2, _) = n1.assoc_editable(py, edit, shift, key2hash, key2, val2)?;
    Ok(n2)
}
