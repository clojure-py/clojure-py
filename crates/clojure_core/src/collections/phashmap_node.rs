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
//! Persistent-only variant: no `edit` tokens in this phase (Phase 8C transient
//! support will carry an edit token on each node — stubbed here as `None`).

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

pub struct BitmapIndexedNode {
    pub bitmap: u32,
    pub array: Vec<MEntry>,
    #[allow(dead_code)]
    pub edit: Option<Arc<AtomicUsize>>,
}

pub struct ArrayNode {
    pub count: u32,
    pub array: Vec<Option<Arc<MNode>>>, // length == 32
    #[allow(dead_code)]
    pub edit: Option<Arc<AtomicUsize>>,
}

pub struct HashCollisionNode {
    pub hash: i32,
    pub entries: Vec<(PyObject, PyObject)>,
    #[allow(dead_code)]
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
            bitmap: 0,
            array: Vec::new(),
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

    // --- without (returns Option<new_node>: None == empty) ---

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

    /// Walk the node, pushing every (key, value) pair into `out`.
    pub fn collect_entries(&self, py: Python<'_>, out: &mut Vec<(PyObject, PyObject)>) {
        match self {
            MNode::Bitmap(n) => {
                for e in &n.array {
                    match e {
                        MEntry::KV { key, val } => {
                            out.push((key.clone_ref(py), val.clone_ref(py)))
                        }
                        MEntry::Child { node } => node.collect_entries(py, out),
                    }
                }
            }
            MNode::Array(n) => {
                for slot in &n.array {
                    if let Some(child) = slot {
                        child.collect_entries(py, out);
                    }
                }
            }
            MNode::Collision(n) => {
                for (k, v) in &n.entries {
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
    if (n.bitmap & bit) == 0 {
        return Ok(None);
    }
    let idx = bitmap_index(n.bitmap, bit);
    match &n.array[idx] {
        MEntry::Child { node } => node.find(py, shift + 5, hash, key),
        MEntry::KV { key: k, val } => {
            if crate::rt::equiv(py, k.clone_ref(py), key)? {
                Ok(Some(val.clone_ref(py)))
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
    let idx = bitmap_index(n.bitmap, bit);
    if (n.bitmap & bit) != 0 {
        match &n.array[idx] {
            MEntry::Child { node } => {
                let (new_child, added) = node.assoc(py, shift + 5, hash, key, val)?;
                if Arc::ptr_eq(&new_child, node) {
                    // no change — return an identical node
                    return Ok((clone_bitmap_arc(n, py), added));
                }
                let new_array = clone_and_set_entry(
                    &n.array,
                    py,
                    idx,
                    MEntry::Child { node: new_child },
                );
                Ok((
                    Arc::new(MNode::Bitmap(BitmapIndexedNode {
                        bitmap: n.bitmap,
                        array: new_array,
                        edit: None,
                    })),
                    added,
                ))
            }
            MEntry::KV { key: existing_key, val: existing_val } => {
                if crate::rt::equiv(py, existing_key.clone_ref(py), key.clone_ref(py))? {
                    // same key — replace value
                    let new_array = clone_and_set_entry(
                        &n.array,
                        py,
                        idx,
                        MEntry::KV {
                            key: existing_key.clone_ref(py),
                            val: val,
                        },
                    );
                    // (if val is identical we still allocate a new node — matches
                    // an upstream check the JVM does with `==` identity; in Python
                    // object identity is different, so we always produce a new node.)
                    let _ = existing_val;
                    Ok((
                        Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            bitmap: n.bitmap,
                            array: new_array,
                            edit: None,
                        })),
                        false,
                    ))
                } else {
                    // hash collision at this slot — split the KV leaf into a subtree.
                    let subtree = create_node_from_two(
                        py,
                        shift + 5,
                        existing_key.clone_ref(py),
                        existing_val.clone_ref(py),
                        hash,
                        key,
                        val,
                    )?;
                    let new_array = clone_and_set_entry(
                        &n.array,
                        py,
                        idx,
                        MEntry::Child { node: subtree },
                    );
                    Ok((
                        Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            bitmap: n.bitmap,
                            array: new_array,
                            edit: None,
                        })),
                        true,
                    ))
                }
            }
        }
    } else {
        // slot empty; insert
        let n_entries = n.bitmap.count_ones() as usize;
        if n_entries >= 16 {
            // promote to ArrayNode
            let mut nodes: Vec<Option<Arc<MNode>>> = (0..32).map(|_| None).collect();
            let jdx = mask(hash, shift) as usize;
            let (leaf_node, _added) = MNode::empty_bitmap().assoc(py, shift + 5, hash, key, val)?;
            nodes[jdx] = Some(leaf_node);
            let mut j = 0usize;
            for i in 0..32u32 {
                if ((n.bitmap >> i) & 1) != 0 {
                    match &n.array[j] {
                        MEntry::Child { node } => {
                            nodes[i as usize] = Some(Arc::clone(node));
                        }
                        MEntry::KV { key: k, val: v } => {
                            // Re-insert this KV one level down.
                            let kh = fold_hash_i64(crate::rt::hash_eq(py, k.clone_ref(py))?);
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
                    count: (n_entries + 1) as u32,
                    array: nodes,
                    edit: None,
                })),
                true,
            ))
        } else {
            // insert in bitmap, grow vec by one.
            let mut new_array: Vec<MEntry> = Vec::with_capacity(n_entries + 1);
            for i in 0..idx {
                new_array.push(n.array[i].clone_entry(py));
            }
            new_array.push(MEntry::KV { key, val });
            for i in idx..n_entries {
                new_array.push(n.array[i].clone_entry(py));
            }
            Ok((
                Arc::new(MNode::Bitmap(BitmapIndexedNode {
                    bitmap: n.bitmap | bit,
                    array: new_array,
                    edit: None,
                })),
                true,
            ))
        }
    }
}

fn bitmap_without(
    n: &BitmapIndexedNode,
    py: Python<'_>,
    shift: u32,
    hash: i32,
    key: PyObject,
) -> PyResult<(Option<Arc<MNode>>, bool)> {
    let bit = bitpos(hash, shift);
    if (n.bitmap & bit) == 0 {
        return Ok((Some(clone_bitmap_arc(n, py)), false));
    }
    let idx = bitmap_index(n.bitmap, bit);
    match &n.array[idx] {
        MEntry::Child { node } => {
            let (new_child_opt, removed) = node.without(py, shift + 5, hash, key)?;
            match new_child_opt {
                Some(new_child) if Arc::ptr_eq(&new_child, node) => {
                    Ok((Some(clone_bitmap_arc(n, py)), false))
                }
                Some(new_child) => {
                    let new_array = clone_and_set_entry(
                        &n.array,
                        py,
                        idx,
                        MEntry::Child { node: new_child },
                    );
                    Ok((
                        Some(Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            bitmap: n.bitmap,
                            array: new_array,
                            edit: None,
                        }))),
                        removed,
                    ))
                }
                None => {
                    if n.bitmap == bit {
                        return Ok((None, removed));
                    }
                    let new_array = remove_entry(&n.array, py, idx);
                    Ok((
                        Some(Arc::new(MNode::Bitmap(BitmapIndexedNode {
                            bitmap: n.bitmap ^ bit,
                            array: new_array,
                            edit: None,
                        }))),
                        removed,
                    ))
                }
            }
        }
        MEntry::KV { key: k, .. } => {
            if crate::rt::equiv(py, k.clone_ref(py), key)? {
                if n.bitmap == bit {
                    return Ok((None, true));
                }
                let new_array = remove_entry(&n.array, py, idx);
                Ok((
                    Some(Arc::new(MNode::Bitmap(BitmapIndexedNode {
                        bitmap: n.bitmap ^ bit,
                        array: new_array,
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
    Arc::new(MNode::Bitmap(BitmapIndexedNode {
        bitmap: n.bitmap,
        array: n.array.iter().map(|e| e.clone_entry(py)).collect(),
        edit: None,
    }))
}

fn clone_and_set_entry(
    array: &[MEntry],
    py: Python<'_>,
    idx: usize,
    new_entry: MEntry,
) -> Vec<MEntry> {
    let mut out: Vec<MEntry> = Vec::with_capacity(array.len());
    for (i, e) in array.iter().enumerate() {
        if i == idx {
            // We'll push the replacement below; placeholder uses a dummy.
            out.push(e.clone_entry(py));
        } else {
            out.push(e.clone_entry(py));
        }
    }
    out[idx] = new_entry;
    out
}

fn remove_entry(array: &[MEntry], py: Python<'_>, idx: usize) -> Vec<MEntry> {
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
            hash: key1hash,
            entries: vec![(key1, val1), (key2, val2)],
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
    match &n.array[idx] {
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
    match &n.array[idx] {
        None => {
            let (new_child, _added) = MNode::empty_bitmap().assoc(py, shift + 5, hash, key, val)?;
            let mut new_array: Vec<Option<Arc<MNode>>> =
                n.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect();
            new_array[idx] = Some(new_child);
            Ok((
                Arc::new(MNode::Array(ArrayNode {
                    count: n.count + 1,
                    array: new_array,
                    edit: None,
                })),
                true,
            ))
        }
        Some(child) => {
            let (new_child, added) = child.assoc(py, shift + 5, hash, key, val)?;
            if Arc::ptr_eq(&new_child, child) {
                return Ok((clone_array_arc(n), added));
            }
            let mut new_array: Vec<Option<Arc<MNode>>> =
                n.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect();
            new_array[idx] = Some(new_child);
            Ok((
                Arc::new(MNode::Array(ArrayNode {
                    count: n.count,
                    array: new_array,
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
    match &n.array[idx] {
        None => Ok((Some(clone_array_arc(n)), false)),
        Some(child) => {
            let (new_child_opt, removed) = child.without(py, shift + 5, hash, key)?;
            match new_child_opt {
                Some(new_child) if Arc::ptr_eq(&new_child, child) => {
                    Ok((Some(clone_array_arc(n)), false))
                }
                Some(new_child) => {
                    let mut new_array: Vec<Option<Arc<MNode>>> =
                        n.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect();
                    new_array[idx] = Some(new_child);
                    Ok((
                        Some(Arc::new(MNode::Array(ArrayNode {
                            count: n.count,
                            array: new_array,
                            edit: None,
                        }))),
                        removed,
                    ))
                }
                None => {
                    if n.count <= 8 {
                        // pack into BitmapIndexedNode
                        Ok((Some(pack_array(n, py, idx)?), removed))
                    } else {
                        let mut new_array: Vec<Option<Arc<MNode>>> =
                            n.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect();
                        new_array[idx] = None;
                        Ok((
                            Some(Arc::new(MNode::Array(ArrayNode {
                                count: n.count - 1,
                                array: new_array,
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
    Arc::new(MNode::Array(ArrayNode {
        count: n.count,
        array: n.array.iter().map(|o| o.as_ref().map(Arc::clone)).collect(),
        edit: None,
    }))
}

/// `pack(edit, idx)` — ArrayNode → BitmapIndexedNode after shrinking below 9
/// entries. The subtree at `idx` is the one being removed; it becomes a
/// hole. The new bitmap carries every other non-empty slot's index.
fn pack_array(n: &ArrayNode, _py: Python<'_>, idx: usize) -> PyResult<Arc<MNode>> {
    let mut bitmap: u32 = 0;
    let mut new_array: Vec<MEntry> = Vec::with_capacity(n.count as usize - 1);
    // pre-idx
    for i in 0..idx {
        if let Some(child) = &n.array[i] {
            new_array.push(MEntry::Child {
                node: Arc::clone(child),
            });
            bitmap |= 1u32 << i;
        }
    }
    // post-idx
    for i in (idx + 1)..n.array.len() {
        if let Some(child) = &n.array[i] {
            new_array.push(MEntry::Child {
                node: Arc::clone(child),
            });
            bitmap |= 1u32 << i;
        }
    }
    Ok(Arc::new(MNode::Bitmap(BitmapIndexedNode {
        bitmap,
        array: new_array,
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
    for (k, v) in &n.entries {
        if crate::rt::equiv(py, k.clone_ref(py), key.clone_ref(py))? {
            return Ok(Some(v.clone_ref(py)));
        }
    }
    Ok(None)
}

fn collision_find_index(n: &HashCollisionNode, py: Python<'_>, key: &PyObject) -> PyResult<Option<usize>> {
    for (i, (k, _)) in n.entries.iter().enumerate() {
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
    if hash == n.hash {
        if let Some(idx) = collision_find_index(n, py, &key)? {
            // replace val
            let mut new_entries: Vec<(PyObject, PyObject)> = n
                .entries
                .iter()
                .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
                .collect();
            new_entries[idx] = (n.entries[idx].0.clone_ref(py), val);
            return Ok((
                Arc::new(MNode::Collision(HashCollisionNode {
                    hash: n.hash,
                    entries: new_entries,
                    edit: None,
                })),
                false,
            ));
        }
        // append new entry
        let mut new_entries: Vec<(PyObject, PyObject)> = n
            .entries
            .iter()
            .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
            .collect();
        new_entries.push((key, val));
        return Ok((
            Arc::new(MNode::Collision(HashCollisionNode {
                hash: n.hash,
                entries: new_entries,
                edit: None,
            })),
            true,
        ));
    }
    // Different hash — nest this collision node inside a bitmap node at the
    // parent's slot, then assoc the new k/v into that.
    let bit = bitpos(n.hash, shift);
    // Clone the collision node as an Arc<MNode> to wrap in the new bitmap.
    let wrapped: Arc<MNode> = Arc::new(MNode::Collision(HashCollisionNode {
        hash: n.hash,
        entries: n
            .entries
            .iter()
            .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
            .collect(),
        edit: None,
    }));
    let parent = Arc::new(MNode::Bitmap(BitmapIndexedNode {
        bitmap: bit,
        array: vec![MEntry::Child { node: wrapped }],
        edit: None,
    }));
    parent.assoc(py, shift, hash, key, val)
}

fn collision_without(
    n: &HashCollisionNode,
    py: Python<'_>,
    key: PyObject,
) -> PyResult<(Option<Arc<MNode>>, bool)> {
    match collision_find_index(n, py, &key)? {
        None => Ok((
            Some(Arc::new(MNode::Collision(HashCollisionNode {
                hash: n.hash,
                entries: n
                    .entries
                    .iter()
                    .map(|(k, v)| (k.clone_ref(py), v.clone_ref(py)))
                    .collect(),
                edit: None,
            }))),
            false,
        )),
        Some(idx) => {
            if n.entries.len() == 1 {
                return Ok((None, true));
            }
            let mut new_entries: Vec<(PyObject, PyObject)> = Vec::with_capacity(n.entries.len() - 1);
            for (i, (k, v)) in n.entries.iter().enumerate() {
                if i == idx {
                    continue;
                }
                new_entries.push((k.clone_ref(py), v.clone_ref(py)));
            }
            Ok((
                Some(Arc::new(MNode::Collision(HashCollisionNode {
                    hash: n.hash,
                    entries: new_entries,
                    edit: None,
                }))),
                true,
            ))
        }
    }
}
