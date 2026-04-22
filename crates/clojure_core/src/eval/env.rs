//! Environment — locals (for let/fn params) + current namespace.
//!
//! Locals are stored in a PersistentHashMap keyed by Symbol name (a String,
//! not the Symbol object, because repeated Symbol construction during eval
//! would be wasteful). The current namespace is a ClojureNamespace module.

use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Environment passed through recursive eval calls.
pub struct Env {
    /// Flat Rust HashMap — rebuilt on `extend`. For small let/fn frames this
    /// is fine; if we see perf issues we can switch to a persistent map with
    /// structural sharing.
    pub locals: std::collections::HashMap<String, PyObject>,
    /// The current ClojureNamespace (a Python module).
    pub current_ns: PyObject,
}

impl Env {
    /// Clone requires a Python token since Py<PyAny> does not impl Clone.
    pub fn clone_with(&self, py: Python<'_>) -> Self {
        Self {
            locals: self
                .locals
                .iter()
                .map(|(k, v)| (k.clone(), v.clone_ref(py)))
                .collect(),
            current_ns: self.current_ns.clone_ref(py),
        }
    }
}

impl Env {
    pub fn new(current_ns: PyObject) -> Self {
        Self {
            locals: std::collections::HashMap::new(),
            current_ns,
        }
    }

    /// Return a new Env with one more local binding.
    pub fn extend(&self, py: Python<'_>, name: &str, val: PyObject) -> Self {
        let mut new_locals: std::collections::HashMap<String, PyObject> = self
            .locals
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_ref(py)))
            .collect();
        new_locals.insert(name.to_string(), val);
        Self {
            locals: new_locals,
            current_ns: self.current_ns.clone_ref(py),
        }
    }

    /// Extend with multiple bindings at once.
    pub fn extend_many(
        &self,
        py: Python<'_>,
        bindings: &[(String, PyObject)],
    ) -> Self {
        let mut new_locals: std::collections::HashMap<String, PyObject> = self
            .locals
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_ref(py)))
            .collect();
        for (k, v) in bindings {
            new_locals.insert(k.clone(), v.clone_ref(py));
        }
        Self {
            locals: new_locals,
            current_ns: self.current_ns.clone_ref(py),
        }
    }

    pub fn lookup_local(&self, name: &str, py: Python<'_>) -> Option<PyObject> {
        self.locals.get(name).map(|v| v.clone_ref(py))
    }
}
