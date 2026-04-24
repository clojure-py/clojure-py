//! PersistentVector — 32-way HAMT bit-partitioned trie + 32-element tail.
//!
//! Port of clojure/lang/PersistentVector.java. Transient + protocol impls
//! land in Phase 6B/6C.

use crate::associative::Associative;
use crate::ilookup::ILookup;
use crate::coll_reduce::CollReduce;
use crate::collections::pvector_node::{VNode, VSlot};
use crate::counted::Counted;
use crate::exceptions::IllegalStateException;
use crate::iequiv::IEquiv;
use crate::ieditable_collection::IEditableCollection;
use crate::ifn::IFn;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::indexed::Indexed;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_stack::IPersistentStack;
use crate::ipersistent_vector::IPersistentVector;
use crate::iseqable::ISeqable;
use crate::reversible::Reversible;
use crate::itransient_associative::ITransientAssociative;
use crate::itransient_collection::ITransientCollection;
use crate::itransient_vector::ITransientVector;
use crate::sequential::Sequential;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

type PyObject = Py<PyAny>;

const BITS: u32 = 5;
const BRANCH: usize = 32;
const MASK: usize = 31;

#[pyclass(module = "clojure._core", name = "PersistentVector", frozen)]
pub struct PersistentVector {
    pub cnt: u32,
    pub shift: u32,
    pub root: Arc<VNode>,
    pub tail: Arc<[PyObject]>,
    pub meta: Option<PyObject>,
}

static EMPTY_ROOT: once_cell::sync::OnceCell<Arc<VNode>> = once_cell::sync::OnceCell::new();

fn empty_root() -> Arc<VNode> {
    EMPTY_ROOT.get_or_init(VNode::empty).clone()
}

impl PersistentVector {
    pub fn new_empty() -> Self {
        Self {
            cnt: 0,
            shift: BITS,
            root: empty_root(),
            tail: Arc::from(Vec::<PyObject>::new().into_boxed_slice()),
            meta: None,
        }
    }

    /// Tail offset — index at which the tail starts (= cnt - tail.len()).
    fn tail_off(&self) -> usize {
        if (self.cnt as usize) < BRANCH {
            0
        } else {
            ((self.cnt as usize - 1) >> BITS) << BITS
        }
    }

    /// Retrieve the 32-element chunk containing index `i`.
    pub fn array_for(&self, py: Python<'_>, i: usize) -> PyResult<Arc<[PyObject]>> {
        if i >= self.cnt as usize {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "index {i} out of bounds for vector of length {}",
                self.cnt
            )));
        }
        if i >= self.tail_off() {
            return Ok(Arc::clone(&self.tail));
        }
        // Walk trie from root.
        let mut node = Arc::clone(&self.root);
        let mut level = self.shift;
        while level > 0 {
            let idx = (i >> level) & MASK;
            let next: Arc<VNode> = {
                let g = node.array.lock();
                match &g[idx] {
                    VSlot::Branch(b) => Arc::clone(b),
                    _ => {
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "vector trie walk hit non-branch at interior level",
                        ))
                    }
                }
            };
            node = next;
            level -= BITS;
        }
        // At leaf level — collect 32 slots into an Arc<[PyObject]>.
        let g = node.array.lock();
        let vec: Vec<PyObject> = (0..BRANCH)
            .map(|j| match &g[j] {
                VSlot::Leaf(v) => v.clone_ref(py),
                _ => py.None(),
            })
            .collect();
        Ok(Arc::from(vec.into_boxed_slice()))
    }

    fn nth_internal(&self, py: Python<'_>, i: usize) -> PyResult<PyObject> {
        let arr = self.array_for(py, i)?;
        Ok(arr[i & MASK].clone_ref(py))
    }

    /// Public wrapper around `nth_internal` for use by sibling modules
    /// (e.g. `seqs::vector_seq`). Semantics are identical to `nth_internal`.
    pub fn nth_internal_pub(&self, py: Python<'_>, i: usize) -> PyResult<PyObject> {
        self.nth_internal(py, i)
    }

    /// Append `x`, returning a new vector.
    pub fn conj_internal(&self, py: Python<'_>, x: PyObject) -> PyResult<Self> {
        if (self.cnt as usize - self.tail_off()) < BRANCH {
            // Tail has room.
            let mut new_tail: Vec<PyObject> = self.tail.iter().map(|o| o.clone_ref(py)).collect();
            new_tail.push(x);
            return Ok(Self {
                cnt: self.cnt + 1,
                shift: self.shift,
                root: Arc::clone(&self.root),
                tail: Arc::from(new_tail.into_boxed_slice()),
                meta: self.meta.as_ref().map(|o| o.clone_ref(py)),
            });
        }
        // Tail is full. Push it into the trie as a leaf node.
        let tail_node = {
            let leaves: [VSlot; 32] =
                std::array::from_fn(|j| VSlot::Leaf(self.tail[j].clone_ref(py)));
            Arc::new(VNode {
                edit: None,
                array: parking_lot::Mutex::new(leaves),
            })
        };
        let (new_root, new_shift) = if ((self.cnt as usize) >> BITS) > (1usize << self.shift) {
            // Root overflow — grow one level.
            let mut slots: [VSlot; 32] = std::array::from_fn(|_| VSlot::Empty);
            slots[0] = VSlot::Branch(Arc::clone(&self.root));
            slots[1] = VSlot::Branch(new_path(self.shift, tail_node));
            let new_root = Arc::new(VNode {
                edit: None,
                array: parking_lot::Mutex::new(slots),
            });
            (new_root, self.shift + BITS)
        } else {
            let new_root = push_tail(self.cnt as usize, self.shift, &self.root, tail_node);
            (new_root, self.shift)
        };
        Ok(Self {
            cnt: self.cnt + 1,
            shift: new_shift,
            root: new_root,
            tail: Arc::from(vec![x].into_boxed_slice()),
            meta: self.meta.as_ref().map(|o| o.clone_ref(py)),
        })
    }

    /// Replace the element at index `i` with `x` (or append at i == cnt).
    pub fn assoc_n_internal(&self, py: Python<'_>, i: usize, x: PyObject) -> PyResult<Self> {
        if i == self.cnt as usize {
            return self.conj_internal(py, x);
        }
        if i >= self.cnt as usize {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "assoc-n index {i} out of bounds for vector of length {}",
                self.cnt
            )));
        }
        if i >= self.tail_off() {
            // In tail: clone tail + replace.
            let mut new_tail: Vec<PyObject> = self.tail.iter().map(|o| o.clone_ref(py)).collect();
            new_tail[i & MASK] = x;
            return Ok(Self {
                cnt: self.cnt,
                shift: self.shift,
                root: Arc::clone(&self.root),
                tail: Arc::from(new_tail.into_boxed_slice()),
                meta: self.meta.as_ref().map(|o| o.clone_ref(py)),
            });
        }
        // In trie: path-copy from root.
        let new_root = do_assoc(self.shift, &self.root, i, x);
        Ok(Self {
            cnt: self.cnt,
            shift: self.shift,
            root: new_root,
            tail: Arc::clone(&self.tail),
            meta: self.meta.as_ref().map(|o| o.clone_ref(py)),
        })
    }

    /// Remove last element.
    pub fn pop_internal(&self, py: Python<'_>) -> PyResult<Self> {
        if self.cnt == 0 {
            return Err(IllegalStateException::new_err("Can't pop empty vector"));
        }
        if self.cnt == 1 {
            // Preserve meta across the transition to empty (matches vanilla).
            let mut empty = Self::new_empty();
            empty.meta = self.meta.as_ref().map(|o| o.clone_ref(py));
            return Ok(empty);
        }
        if (self.cnt as usize - self.tail_off()) > 1 {
            // Tail has more than one element.
            let new_tail: Vec<PyObject> = self.tail[..self.tail.len() - 1]
                .iter()
                .map(|o| o.clone_ref(py))
                .collect();
            return Ok(Self {
                cnt: self.cnt - 1,
                shift: self.shift,
                root: Arc::clone(&self.root),
                tail: Arc::from(new_tail.into_boxed_slice()),
                meta: self.meta.as_ref().map(|o| o.clone_ref(py)),
            });
        }
        // Tail has exactly one element. Pull the last leaf from trie into new tail.
        let new_tail = self.array_for(py, self.cnt as usize - 2)?;
        let new_root_opt = pop_tail(self.cnt as usize, self.shift, &self.root);
        let mut new_shift = self.shift;
        let mut new_root = new_root_opt.unwrap_or_else(empty_root);

        // Collapse root if it has a single child at index 0 (matches Java's
        // one-step check: `shift > 5 && newroot.array[1] == null`).
        if new_shift > BITS {
            let collapse_child = {
                let g = new_root.array.lock();
                let slot1_empty = matches!(&g[1], VSlot::Empty);
                if slot1_empty {
                    match &g[0] {
                        VSlot::Branch(b) => Some(Arc::clone(b)),
                        _ => None,
                    }
                } else {
                    None
                }
            };
            if let Some(child) = collapse_child {
                new_root = child;
                new_shift -= BITS;
            }
        }
        Ok(Self {
            cnt: self.cnt - 1,
            shift: new_shift,
            root: new_root,
            tail: new_tail,
            meta: self.meta.as_ref().map(|o| o.clone_ref(py)),
        })
    }
}

// --- Helpers. ---

/// Build a path of empty nodes down to level 0, with `node` at the bottom.
fn new_path(level: u32, node: Arc<VNode>) -> Arc<VNode> {
    if level == 0 {
        return node;
    }
    let mut slots: [VSlot; 32] = std::array::from_fn(|_| VSlot::Empty);
    slots[0] = VSlot::Branch(new_path(level - BITS, node));
    Arc::new(VNode {
        edit: None,
        array: parking_lot::Mutex::new(slots),
    })
}

/// Push `tail_node` (32-leaf node) into the trie at the correct position.
///
/// `cnt` is the FULL vector count BEFORE the conj that triggered this push
/// (matches Java's `cnt - 1` subidx arithmetic where `cnt` is the field on
/// PersistentVector at cons time — i.e. the count prior to the new element).
fn push_tail(cnt: usize, level: u32, parent: &Arc<VNode>, tail_node: Arc<VNode>) -> Arc<VNode> {
    let subidx = ((cnt - 1) >> level) & MASK;
    let mut new_array = parent.clone_array();
    let new_child = if level == BITS {
        tail_node
    } else {
        let existing = {
            let g = parent.array.lock();
            match &g[subidx] {
                VSlot::Branch(b) => Some(Arc::clone(b)),
                _ => None,
            }
        };
        match existing {
            Some(child) => push_tail(cnt, level - BITS, &child, tail_node),
            None => new_path(level - BITS, tail_node),
        }
    };
    new_array[subidx] = VSlot::Branch(new_child);
    Arc::new(VNode {
        edit: None,
        array: parking_lot::Mutex::new(new_array),
    })
}

/// Path-copy assoc: walk down to level 0, replace the leaf, rebuild up.
fn do_assoc(level: u32, node: &Arc<VNode>, i: usize, x: PyObject) -> Arc<VNode> {
    let mut new_array = node.clone_array();
    if level == 0 {
        new_array[i & MASK] = VSlot::Leaf(x);
    } else {
        let subidx = (i >> level) & MASK;
        let child = {
            let g = node.array.lock();
            match &g[subidx] {
                VSlot::Branch(b) => Arc::clone(b),
                _ => {
                    return Arc::new(VNode {
                        edit: None,
                        array: parking_lot::Mutex::new(new_array),
                    })
                }
            }
        };
        new_array[subidx] = VSlot::Branch(do_assoc(level - BITS, &child, i, x));
    }
    Arc::new(VNode {
        edit: None,
        array: parking_lot::Mutex::new(new_array),
    })
}

/// Pop tail: returns new root without the last leaf node. Returns None if the
/// root becomes empty.
///
/// `cnt` is the vector count BEFORE the pop (so the last element's index is
/// `cnt - 1`, and the second-to-last's chunk lives at subidx = `((cnt-2) >> level) & MASK`).
fn pop_tail(cnt: usize, level: u32, node: &Arc<VNode>) -> Option<Arc<VNode>> {
    let subidx = ((cnt - 2) >> level) & MASK;
    if level > BITS {
        let child = {
            let g = node.array.lock();
            match &g[subidx] {
                VSlot::Branch(b) => Arc::clone(b),
                _ => return None,
            }
        };
        let new_child = pop_tail(cnt, level - BITS, &child);
        if new_child.is_none() && subidx == 0 {
            None
        } else {
            let mut new_array = node.clone_array();
            new_array[subidx] = match new_child {
                Some(c) => VSlot::Branch(c),
                None => VSlot::Empty,
            };
            Some(Arc::new(VNode {
                edit: None,
                array: parking_lot::Mutex::new(new_array),
            }))
        }
    } else if subidx == 0 {
        None
    } else {
        let mut new_array = node.clone_array();
        new_array[subidx] = VSlot::Empty;
        Some(Arc::new(VNode {
            edit: None,
            array: parking_lot::Mutex::new(new_array),
        }))
    }
}

// --- Python-facing methods. ---

#[pymethods]
impl PersistentVector {
    fn __len__(&self) -> usize {
        self.cnt as usize
    }
    fn __bool__(&self) -> bool {
        self.cnt > 0
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentVectorIter>> {
        Py::new(py, PersistentVectorIter { vec: slf, pos: 0 })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __getitem__(&self, py: Python<'_>, i: isize) -> PyResult<PyObject> {
        let idx = if i < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
        } else {
            i as usize
        };
        self.nth_internal(py, idx)
    }

    fn __contains__(&self, py: Python<'_>, item: PyObject) -> PyResult<bool> {
        // O(n) membership check via rt::equiv.
        for i in 0..(self.cnt as usize) {
            let e = self.nth_internal(py, i)?;
            if crate::rt::equiv(py, e, item.clone_ref(py))? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut parts = Vec::with_capacity(self.cnt as usize);
        for i in 0..(self.cnt as usize) {
            let e = self.nth_internal(py, i)?;
            parts.push(e.bind(py).repr()?.extract::<String>()?);
        }
        Ok(format!("[{}]", parts.join(" ")))
    }
    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.__repr__(py)
    }

    #[pyo3(signature = (i, /))]
    fn nth(&self, py: Python<'_>, i: isize) -> PyResult<PyObject> {
        if i < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "index {i} out of bounds for vector of length {}",
                self.cnt
            )));
        }
        self.nth_internal(py, i as usize)
    }

    #[pyo3(signature = (i, default, /))]
    fn nth_or_default(&self, py: Python<'_>, i: isize, default: PyObject) -> PyResult<PyObject> {
        if i < 0 || (i as usize) >= self.cnt as usize {
            return Ok(default);
        }
        self.nth_internal(py, i as usize)
    }

    fn conj(&self, py: Python<'_>, x: PyObject) -> PyResult<Py<PersistentVector>> {
        let new = self.conj_internal(py, x)?;
        Py::new(py, new)
    }

    #[pyo3(signature = (i, x, /))]
    fn assoc_n(&self, py: Python<'_>, i: isize, x: PyObject) -> PyResult<Py<PersistentVector>> {
        if i < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "assoc-n index {i} out of bounds for vector of length {}",
                self.cnt
            )));
        }
        let new = self.assoc_n_internal(py, i as usize, x)?;
        Py::new(py, new)
    }

    fn pop(&self, py: Python<'_>) -> PyResult<Py<PersistentVector>> {
        let new = self.pop_internal(py)?;
        Py::new(py, new)
    }

    /// `(v i)` / `(v i default)` — vector-as-IFn: behaves like nth.
    #[pyo3(signature = (*args))]
    fn __call__(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        match args.len() {
            1 => {
                let a0 = args.get_item(0)?;
                let i = a0.extract::<i64>().map_err(|_| {
                    crate::exceptions::IllegalArgumentException::new_err("Vector index must be an integer")
                })?;
                if i < 0 {
                    return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
                }
                self.nth_internal(py, i as usize)
            }
            2 => {
                let a0 = args.get_item(0)?;
                let a1 = args.get_item(1)?.unbind();
                let Ok(i) = a0.extract::<i64>() else { return Ok(a1); };
                if i < 0 { return Ok(a1); }
                if (i as u64) >= (self.cnt as u64) { return Ok(a1); }
                self.nth_internal(py, i as usize)
            }
            n => Err(crate::exceptions::ArityException::new_err(format!(
                "Wrong number of args ({n}) passed to: PersistentVector"
            ))),
        }
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }
}

#[pyclass(module = "clojure._core", name = "PersistentVectorIter")]
pub struct PersistentVectorIter {
    vec: Py<PersistentVector>,
    pos: u32,
}

#[pymethods]
impl PersistentVectorIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let v = self.vec.bind(py).get();
        if self.pos >= v.cnt {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        let item = v.nth_internal(py, self.pos as usize)?;
        self.pos += 1;
        Ok(item)
    }
}

// --- Python-facing constructor. ---

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn vector(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<Py<PersistentVector>> {
    let mut v = PersistentVector::new_empty();
    for i in 0..args.len() {
        v = v.conj_internal(py, args.get_item(i)?.unbind())?;
    }
    Py::new(py, v)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentVector>()?;
    m.add_class::<PersistentVectorIter>()?;
    m.add_class::<TransientVector>()?;
    m.add_function(wrap_pyfunction!(vector, m)?)?;
    m.add_function(wrap_pyfunction!(transient_fn, m)?)?;
    Ok(())
}

// --- Protocol impls (Phase 6B). ---

#[implements(Counted)]
impl Counted for PersistentVector {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().cnt as usize)
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentVector {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        // Same-type fast path: walk by index without going through seq.
        let other_b = other.bind(py);
        if let Ok(other_pv) = other_b.cast::<PersistentVector>() {
            let a = this.bind(py).get();
            let b = other_pv.get();
            if a.cnt != b.cnt { return Ok(false); }
            for i in 0..(a.cnt as usize) {
                let av = a.nth_internal(py, i)?;
                let bv = b.nth_internal(py, i)?;
                if !crate::rt::equiv(py, av, bv)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        // Cross-type: vectors are Sequential — equal to any Sequential
        // with the same elements in the same order (e.g. lists, lazy seqs).
        if !crate::rt::is_sequential(py, &other) {
            return Ok(false);
        }
        crate::rt::sequential_equiv(py, this.into_any(), other)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentVector {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        let s = this.bind(py).get();
        let mut h: i64 = 1;
        for i in 0..(s.cnt as usize) {
            let v = s.nth_internal(py, i)?;
            let eh = crate::rt::hash_eq(py, v)?;
            h = h.wrapping_mul(31).wrapping_add(eh);
        }
        Ok(h)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentVector {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Ok(Py::new(py, PersistentVector {
            cnt: s.cnt,
            shift: s.shift,
            root: Arc::clone(&s.root),
            tail: Arc::clone(&s.tail),
            meta: m,
        })?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentVector {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().cnt as usize)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.conj_internal(py, x)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn empty(_this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(Py::new(py, PersistentVector::new_empty())?.into_any())
    }
}

#[implements(IPersistentVector)]
impl IPersistentVector for PersistentVector {
    fn length(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().cnt as usize)
    }
    fn assoc_n(this: Py<Self>, py: Python<'_>, i: PyObject, x: PyObject) -> PyResult<PyObject> {
        let idx = i.bind(py).extract::<i64>().map_err(|_| {
            crate::exceptions::IllegalArgumentException::new_err("vector index must be integer")
        })?;
        if idx < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
        }
        let s = this.bind(py).get();
        let new = s.assoc_n_internal(py, idx as usize, x)?;
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(Indexed)]
impl Indexed for PersistentVector {
    fn nth(this: Py<Self>, py: Python<'_>, i: PyObject) -> PyResult<PyObject> {
        let idx = i.bind(py).extract::<i64>().map_err(|_| {
            crate::exceptions::IllegalArgumentException::new_err("index must be integer")
        })?;
        if idx < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
        }
        this.bind(py).get().nth_internal(py, idx as usize)
    }
    fn nth_or_default(this: Py<Self>, py: Python<'_>, i: PyObject, default: PyObject) -> PyResult<PyObject> {
        let idx = match i.bind(py).extract::<i64>() {
            Ok(v) => v,
            Err(_) => return Ok(default),
        };
        if idx < 0 { return Ok(default); }
        let s = this.bind(py).get();
        if (idx as u64) >= (s.cnt as u64) { return Ok(default); }
        s.nth_internal(py, idx as usize)
    }
}

#[implements(IPersistentStack)]
impl IPersistentStack for PersistentVector {
    fn peek(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.cnt == 0 { return Ok(py.None()); }
        s.nth_internal(py, (s.cnt - 1) as usize)
    }
    fn pop(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.pop_internal(py)?;
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(Associative)]
impl Associative for PersistentVector {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        let s = this.bind(py).get();
        // For vector, k must be an integer in [0, cnt).
        let Ok(i) = k.bind(py).extract::<i64>() else { return Ok(false); };
        Ok(i >= 0 && (i as u64) < (s.cnt as u64))
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let Ok(i) = k.bind(py).extract::<i64>() else { return Ok(py.None()); };
        if i < 0 || (i as u64) >= (s.cnt as u64) { return Ok(py.None()); }
        // Return a (key, value) tuple as MapEntry stand-in until Phase 7.
        let v = s.nth_internal(py, i as usize)?;
        Ok(pyo3::types::PyTuple::new(py, &[k, v])?.unbind().into_any())
    }
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        let i = k.bind(py).extract::<i64>().map_err(|_| {
            crate::exceptions::IllegalArgumentException::new_err("Vector key must be an integer")
        })?;
        if i < 0 {
            return Err(crate::exceptions::IllegalArgumentException::new_err("Vector index out of bounds"));
        }
        let s = this.bind(py).get();
        let new = s.assoc_n_internal(py, i as usize, v)?;
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(Sequential)]
impl Sequential for PersistentVector {}

#[implements(CollReduce)]
impl CollReduce for PersistentVector {
    fn coll_reduce1(this: Py<Self>, py: Python<'_>, f: PyObject) -> PyResult<PyObject> {
        let v: &PersistentVector = this.bind(py).get();
        if v.cnt == 0 {
            return crate::rt::invoke_n(py, f, &[]);
        }
        let mut acc: Option<PyObject> = None;
        let mut i: usize = 0;
        let total = v.cnt as usize;
        while i < total {
            let arr = v.array_for(py, i)?;
            for x in arr.iter() {
                let val = x.clone_ref(py);
                acc = Some(match acc {
                    None => val,
                    Some(a) => {
                        let next = crate::rt::invoke_n(py, f.clone_ref(py), &[a, val])?;
                        if crate::reduced::is_reduced(py, &next) {
                            return Ok(crate::reduced::unreduced(py, next));
                        }
                        next
                    }
                });
            }
            i += arr.len();
        }
        Ok(acc.unwrap_or_else(|| py.None()))
    }
    fn coll_reduce2(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let v: &PersistentVector = this.bind(py).get();
        let mut acc = init;
        let mut i: usize = 0;
        let total = v.cnt as usize;
        while i < total {
            let arr = v.array_for(py, i)?;
            for x in arr.iter() {
                acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, x.clone_ref(py)])?;
                if crate::reduced::is_reduced(py, &acc) {
                    return Ok(crate::reduced::unreduced(py, acc));
                }
            }
            i += arr.len();
        }
        Ok(acc)
    }
}

#[implements(ISeqable)]
impl ISeqable for PersistentVector {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        if this.bind(py).get().cnt == 0 {
            return Ok(py.None());
        }
        let vs = crate::seqs::vector_seq::VectorSeq {
            vec: this,
            i: 0,
            meta: None,
        };
        Ok(Py::new(py, vs)?.into_any())
    }
}

#[implements(Reversible)]
impl Reversible for PersistentVector {
    fn rseq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let cnt = this.bind(py).get().cnt;
        if cnt == 0 {
            return Ok(py.None());
        }
        // Matches JVM APersistentVector.RSeq: O(1) construct, O(1) per step
        // via nth backward.
        let rs = crate::seqs::vector_rseq::VectorRSeq {
            vec: this,
            i: cnt - 1,
            meta: None,
        };
        Ok(Py::new(py, rs)?.into_any())
    }
}

#[implements(IFn)]
impl IFn for PersistentVector {
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        // (v i) — nth
        let i = a0.bind(py).extract::<i64>().map_err(|_| {
            crate::exceptions::IllegalArgumentException::new_err("Vector index must be an integer")
        })?;
        if i < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
        }
        this.bind(py).get().nth_internal(py, i as usize)
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        // (v i default) — nth-or-default
        let i_res = a0.bind(py).extract::<i64>();
        let Ok(i) = i_res else { return Ok(a1); };
        if i < 0 { return Ok(a1); }
        let s = this.bind(py).get();
        if (i as u64) >= (s.cnt as u64) { return Ok(a1); }
        s.nth_internal(py, i as usize)
    }
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        match args.len() {
            1 => {
                let a0 = args.get_item(0)?.unbind();
                Self::invoke1(this, py, a0)
            }
            2 => {
                let a0 = args.get_item(0)?.unbind();
                let a1 = args.get_item(1)?.unbind();
                Self::invoke2(this, py, a0, a1)
            }
            n => Err(crate::exceptions::ArityException::new_err(format!(
                "Wrong number of args ({n}) passed to: PersistentVector"
            ))),
        }
    }
}

// ============================================================================
// TransientVector (Phase 6C)
// ============================================================================
//
// Mutable-in-place variant of PersistentVector. Each transient carries an
// `edit` token (a unique Arc<AtomicUsize>) and stamps every node it has
// taken ownership of with that token. Operations mutate nodes whose edit
// matches (fast-path), and clone otherwise (defensive path, preserves any
// previously-published persistent snapshots).
//
// Safety:
//   - `alive: AtomicBool` guards against use-after-`persistent!`.
//   - Thread ownership is NOT enforced, matching Clojure JVM post-CLJ-1613:
//     callers are responsible for their own synchronization when handing a
//     transient between threads (e.g. via `future`'s `@deref` happens-before).

#[pyclass(module = "clojure._core", name = "TransientVector", frozen)]
pub struct TransientVector {
    state: parking_lot::Mutex<TransientVectorState>,
    alive: AtomicBool,
    edit: Arc<AtomicUsize>,
}

struct TransientVectorState {
    cnt: u32,
    shift: u32,
    root: Arc<VNode>,
    /// Mutable tail (Vec instead of Arc<[_]>). Capacity is kept at BRANCH so
    /// conj_bang pushes don't need to reallocate until a trie flush.
    tail: Vec<PyObject>,
}

impl TransientVector {
    fn check_alive_and_owner(&self) -> PyResult<()> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(IllegalStateException::new_err(
                "Transient used after persistent!",
            ));
        }
        Ok(())
    }

    fn from_persistent(py: Python<'_>, v: &PersistentVector) -> Self {
        let edit = Arc::new(AtomicUsize::new(1));
        let mut tail: Vec<PyObject> = v.tail.iter().map(|o| o.clone_ref(py)).collect();
        if tail.capacity() < BRANCH {
            tail.reserve(BRANCH - tail.len());
        }
        Self {
            state: parking_lot::Mutex::new(TransientVectorState {
                cnt: v.cnt,
                shift: v.shift,
                root: v.root.editable_clone(Arc::clone(&edit)),
                tail,
            }),
            alive: AtomicBool::new(true),
            edit,
        }
    }
}

fn t_tail_off(st: &TransientVectorState) -> usize {
    if (st.cnt as usize) < BRANCH {
        0
    } else {
        ((st.cnt as usize - 1) >> BITS) << BITS
    }
}

/// Transient counterpart of `new_path`: builds a chain of empty nodes, each
/// stamped with `edit`, terminating at `node` at level 0.
fn new_path_editable(level: u32, node: Arc<VNode>, edit: &Arc<AtomicUsize>) -> Arc<VNode> {
    if level == 0 {
        return node;
    }
    let mut slots: [VSlot; 32] = std::array::from_fn(|_| VSlot::Empty);
    slots[0] = VSlot::Branch(new_path_editable(level - BITS, node, edit));
    Arc::new(VNode {
        edit: Some(Arc::clone(edit)),
        array: parking_lot::Mutex::new(slots),
    })
}

/// Push tail (as a leaf-carrying VNode) into the trie with in-place mutation
/// where edit tokens permit. `cnt` is the transient's count *before* the conj
/// that triggered the flush (mirrors Java's `cnt - 1` subidx arithmetic).
fn push_tail_editable(
    level: u32,
    parent: &Arc<VNode>,
    tail_node: Arc<VNode>,
    edit: &Arc<AtomicUsize>,
    cnt: usize,
) -> Arc<VNode> {
    let subidx = ((cnt - 1) >> level) & MASK;
    let new_parent = parent.ensure_editable(edit);
    let new_child = if level == BITS {
        tail_node
    } else {
        let existing = {
            let g = parent.array.lock();
            match &g[subidx] {
                VSlot::Branch(b) => Some(Arc::clone(b)),
                _ => None,
            }
        };
        match existing {
            Some(child) => push_tail_editable(level - BITS, &child, tail_node, edit, cnt),
            None => new_path_editable(level - BITS, tail_node, edit),
        }
    };
    new_parent.array.lock()[subidx] = VSlot::Branch(new_child);
    new_parent
}

/// Path-copy-or-mutate assoc for transients. Descends the trie, ensuring each
/// node on the path is editable before setting the leaf.
fn do_assoc_editable(
    level: u32,
    node: &Arc<VNode>,
    i: usize,
    x: PyObject,
    edit: &Arc<AtomicUsize>,
) -> Arc<VNode> {
    let new_node = node.ensure_editable(edit);
    if level == 0 {
        new_node.array.lock()[i & MASK] = VSlot::Leaf(x);
    } else {
        let subidx = (i >> level) & MASK;
        let child_opt: Option<Arc<VNode>> = {
            let g = new_node.array.lock();
            match &g[subidx] {
                VSlot::Branch(b) => Some(Arc::clone(b)),
                _ => None,
            }
        };
        if let Some(child) = child_opt {
            let new_child = do_assoc_editable(level - BITS, &child, i, x, edit);
            new_node.array.lock()[subidx] = VSlot::Branch(new_child);
        }
    }
    new_node
}

impl TransientVector {
    /// Index-lookup helper mirroring PersistentVector::nth_internal. Reads
    /// from the live transient state (tail + trie).
    pub(crate) fn t_nth_internal(&self, py: Python<'_>, i: usize) -> PyResult<PyObject> {
        let st = self.state.lock();
        if i >= st.cnt as usize {
            return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "transient vector index {i} out of bounds (len {})",
                st.cnt
            )));
        }
        let tail_off = t_tail_off(&st);
        if i >= tail_off {
            return Ok(st.tail[i - tail_off].clone_ref(py));
        }
        // Walk the trie by shift bits.
        let mut node = Arc::clone(&st.root);
        let mut level = st.shift;
        while level > 0 {
            let idx = (i >> level) & MASK;
            let next: Arc<VNode> = {
                let g = node.array.lock();
                match &g[idx] {
                    VSlot::Branch(b) => Arc::clone(b),
                    _ => {
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "transient trie walk hit non-branch at interior level",
                        ));
                    }
                }
            };
            node = next;
            level -= BITS;
        }
        let g = node.array.lock();
        match &g[i & MASK] {
            VSlot::Leaf(v) => Ok(v.clone_ref(py)),
            _ => Ok(py.None()),
        }
    }

    pub(crate) fn t_count(&self) -> usize {
        self.state.lock().cnt as usize
    }
}

#[pymethods]
impl TransientVector {
    fn __len__(&self) -> PyResult<usize> {
        self.check_alive_and_owner()?;
        Ok(self.state.lock().cnt as usize)
    }

    /// `(contains? tv i)` / `i in tv` — vector contains? is index-valid.
    fn __contains__(&self, py: Python<'_>, key: PyObject) -> PyResult<bool> {
        self.check_alive_and_owner()?;
        let _ = py;
        let Ok(idx) = key.bind(py).extract::<i64>() else { return Ok(false); };
        if idx < 0 { return Ok(false); }
        Ok((idx as usize) < self.t_count())
    }

    fn __getitem__(&self, py: Python<'_>, i: isize) -> PyResult<PyObject> {
        self.check_alive_and_owner()?;
        if i < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
        }
        self.t_nth_internal(py, i as usize)
    }

    fn conj_bang(slf: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<Py<Self>> {
        {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let mut st = this.state.lock();
            let tail_room = (st.cnt as usize) - t_tail_off(&st);
            if tail_room < BRANCH {
                st.tail.push(x);
                st.cnt += 1;
            } else {
                // Tail is full. Flush it into the trie as a leaf-carrying VNode.
                let tail_leaves: [VSlot; 32] = std::array::from_fn(|j| {
                    if j < st.tail.len() {
                        VSlot::Leaf(st.tail[j].clone_ref(py))
                    } else {
                        VSlot::Empty
                    }
                });
                let tail_node = Arc::new(VNode {
                    edit: Some(Arc::clone(&this.edit)),
                    array: parking_lot::Mutex::new(tail_leaves),
                });
                let mut new_root = Arc::clone(&st.root);
                let mut new_shift = st.shift;
                if ((st.cnt as usize) >> BITS) > (1usize << st.shift) {
                    // Root overflow — grow one level.
                    let mut slots: [VSlot; 32] = std::array::from_fn(|_| VSlot::Empty);
                    slots[0] = VSlot::Branch(new_root);
                    slots[1] = VSlot::Branch(new_path_editable(st.shift, tail_node, &this.edit));
                    new_root = Arc::new(VNode {
                        edit: Some(Arc::clone(&this.edit)),
                        array: parking_lot::Mutex::new(slots),
                    });
                    new_shift += BITS;
                } else {
                    let cnt_now = st.cnt as usize;
                    new_root = push_tail_editable(new_shift, &new_root, tail_node, &this.edit, cnt_now);
                }
                st.root = new_root;
                st.shift = new_shift;
                // Start a fresh tail containing the new element.
                let mut new_tail: Vec<PyObject> = Vec::with_capacity(BRANCH);
                new_tail.push(x);
                st.tail = new_tail;
                st.cnt += 1;
            }
            drop(st);
        }
        Ok(slf)
    }

    fn assoc_bang(slf: Py<Self>, py: Python<'_>, i: PyObject, x: PyObject) -> PyResult<Py<Self>> {
        let idx = {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let idx = i.bind(py).extract::<i64>().map_err(|_| {
                crate::exceptions::IllegalArgumentException::new_err("index must be integer")
            })?;
            if idx < 0 {
                return Err(pyo3::exceptions::PyIndexError::new_err("negative index"));
            }
            idx as usize
        };
        // Handle the append case *outside* the state lock so conj_bang can re-acquire.
        {
            let this = slf.bind(py).get();
            let cnt = this.state.lock().cnt as usize;
            if idx == cnt {
                drop(this);
                return Self::conj_bang(slf, py, x);
            }
            if idx > cnt {
                return Err(pyo3::exceptions::PyIndexError::new_err(format!(
                    "index {idx} out of bounds for transient vector of length {cnt}"
                )));
            }
        }
        let this = slf.bind(py).get();
        let mut st = this.state.lock();
        if idx >= t_tail_off(&st) {
            let off = idx - t_tail_off(&st);
            st.tail[off] = x;
        } else {
            let new_root = do_assoc_editable(st.shift, &st.root, idx, x, &this.edit);
            st.root = new_root;
        }
        drop(st);
        Ok(slf)
    }

    fn pop_bang(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<Self>> {
        {
            let this = slf.bind(py).get();
            this.check_alive_and_owner()?;
            let mut st = this.state.lock();
            if st.cnt == 0 {
                return Err(IllegalStateException::new_err("Can't pop empty vector"));
            }
            if st.cnt == 1 {
                st.cnt = 0;
                st.tail.clear();
            } else if (st.cnt as usize - t_tail_off(&st)) > 1 {
                st.tail.pop();
                st.cnt -= 1;
            } else {
                // Tail has exactly one element — pull second-to-last chunk out of trie
                // into the new tail, then collapse the trie.
                let cnt_before = st.cnt as usize;
                let shift_before = st.shift;
                // Build a throwaway persistent snapshot to reuse `array_for`.
                let snapshot_tail: Arc<[PyObject]> = Arc::from(
                    st.tail
                        .iter()
                        .map(|o| o.clone_ref(py))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                );
                let snapshot = PersistentVector {
                    cnt: st.cnt,
                    shift: st.shift,
                    root: Arc::clone(&st.root),
                    tail: snapshot_tail,
                    meta: None,
                };
                let new_tail_arc = snapshot.array_for(py, cnt_before - 2)?;

                let new_root_opt = pop_tail(cnt_before, shift_before, &st.root);
                let mut new_root = new_root_opt.unwrap_or_else(empty_root);
                let mut new_shift = shift_before;

                // Collapse a single-child root (mirrors persistent pop's collapse).
                if new_shift > BITS {
                    let collapse_child = {
                        let g = new_root.array.lock();
                        let slot1_empty = matches!(&g[1], VSlot::Empty);
                        if slot1_empty {
                            match &g[0] {
                                VSlot::Branch(b) => Some(Arc::clone(b)),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    };
                    if let Some(child) = collapse_child {
                        new_root = child;
                        new_shift -= BITS;
                    }
                }
                // Ensure the new root is editable for this transient.
                let new_root_stamped = new_root.ensure_editable(&this.edit);

                st.root = new_root_stamped;
                st.shift = new_shift;
                st.tail = new_tail_arc.iter().map(|o| o.clone_ref(py)).collect();
                if st.tail.capacity() < BRANCH {
                    let extra = BRANCH - st.tail.len();
                    st.tail.reserve(extra);
                }
                st.cnt -= 1;
            }
            drop(st);
        }
        Ok(slf)
    }

    fn persistent_bang(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentVector>> {
        let this = slf.bind(py).get();
        this.check_alive_and_owner()?;
        let st = this.state.lock();
        let pv = PersistentVector {
            cnt: st.cnt,
            shift: st.shift,
            root: Arc::clone(&st.root),
            tail: Arc::from(
                st.tail.iter().map(|o| o.clone_ref(py)).collect::<Vec<_>>().into_boxed_slice(),
            ),
            meta: None,
        };
        drop(st);
        this.alive.store(false, Ordering::Release);
        Py::new(py, pv)
    }
}

// --- Protocol impls ---

#[implements(IEditableCollection)]
impl IEditableCollection for PersistentVector {
    fn as_transient(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let t = TransientVector::from_persistent(py, s);
        Ok(Py::new(py, t)?.into_any())
    }
}

#[implements(ITransientCollection)]
impl ITransientCollection for TransientVector {
    fn conj_bang(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let r = TransientVector::conj_bang(this, py, x)?;
        Ok(r.into_any())
    }
    fn persistent_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let r = TransientVector::persistent_bang(this, py)?;
        Ok(r.into_any())
    }
}

#[implements(ITransientAssociative)]
impl ITransientAssociative for TransientVector {
    fn assoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        let r = TransientVector::assoc_bang(this, py, k, v)?;
        Ok(r.into_any())
    }
}

#[implements(ITransientVector)]
impl ITransientVector for TransientVector {
    fn pop_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let r = TransientVector::pop_bang(this, py)?;
        Ok(r.into_any())
    }
}

/// Vectors support `(get v i)` / `(get v i default)` — index-keyed lookup.
#[implements(ILookup)]
impl ILookup for TransientVector {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        s.check_alive_and_owner()?;
        let Ok(idx) = k.bind(py).extract::<i64>() else { return Ok(not_found); };
        if idx < 0 || (idx as usize) >= s.t_count() {
            return Ok(not_found);
        }
        s.t_nth_internal(py, idx as usize)
    }
}

/// `(find tv i)` returns a MapEntry [i value] when i is in range, nil otherwise.
/// `(contains? tv i)` via ILookup-default.
#[implements(Associative)]
impl Associative for TransientVector {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        let s = this.bind(py).get();
        s.check_alive_and_owner()?;
        let Ok(idx) = k.bind(py).extract::<i64>() else { return Ok(false); };
        Ok(idx >= 0 && (idx as usize) < s.t_count())
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        s.check_alive_and_owner()?;
        let Ok(idx) = k.bind(py).extract::<i64>() else { return Ok(py.None()); };
        if idx < 0 || (idx as usize) >= s.t_count() {
            return Ok(py.None());
        }
        let v = s.t_nth_internal(py, idx as usize)?;
        let me = crate::collections::map_entry::MapEntry::new(k, v);
        Ok(Py::new(py, me)?.into_any())
    }
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        // Transient vector `assoc` is `assoc!` — in-place.
        let r = TransientVector::assoc_bang(this, py, k, v)?;
        Ok(r.into_any())
    }
}

// --- `transient` module-level function (clojure.core/transient alias) ---

#[pyfunction]
#[pyo3(name = "transient")]
pub fn transient_fn(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    // Dispatch through IEditableCollection (so any implementer works).
    static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>>
        = once_cell::sync::OnceCell::new();
    crate::protocol_fn::dispatch_cached_1(
        py, &PFN, "IEditableCollection", "as_transient", coll,
    )
}
