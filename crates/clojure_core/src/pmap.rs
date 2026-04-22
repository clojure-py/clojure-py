//! Minimal persistent-map utility, used only by the binding stack (Task 29+).
//! Keyed by Py<PyAny> pointer identity. O(n) operations; fine for <16 entries.
//! Arc-shared so snapshots (e.g. bound-fn captures) are cheap.

use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::sync::Arc;

type PyObject = Py<PyAny>;

pub struct Entry {
    pub key_ptr: usize,
    pub key: PyObject,
    pub val: PyObject,
}

#[derive(Default)]
pub struct PMap(pub Arc<Vec<Entry>>);

impl Clone for PMap {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl PMap {
    pub fn new() -> Self {
        Self(Arc::new(Vec::new()))
    }

    /// Return a new PMap with `(key → val)` installed. If `key` already present
    /// (by pointer identity), its entry's value is replaced; otherwise appended.
    pub fn assoc(&self, py: Python<'_>, key: &PyObject, val: PyObject) -> Self {
        let kptr = key.as_ptr() as usize;
        let mut v = Vec::new();
        let mut found = false;
        for e in self.0.iter() {
            if e.key_ptr == kptr {
                v.push(Entry {
                    key_ptr: e.key_ptr,
                    key: e.key.clone_ref(py),
                    val: val.clone_ref(py),
                });
                found = true;
            } else {
                v.push(Entry {
                    key_ptr: e.key_ptr,
                    key: e.key.clone_ref(py),
                    val: e.val.clone_ref(py),
                });
            }
        }
        if !found {
            v.push(Entry {
                key_ptr: kptr,
                key: key.clone_ref(py),
                val,
            });
        }
        Self(Arc::new(v))
    }

    /// Look up by pointer identity. Returns the value without cloning.
    pub fn get(&self, key: &PyObject) -> Option<&PyObject> {
        let kptr = key.as_ptr() as usize;
        self.0.iter().find(|e| e.key_ptr == kptr).map(|e| &e.val)
    }

    /// Return a new PMap containing every entry of `self` plus every entry of `other`,
    /// with `other`'s values winning on key collisions.
    pub fn merge(&self, py: Python<'_>, other: &Self) -> Self {
        let mut out = self.clone();
        for e in other.0.iter() {
            out = out.assoc(py, &e.key, e.val.clone_ref(py));
        }
        out
    }

    /// In-place mutate an entry's value. Used by `set!` on a dynamic var — we need
    /// to update the top frame's entry without allocating a new PMap.
    ///
    /// Returns `true` if the key was found and updated, `false` if absent (in which
    /// case the caller should raise — `set!` outside a `binding` block is an error).
    pub fn update_in_place(&mut self, py: Python<'_>, key: &PyObject, val: PyObject) -> bool {
        let kptr = key.as_ptr() as usize;
        // We need to make the inner Vec mutable. If there are other holders of this Arc,
        // this will trigger a clone. We rebuild the Vec while updating the target entry.
        let entries = &*self.0;
        for e in entries.iter() {
            if e.key_ptr == kptr {
                // Found the entry; rebuild with the new value
                let mut v = Vec::new();
                for entry in entries.iter() {
                    if entry.key_ptr == kptr {
                        v.push(Entry {
                            key_ptr: entry.key_ptr,
                            key: entry.key.clone_ref(py),
                            val: val.clone_ref(py),
                        });
                    } else {
                        v.push(Entry {
                            key_ptr: entry.key_ptr,
                            key: entry.key.clone_ref(py),
                            val: entry.val.clone_ref(py),
                        });
                    }
                }
                self.0 = Arc::new(v);
                return true;
            }
        }
        false
    }
}
