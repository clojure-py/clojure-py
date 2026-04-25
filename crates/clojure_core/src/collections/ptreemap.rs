//! `PersistentTreeMap` — sorted associative map backed by a persistent
//! red-black tree.
//!
//! Port of `clojure.lang.PersistentTreeMap`. Insert balances per Okasaki;
//! delete uses the canonical balance-on-remove scheme with `balance_left_del`
//! / `balance_right_del` matching Clojure's Java implementation.
//!
//! Comparator: either the default (`rt::compare`, dispatching through the
//! `Comparable` protocol) or a user-supplied Python callable `(c a b) -> int`.
//! Stored as `Option<PyObject>`; `None` means use default.
//!
//! Memory: every node carries color + key + val + left + right. Empty
//! subtrees are `None`. Nodes are `Arc<Node>`-shared so assoc/without
//! produce structurally shared trees.

use crate::associative::Associative;
use crate::coll_reduce::CollReduce;
use crate::counted::Counted;
use crate::ifn::IFn;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::ikvreduce::IKVReduce;
use crate::ilookup::ILookup;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_map::IPersistentMap;
use crate::iseqable::ISeqable;
use crate::reversible::Reversible;
use crate::sorted::Sorted;
use clojure_core_macros::implements;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};
use std::sync::Arc;

type PyObject = Py<PyAny>;

// --- Node -------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Color {
    Red,
    Black,
}

pub(crate) struct Node {
    pub color: Color,
    pub key: PyObject,
    pub val: PyObject,
    pub left: Option<Arc<Node>>,
    pub right: Option<Arc<Node>>,
}

impl Node {
    fn new(
        color: Color,
        key: PyObject,
        val: PyObject,
        left: Option<Arc<Node>>,
        right: Option<Arc<Node>>,
    ) -> Arc<Self> {
        Arc::new(Self { color, key, val, left, right })
    }

    fn red(k: PyObject, v: PyObject, l: Option<Arc<Node>>, r: Option<Arc<Node>>) -> Arc<Self> {
        Self::new(Color::Red, k, v, l, r)
    }

    fn black(k: PyObject, v: PyObject, l: Option<Arc<Node>>, r: Option<Arc<Node>>) -> Arc<Self> {
        Self::new(Color::Black, k, v, l, r)
    }

    fn redden(n: &Arc<Node>, py: Python<'_>) -> Arc<Node> {
        Self::red(
            n.key.clone_ref(py),
            n.val.clone_ref(py),
            n.left.clone(),
            n.right.clone(),
        )
    }

    fn blacken(n: &Arc<Node>, py: Python<'_>) -> Arc<Node> {
        if n.color == Color::Black {
            return n.clone();
        }
        Self::black(
            n.key.clone_ref(py),
            n.val.clone_ref(py),
            n.left.clone(),
            n.right.clone(),
        )
    }

    fn is_red(opt: &Option<Arc<Node>>) -> bool {
        matches!(opt.as_ref().map(|n| n.color), Some(Color::Red))
    }
}

// --- Comparator -------------------------------------------------------------

fn cmp(
    py: Python<'_>,
    comparator: Option<&PyObject>,
    a: &PyObject,
    b: &PyObject,
) -> PyResult<std::cmp::Ordering> {
    let n: i64 = match comparator {
        None => crate::rt::compare(py, a.clone_ref(py), b.clone_ref(py))?,
        Some(c) => {
            let r = c.bind(py).call1((a.clone_ref(py), b.clone_ref(py)))?;
            // Accept either an int (real Comparator) or a boolean (predicate
            // style — `(> a b)` means a is greater-than-b, treated as "a < b"
            // relative to the reversed-order sort, matching Clojure/Java's
            // IFn-as-Comparator conversion). Check PyBool FIRST: `bool` is an
            // `int` subclass so `extract::<i64>` would happily coerce
            // `true → 1` and `false → 0`, masking the predicate case.
            if r.cast::<pyo3::types::PyBool>().is_ok() {
                if r.is_truthy()? {
                    -1
                } else {
                    let r2 = c.bind(py).call1((b.clone_ref(py), a.clone_ref(py)))?;
                    if r2.is_truthy()? { 1 } else { 0 }
                }
            } else if let Ok(i) = r.extract::<i64>() {
                i
            } else {
                return Err(crate::exceptions::IllegalArgumentException::new_err(
                    "Comparator must return int or bool",
                ));
            }
        }
    };
    Ok(if n < 0 {
        std::cmp::Ordering::Less
    } else if n > 0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    })
}

// --- RBT algorithms ---------------------------------------------------------

fn lookup(
    py: Python<'_>,
    comp: Option<&PyObject>,
    node: &Option<Arc<Node>>,
    key: &PyObject,
) -> PyResult<Option<PyObject>> {
    let mut cur = node.as_ref().cloned();
    while let Some(n) = cur {
        match cmp(py, comp, key, &n.key)? {
            std::cmp::Ordering::Less => cur = n.left.as_ref().cloned(),
            std::cmp::Ordering::Greater => cur = n.right.as_ref().cloned(),
            std::cmp::Ordering::Equal => return Ok(Some(n.val.clone_ref(py))),
        }
    }
    Ok(None)
}

/// `(assoc tree k v)` — returns `(new_root, was_new_key)`.
fn assoc(
    py: Python<'_>,
    comp: Option<&PyObject>,
    node: &Option<Arc<Node>>,
    key: PyObject,
    val: PyObject,
) -> PyResult<(Arc<Node>, bool)> {
    match node {
        None => Ok((Node::red(key, val, None, None), true)),
        Some(n) => {
            let c = cmp(py, comp, &key, &n.key)?;
            match c {
                std::cmp::Ordering::Equal => {
                    let replaced = Arc::new(Node {
                        color: n.color,
                        key,
                        val,
                        left: n.left.clone(),
                        right: n.right.clone(),
                    });
                    Ok((replaced, false))
                }
                std::cmp::Ordering::Less => {
                    let (new_l, inserted) = assoc(py, comp, &n.left, key, val)?;
                    let balanced = balance(
                        n.color,
                        n.key.clone_ref(py),
                        n.val.clone_ref(py),
                        Some(new_l),
                        n.right.clone(),
                    );
                    Ok((balanced, inserted))
                }
                std::cmp::Ordering::Greater => {
                    let (new_r, inserted) = assoc(py, comp, &n.right, key, val)?;
                    let balanced = balance(
                        n.color,
                        n.key.clone_ref(py),
                        n.val.clone_ref(py),
                        n.left.clone(),
                        Some(new_r),
                    );
                    Ok((balanced, inserted))
                }
            }
        }
    }
}

/// Okasaki's balance function — the 4-case fix for red-red violations.
fn balance(
    color: Color,
    k: PyObject,
    v: PyObject,
    l: Option<Arc<Node>>,
    r: Option<Arc<Node>>,
) -> Arc<Node> {
    if color == Color::Red {
        return Node::red(k, v, l, r);
    }

    // Case 1: B(R(R a x b) y c) z d
    if let Some(ln) = &l {
        if ln.color == Color::Red {
            if let Some(lln) = &ln.left {
                if lln.color == Color::Red {
                    // Python-side helpers need GIL to clone; but we only need
                    // to juggle Arcs here — we're moving them.
                    let new_l = Node::black_from_fields(lln);
                    let new_r = Node::black_inline(k, v, ln.right.clone(), r.clone());
                    return Node::red_inline(
                        ln.key_rc(),
                        ln.val_rc(),
                        Some(new_l),
                        Some(new_r),
                    );
                }
            }
            if let Some(lrn) = &ln.right {
                if lrn.color == Color::Red {
                    let new_l = Node::black_inline(
                        ln.key_rc(),
                        ln.val_rc(),
                        ln.left.clone(),
                        lrn.left.clone(),
                    );
                    let new_r =
                        Node::black_inline(k, v, lrn.right.clone(), r.clone());
                    return Node::red_inline(
                        lrn.key_rc(),
                        lrn.val_rc(),
                        Some(new_l),
                        Some(new_r),
                    );
                }
            }
        }
    }
    if let Some(rn) = &r {
        if rn.color == Color::Red {
            if let Some(rln) = &rn.left {
                if rln.color == Color::Red {
                    let new_l = Node::black_inline(k, v, l.clone(), rln.left.clone());
                    let new_r = Node::black_inline(
                        rn.key_rc(),
                        rn.val_rc(),
                        rln.right.clone(),
                        rn.right.clone(),
                    );
                    return Node::red_inline(
                        rln.key_rc(),
                        rln.val_rc(),
                        Some(new_l),
                        Some(new_r),
                    );
                }
            }
            if let Some(rrn) = &rn.right {
                if rrn.color == Color::Red {
                    let new_l = Node::black_inline(k, v, l.clone(), rn.left.clone());
                    let new_r = Node::black_from_fields(rrn);
                    return Node::red_inline(
                        rn.key_rc(),
                        rn.val_rc(),
                        Some(new_l),
                        Some(new_r),
                    );
                }
            }
        }
    }

    Node::black(k, v, l, r)
}

// Key/val cloning requires Python — use the unsafe_increment shortcut: we
// clone the *Arc* of inner nodes (cheap), but for keys/vals we do need to
// clone via `Py::clone_ref`. Deferred to a helper that acquires the GIL
// through pyo3.
impl Node {
    fn key_rc(&self) -> PyObject {
        // Python::attach gives us a GIL token on demand — we use it to produce
        // a new strong reference to the Py<PyAny> without affecting behavior.
        Python::attach(|py| self.key.clone_ref(py))
    }
    fn val_rc(&self) -> PyObject {
        Python::attach(|py| self.val.clone_ref(py))
    }
    fn black_from_fields(src: &Arc<Node>) -> Arc<Node> {
        Arc::new(Node {
            color: Color::Black,
            key: src.key_rc(),
            val: src.val_rc(),
            left: src.left.clone(),
            right: src.right.clone(),
        })
    }
    fn black_inline(
        k: PyObject,
        v: PyObject,
        l: Option<Arc<Node>>,
        r: Option<Arc<Node>>,
    ) -> Arc<Node> {
        Arc::new(Node { color: Color::Black, key: k, val: v, left: l, right: r })
    }
    fn red_inline(
        k: PyObject,
        v: PyObject,
        l: Option<Arc<Node>>,
        r: Option<Arc<Node>>,
    ) -> Arc<Node> {
        Arc::new(Node { color: Color::Red, key: k, val: v, left: l, right: r })
    }
}

/// Delete. Returns `(Option<new_root>, was_present)`.
fn without(
    py: Python<'_>,
    comp: Option<&PyObject>,
    node: &Option<Arc<Node>>,
    key: &PyObject,
) -> PyResult<(Option<Arc<Node>>, bool)> {
    let Some(n) = node.as_ref() else {
        return Ok((None, false));
    };
    match cmp(py, comp, key, &n.key)? {
        std::cmp::Ordering::Equal => Ok((append(py, &n.left, &n.right), true)),
        std::cmp::Ordering::Less => {
            let (new_l, found) = without(py, comp, &n.left, key)?;
            if !found {
                return Ok((Some(n.clone()), false));
            }
            let result = if Node::is_red(&n.left) || matches!(new_l.as_ref().map(|x| x.color), Some(Color::Red)) {
                Some(Node::red(
                    n.key.clone_ref(py),
                    n.val.clone_ref(py),
                    new_l,
                    n.right.clone(),
                ))
            } else {
                Some(balance_left_del(
                    n.key.clone_ref(py),
                    n.val.clone_ref(py),
                    new_l,
                    n.right.clone(),
                ))
            };
            Ok((result, true))
        }
        std::cmp::Ordering::Greater => {
            let (new_r, found) = without(py, comp, &n.right, key)?;
            if !found {
                return Ok((Some(n.clone()), false));
            }
            let result = if Node::is_red(&n.right) || matches!(new_r.as_ref().map(|x| x.color), Some(Color::Red)) {
                Some(Node::red(
                    n.key.clone_ref(py),
                    n.val.clone_ref(py),
                    n.left.clone(),
                    new_r,
                ))
            } else {
                Some(balance_right_del(
                    n.key.clone_ref(py),
                    n.val.clone_ref(py),
                    n.left.clone(),
                    new_r,
                ))
            };
            Ok((result, true))
        }
    }
}

/// Merge the left and right subtrees after deleting their parent.
fn append(
    py: Python<'_>,
    l: &Option<Arc<Node>>,
    r: &Option<Arc<Node>>,
) -> Option<Arc<Node>> {
    match (l, r) {
        (None, None) => None,
        (None, Some(rn)) => Some(rn.clone()),
        (Some(ln), None) => Some(ln.clone()),
        (Some(ln), Some(rn)) => {
            // Four cases on (ln.color, rn.color).
            if ln.color == Color::Red && rn.color == Color::Red {
                let mid = append(py, &ln.right, &rn.left);
                if matches!(mid.as_ref().map(|m| m.color), Some(Color::Red)) {
                    let midn = mid.as_ref().unwrap();
                    let new_l = Node::red_inline(
                        ln.key.clone_ref(py),
                        ln.val.clone_ref(py),
                        ln.left.clone(),
                        midn.left.clone(),
                    );
                    let new_r = Node::red_inline(
                        rn.key.clone_ref(py),
                        rn.val.clone_ref(py),
                        midn.right.clone(),
                        rn.right.clone(),
                    );
                    Some(Node::red_inline(
                        midn.key.clone_ref(py),
                        midn.val.clone_ref(py),
                        Some(new_l),
                        Some(new_r),
                    ))
                } else {
                    let new_r = Node::red_inline(
                        rn.key.clone_ref(py),
                        rn.val.clone_ref(py),
                        mid,
                        rn.right.clone(),
                    );
                    Some(Node::red_inline(
                        ln.key.clone_ref(py),
                        ln.val.clone_ref(py),
                        ln.left.clone(),
                        Some(new_r),
                    ))
                }
            } else if ln.color == Color::Black && rn.color == Color::Black {
                let mid = append(py, &ln.right, &rn.left);
                if matches!(mid.as_ref().map(|m| m.color), Some(Color::Red)) {
                    let midn = mid.as_ref().unwrap();
                    let new_l = Node::black_inline(
                        ln.key.clone_ref(py),
                        ln.val.clone_ref(py),
                        ln.left.clone(),
                        midn.left.clone(),
                    );
                    let new_r = Node::black_inline(
                        rn.key.clone_ref(py),
                        rn.val.clone_ref(py),
                        midn.right.clone(),
                        rn.right.clone(),
                    );
                    Some(Node::red_inline(
                        midn.key.clone_ref(py),
                        midn.val.clone_ref(py),
                        Some(new_l),
                        Some(new_r),
                    ))
                } else {
                    let new_r = Node::black_inline(
                        rn.key.clone_ref(py),
                        rn.val.clone_ref(py),
                        mid,
                        rn.right.clone(),
                    );
                    Some(balance_left_del(
                        ln.key.clone_ref(py),
                        ln.val.clone_ref(py),
                        ln.left.clone(),
                        Some(new_r),
                    ))
                }
            } else if rn.color == Color::Red {
                let new_l = append(py, &Some(ln.clone()), &rn.left);
                Some(Node::red_inline(
                    rn.key.clone_ref(py),
                    rn.val.clone_ref(py),
                    new_l,
                    rn.right.clone(),
                ))
            } else {
                // ln.color == Red
                let new_r = append(py, &ln.right, &Some(rn.clone()));
                Some(Node::red_inline(
                    ln.key.clone_ref(py),
                    ln.val.clone_ref(py),
                    ln.left.clone(),
                    new_r,
                ))
            }
        }
    }
}

/// Rebalance after deleting from the left subtree.
fn balance_left_del(
    k: PyObject,
    v: PyObject,
    l: Option<Arc<Node>>,
    r: Option<Arc<Node>>,
) -> Arc<Node> {
    if let Some(ln) = &l {
        if ln.color == Color::Red {
            return Node::red_inline(
                k,
                v,
                Some(Node::black_from_fields(ln)),
                r,
            );
        }
    }
    if let Some(rn) = &r {
        if rn.color == Color::Black {
            let redd = Arc::new(Node {
                color: Color::Red,
                key: rn.key_rc(),
                val: rn.val_rc(),
                left: rn.left.clone(),
                right: rn.right.clone(),
            });
            return balance(
                Color::Black,
                k,
                v,
                l,
                Some(redd),
            );
        }
        if rn.color == Color::Red {
            if let Some(rln) = &rn.left {
                if rln.color == Color::Black {
                    // [r's-left-left, k,v, r's-left] becomes balanced
                    let new_right = {
                        let rln_red = Arc::new(Node {
                            color: Color::Red,
                            key: rn.right.as_ref().map(|x| x.key_rc()).unwrap_or_else(|| Python::attach(|py| py.None())),
                            val: rn.right.as_ref().map(|x| x.val_rc()).unwrap_or_else(|| Python::attach(|py| py.None())),
                            left: rn.right.as_ref().and_then(|x| x.left.clone()),
                            right: rn.right.as_ref().and_then(|x| x.right.clone()),
                        });
                        balance(Color::Black, rn.key_rc(), rn.val_rc(), rln.right.clone(), Some(rln_red))
                    };
                    return Node::red_inline(
                        rln.key_rc(),
                        rln.val_rc(),
                        Some(Node::black_inline(k, v, l, rln.left.clone())),
                        Some(new_right),
                    );
                }
            }
        }
    }
    // Fallback — shouldn't normally reach here if tree invariants hold.
    Node::black(k, v, l, r)
}

/// Rebalance after deleting from the right subtree. Mirror of
/// `balance_left_del`.
fn balance_right_del(
    k: PyObject,
    v: PyObject,
    l: Option<Arc<Node>>,
    r: Option<Arc<Node>>,
) -> Arc<Node> {
    if let Some(rn) = &r {
        if rn.color == Color::Red {
            return Node::red_inline(
                k,
                v,
                l,
                Some(Node::black_from_fields(rn)),
            );
        }
    }
    if let Some(ln) = &l {
        if ln.color == Color::Black {
            let redd = Arc::new(Node {
                color: Color::Red,
                key: ln.key_rc(),
                val: ln.val_rc(),
                left: ln.left.clone(),
                right: ln.right.clone(),
            });
            return balance(
                Color::Black,
                k,
                v,
                Some(redd),
                r,
            );
        }
        if ln.color == Color::Red {
            if let Some(lrn) = &ln.right {
                if lrn.color == Color::Black {
                    let new_left = {
                        let lrn_red = Arc::new(Node {
                            color: Color::Red,
                            key: ln.left.as_ref().map(|x| x.key_rc()).unwrap_or_else(|| Python::attach(|py| py.None())),
                            val: ln.left.as_ref().map(|x| x.val_rc()).unwrap_or_else(|| Python::attach(|py| py.None())),
                            left: ln.left.as_ref().and_then(|x| x.left.clone()),
                            right: ln.left.as_ref().and_then(|x| x.right.clone()),
                        });
                        balance(Color::Black, ln.key_rc(), ln.val_rc(), Some(lrn_red), lrn.left.clone())
                    };
                    return Node::red_inline(
                        lrn.key_rc(),
                        lrn.val_rc(),
                        Some(new_left),
                        Some(Node::black_inline(k, v, lrn.right.clone(), r)),
                    );
                }
            }
        }
    }
    Node::black(k, v, l, r)
}

// --- Iteration --------------------------------------------------------------

/// In-order traversal of an RBT, collecting `(k, v)` pairs.
pub(crate) fn collect_entries(
    py: Python<'_>,
    node: &Option<Arc<Node>>,
    ascending: bool,
    out: &mut Vec<(PyObject, PyObject)>,
) {
    let Some(n) = node.as_ref() else {
        return;
    };
    if ascending {
        collect_entries(py, &n.left, ascending, out);
        out.push((n.key.clone_ref(py), n.val.clone_ref(py)));
        collect_entries(py, &n.right, ascending, out);
    } else {
        collect_entries(py, &n.right, ascending, out);
        out.push((n.key.clone_ref(py), n.val.clone_ref(py)));
        collect_entries(py, &n.left, ascending, out);
    }
}

/// Entries from `key` onwards (or backwards, if descending).
pub(crate) fn collect_entries_from(
    py: Python<'_>,
    comp: Option<&PyObject>,
    node: &Option<Arc<Node>>,
    key: &PyObject,
    ascending: bool,
) -> PyResult<Vec<(PyObject, PyObject)>> {
    let mut all = Vec::new();
    collect_entries(py, node, ascending, &mut all);
    // Filter.
    let mut out = Vec::with_capacity(all.len());
    for (k, v) in all {
        let c = cmp(py, comp, &k, key)?;
        let include = match (ascending, c) {
            (true, std::cmp::Ordering::Less) => false,
            (true, _) => true,
            (false, std::cmp::Ordering::Greater) => false,
            (false, _) => true,
        };
        if include {
            out.push((k, v));
        }
    }
    Ok(out)
}

// --- PersistentTreeMap ------------------------------------------------------

#[pyclass(module = "clojure._core", name = "PersistentTreeMap", frozen)]
pub struct PersistentTreeMap {
    pub count: u32,
    pub root: Option<Arc<Node>>,
    /// `None` → default ordering (via `rt::compare`).
    pub comparator: Option<PyObject>,
    pub meta: Option<PyObject>,
}

impl PersistentTreeMap {
    pub fn new_empty() -> Self {
        Self { count: 0, root: None, comparator: None, meta: None }
    }

    pub fn new_with_comparator(comparator: Option<PyObject>) -> Self {
        Self { count: 0, root: None, comparator, meta: None }
    }

    fn comp_ref(&self) -> Option<&PyObject> {
        self.comparator.as_ref()
    }

    pub fn assoc_internal(
        &self,
        py: Python<'_>,
        k: PyObject,
        v: PyObject,
    ) -> PyResult<Self> {
        let (new_root, was_new) = assoc(py, self.comp_ref(), &self.root, k, v)?;
        let new_root_black = Node::blacken(&new_root, py);
        Ok(Self {
            count: self.count + if was_new { 1 } else { 0 },
            root: Some(new_root_black),
            comparator: self.comparator.as_ref().map(|c| c.clone_ref(py)),
            meta: self.meta.as_ref().map(|m| m.clone_ref(py)),
        })
    }

    pub fn without_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<Self> {
        let (new_root, was_present) = without(py, self.comp_ref(), &self.root, &k)?;
        if !was_present {
            return Ok(Self {
                count: self.count,
                root: self.root.as_ref().map(Arc::clone),
                comparator: self.comparator.as_ref().map(|c| c.clone_ref(py)),
                meta: self.meta.as_ref().map(|m| m.clone_ref(py)),
            });
        }
        let new_root_black = new_root.as_ref().map(|n| Node::blacken(n, py));
        Ok(Self {
            count: self.count - 1,
            root: new_root_black,
            comparator: self.comparator.as_ref().map(|c| c.clone_ref(py)),
            meta: self.meta.as_ref().map(|m| m.clone_ref(py)),
        })
    }

    pub fn val_at_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        match lookup(py, self.comp_ref(), &self.root, &k)? {
            Some(v) => Ok(v),
            None => Ok(py.None()),
        }
    }

    pub fn val_at_default_internal(
        &self,
        py: Python<'_>,
        k: PyObject,
        default: PyObject,
    ) -> PyResult<PyObject> {
        Ok(lookup(py, self.comp_ref(), &self.root, &k)?.unwrap_or(default))
    }

    pub fn contains_key_internal(&self, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        Ok(lookup(py, self.comp_ref(), &self.root, &k)?.is_some())
    }
}

#[pymethods]
impl PersistentTreeMap {
    fn __len__(&self) -> usize {
        self.count as usize
    }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<crate::seqs::cons::ConsIter>> {
        let s = <PersistentTreeMap as ISeqable>::seq(slf, py)?;
        Py::new(py, crate::seqs::cons::ConsIter { current: s })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other)
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let s = slf.bind(py).get();
        let mut entries: Vec<(PyObject, PyObject)> = Vec::new();
        collect_entries(py, &s.root, true, &mut entries);
        let mut out = String::from("{");
        let mut first = true;
        for (k, v) in entries {
            if !first {
                out.push_str(", ");
            }
            first = false;
            out.push_str(&k.bind(py).repr()?.extract::<String>()?);
            out.push(' ');
            out.push_str(&v.bind(py).repr()?.extract::<String>()?);
        }
        out.push('}');
        Ok(out)
    }
}

#[implements(Counted)]
impl Counted for PersistentTreeMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
}

#[implements(IEquiv)]
impl IEquiv for PersistentTreeMap {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        crate::ipersistent_map::cross_map_equiv(py, this.into_any(), other)
    }
}

#[implements(IHashEq)]
impl IHashEq for PersistentTreeMap {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        // Vanilla `APersistentMap.hasheq` = `Murmur3.hashUnordered`. Iteration
        // is sorted, but the unordered hash is order-independent anyway.
        Ok(crate::murmur3::hash_unordered_seq(py, this.into_any())? as i64)
    }
}

#[implements(IMeta)]
impl IMeta for PersistentTreeMap {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.meta.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let m = if meta.is_none(py) { None } else { Some(meta) };
        let new = Self {
            count: s.count,
            root: s.root.as_ref().map(Arc::clone),
            comparator: s.comparator.as_ref().map(|c| c.clone_ref(py)),
            meta: m,
        };
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for PersistentTreeMap {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize> {
        Ok(this.bind(py).get().count as usize)
    }
    fn conj(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        if x.is_none(py) {
            return Ok(this.into_any());
        }
        let x_b = x.bind(py);
        if let Ok(me) = x_b.cast::<crate::collections::map_entry::MapEntry>() {
            let s = this.bind(py).get();
            let k = me.get().key.clone_ref(py);
            let v = me.get().val.clone_ref(py);
            let new = s.assoc_internal(py, k, v)?;
            return Ok(Py::new(py, new)?.into_any());
        }
        // 2-tuple-like or another map.
        if let Ok(_) = x_b.cast::<PersistentTreeMap>() {
            let mut acc: PyObject = this.clone_ref(py).into_any();
            let mut cur = crate::rt::seq(py, x.clone_ref(py))?;
            while !cur.is_none(py) {
                let entry = crate::rt::first(py, cur.clone_ref(py))?;
                acc = crate::rt::conj(py, acc, entry)?;
                cur = crate::rt::next_(py, cur)?;
            }
            return Ok(acc);
        }
        let k = x_b.get_item(0)?.unbind();
        let v = x_b.get_item(1)?.unbind();
        let s = this.bind(py).get();
        let new = s.assoc_internal(py, k, v)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn empty(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = Self {
            count: 0,
            root: None,
            comparator: s.comparator.as_ref().map(|c| c.clone_ref(py)),
            meta: None,
        };
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(IPersistentMap)]
impl IPersistentMap for PersistentTreeMap {
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.assoc_internal(py, k, v)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn without(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let new = s.without_internal(py, k)?;
        Ok(Py::new(py, new)?.into_any())
    }
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_key_internal(py, k)
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if !s.contains_key_internal(py, k.clone_ref(py))? {
            return Ok(py.None());
        }
        let v = s.val_at_internal(py, k.clone_ref(py))?;
        let me = crate::collections::map_entry::MapEntry::new(k, v);
        Ok(Py::new(py, me)?.into_any())
    }
}

#[implements(Associative)]
impl Associative for PersistentTreeMap {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool> {
        this.bind(py).get().contains_key_internal(py, k)
    }
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject> {
        <PersistentTreeMap as IPersistentMap>::entry_at(this, py, k)
    }
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject> {
        <PersistentTreeMap as IPersistentMap>::assoc(this, py, k, v)
    }
}

#[implements(ISeqable)]
impl ISeqable for PersistentTreeMap {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.count == 0 {
            return Ok(py.None());
        }
        let mut entries = Vec::new();
        collect_entries(py, &s.root, true, &mut entries);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, v) in entries.into_iter().rev() {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            let cons = crate::seqs::cons::Cons::new(me_py, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
}

#[implements(ILookup)]
impl ILookup for PersistentTreeMap {
    fn val_at(this: Py<Self>, py: Python<'_>, k: PyObject, not_found: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, k, not_found)
    }
}

#[implements(CollReduce)]
impl CollReduce for PersistentTreeMap {
    fn coll_reduce1(this: Py<Self>, py: Python<'_>, f: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let mut entries = Vec::new();
        collect_entries(py, &s.root, true, &mut entries);
        if entries.is_empty() {
            return crate::rt::invoke_n(py, f, &[]);
        }
        let mut it = entries.into_iter();
        let (k0, v0) = it.next().unwrap();
        let me0 = crate::collections::map_entry::MapEntry::new(k0, v0);
        let mut acc: PyObject = Py::new(py, me0)?.into_any();
        for (k, v) in it {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, me_py])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
    fn coll_reduce2(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let mut entries = Vec::new();
        collect_entries(py, &s.root, true, &mut entries);
        let mut acc = init;
        for (k, v) in entries {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, me_py])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
}

#[implements(IKVReduce)]
impl IKVReduce for PersistentTreeMap {
    fn kv_reduce(this: Py<Self>, py: Python<'_>, f: PyObject, init: PyObject) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        let mut entries = Vec::new();
        collect_entries(py, &s.root, true, &mut entries);
        let mut acc = init;
        for (k, v) in entries {
            acc = crate::rt::invoke_n(py, f.clone_ref(py), &[acc, k, v])?;
            if crate::reduced::is_reduced(py, &acc) {
                return Ok(crate::reduced::unreduced(py, acc));
            }
        }
        Ok(acc)
    }
}

#[implements(IFn)]
impl IFn for PersistentTreeMap {
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_internal(py, a0)
    }
    fn invoke2(this: Py<Self>, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        this.bind(py).get().val_at_default_internal(py, a0, a1)
    }
}

#[implements(Reversible)]
impl Reversible for PersistentTreeMap {
    fn rseq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        if s.count == 0 {
            return Ok(py.None());
        }
        let mut entries = Vec::new();
        collect_entries(py, &s.root, false, &mut entries);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, v) in entries.into_iter().rev() {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py = Py::new(py, me)?.into_any();
            let cons = crate::seqs::cons::Cons::new(me_py, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
}

#[implements(Sorted)]
impl Sorted for PersistentTreeMap {
    fn sorted_seq(this: Py<Self>, py: Python<'_>, ascending: PyObject) -> PyResult<PyObject> {
        let asc = ascending.bind(py).is_truthy()?;
        let s = this.bind(py).get();
        if s.count == 0 {
            return Ok(py.None());
        }
        let mut entries = Vec::new();
        collect_entries(py, &s.root, asc, &mut entries);
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, v) in entries.into_iter().rev() {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            let cons = crate::seqs::cons::Cons::new(me_py, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
    fn sorted_seq_from(
        this: Py<Self>,
        py: Python<'_>,
        key: PyObject,
        ascending: PyObject,
    ) -> PyResult<PyObject> {
        let asc = ascending.bind(py).is_truthy()?;
        let s = this.bind(py).get();
        if s.count == 0 {
            return Ok(py.None());
        }
        let entries = collect_entries_from(py, s.comp_ref(), &s.root, &key, asc)?;
        if entries.is_empty() {
            return Ok(py.None());
        }
        let mut tail: PyObject = crate::collections::plist::empty_list(py).into_any();
        for (k, v) in entries.into_iter().rev() {
            let me = crate::collections::map_entry::MapEntry::new(k, v);
            let me_py: PyObject = Py::new(py, me)?.into_any();
            let cons = crate::seqs::cons::Cons::new(me_py, tail);
            tail = Py::new(py, cons)?.into_any();
        }
        Ok(tail)
    }
    fn entry_key(_this: Py<Self>, py: Python<'_>, entry: PyObject) -> PyResult<PyObject> {
        // For a map, the entry is a MapEntry; key is its .key.
        let b = entry.bind(py);
        if let Ok(me) = b.cast::<crate::collections::map_entry::MapEntry>() {
            return Ok(me.get().key.clone_ref(py));
        }
        // Fallback: nth 0.
        crate::rt::first(py, entry)
    }
    fn comparator_of(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let s = this.bind(py).get();
        Ok(s.comparator
            .as_ref()
            .map(|c| c.clone_ref(py))
            .unwrap_or_else(|| py.None()))
    }
}

// --- Constructor -----------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (*args))]
pub fn sorted_map(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<Py<PersistentTreeMap>> {
    if args.len() % 2 != 0 {
        return Err(crate::exceptions::IllegalArgumentException::new_err(
            "sorted-map requires an even number of arguments",
        ));
    }
    let mut m = PersistentTreeMap::new_empty();
    let mut i = 0usize;
    while i < args.len() {
        let k = args.get_item(i)?.unbind();
        let v = args.get_item(i + 1)?.unbind();
        m = m.assoc_internal(py, k, v)?;
        i += 2;
    }
    Py::new(py, m)
}

#[pyfunction]
#[pyo3(signature = (comparator, *args))]
pub fn sorted_map_by(
    py: Python<'_>,
    comparator: PyObject,
    args: Bound<'_, PyTuple>,
) -> PyResult<Py<PersistentTreeMap>> {
    if args.len() % 2 != 0 {
        return Err(crate::exceptions::IllegalArgumentException::new_err(
            "sorted-map-by requires an even number of entries after the comparator",
        ));
    }
    let mut m = PersistentTreeMap::new_with_comparator(Some(comparator));
    let mut i = 0usize;
    while i < args.len() {
        let k = args.get_item(i)?.unbind();
        let v = args.get_item(i + 1)?.unbind();
        m = m.assoc_internal(py, k, v)?;
        i += 2;
    }
    Py::new(py, m)
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PersistentTreeMap>()?;
    m.add_function(wrap_pyfunction!(sorted_map, m)?)?;
    m.add_function(wrap_pyfunction!(sorted_map_by, m)?)?;
    Ok(())
}
