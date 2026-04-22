//! PersistentVector — 32-way HAMT bit-partitioned trie + 32-element tail.
//!
//! Port of clojure/lang/PersistentVector.java. Transient + protocol impls
//! land in Phase 6B/6C.

use crate::collections::pvector_node::{VNode, VSlot};
use crate::exceptions::IllegalStateException;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

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
    pub meta: RwLock<Option<PyObject>>,
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
            meta: RwLock::new(None),
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
                meta: RwLock::new(self.meta.read().as_ref().map(|o| o.clone_ref(py))),
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
            meta: RwLock::new(self.meta.read().as_ref().map(|o| o.clone_ref(py))),
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
                meta: RwLock::new(self.meta.read().as_ref().map(|o| o.clone_ref(py))),
            });
        }
        // In trie: path-copy from root.
        let new_root = do_assoc(self.shift, &self.root, i, x);
        Ok(Self {
            cnt: self.cnt,
            shift: self.shift,
            root: new_root,
            tail: Arc::clone(&self.tail),
            meta: RwLock::new(self.meta.read().as_ref().map(|o| o.clone_ref(py))),
        })
    }

    /// Remove last element.
    pub fn pop_internal(&self, py: Python<'_>) -> PyResult<Self> {
        if self.cnt == 0 {
            return Err(IllegalStateException::new_err("Can't pop empty vector"));
        }
        if self.cnt == 1 {
            return Ok(Self::new_empty());
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
                meta: RwLock::new(self.meta.read().as_ref().map(|o| o.clone_ref(py))),
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
            meta: RwLock::new(self.meta.read().as_ref().map(|o| o.clone_ref(py))),
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

    #[getter]
    fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta
            .read()
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }

    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<Py<PersistentVector>> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        Py::new(
            py,
            Self {
                cnt: self.cnt,
                shift: self.shift,
                root: Arc::clone(&self.root),
                tail: Arc::clone(&self.tail),
                meta: RwLock::new(m),
            },
        )
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
    m.add_function(wrap_pyfunction!(vector, m)?)?;
    Ok(())
}
