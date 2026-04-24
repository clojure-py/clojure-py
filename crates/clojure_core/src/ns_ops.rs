//! Namespace operations: intern, refer, alias, import + ns-* introspection.
//!
//! Vars live as regular Python module attributes on a ClojureNamespace.
//! Meta-information (aliases, refers, imports) lives in dunder attributes
//! populated at namespace creation time (see namespace.rs).

use crate::symbol::Symbol;
use crate::var::Var;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule};

type PyObject = Py<PyAny>;

/// `(intern ns sym)` — create a Var named `sym` in `ns`, or return the existing
/// one if a Var of that name is already interned. Symbol name is stored un-munged
/// as the module attribute (so `foo?` is accessed via `getattr(ns, "foo?")`).
///
/// Vanilla-matching subtlety: if the existing attribute is a Var whose
/// HOME is a different ns (i.e. it was brought in by `refer`), we do NOT
/// reuse it — creating a fresh Var in `ns` is what `def` should do.
/// Reusing a refer'd Var would silently mutate the other ns's binding.
#[pyfunction]
pub fn intern(py: Python<'_>, ns: PyObject, sym: Py<Symbol>) -> PyResult<Py<Var>> {
    let name = {
        let s = sym.bind(py).get();
        s.name.to_string()
    };
    let ns_b = ns.bind(py);
    // Reuse existing Var if this ns owns it (same Python identity on the
    // Var's `.ns` as the ns we're interning into). Otherwise shadow.
    if let Ok(existing) = ns_b.getattr(name.as_str()) {
        if let Ok(v) = existing.cast::<Var>() {
            let var_ns = v.get().ns.clone_ref(py);
            let same = crate::rt::identical(py, var_ns, ns.clone_ref(py));
            if same {
                return Ok(v.clone().unbind());
            }
        }
    }
    // Construct a new Var. Var::new takes (ns_obj, sym_obj).
    let sym_as_any: PyObject = sym.clone_ref(py).into_any();
    let var_obj = Py::new(py, Var::new(py, ns.clone_ref(py), sym_as_any)?)?;
    ns_b.setattr(name.as_str(), var_obj.clone_ref(py))?;
    Ok(var_obj)
}

/// `(refer ns target-sym var)` — make `var` accessible in `ns` under the name
/// `target-sym`, and record the provenance in `__clj_refers__`.
#[pyfunction]
pub fn refer(py: Python<'_>, ns: PyObject, target_sym: Py<Symbol>, var: Py<Var>) -> PyResult<()> {
    let name = {
        let s = target_sym.bind(py).get();
        s.name.to_string()
    };
    let ns_b = ns.bind(py);
    ns_b.setattr(name.as_str(), var.clone_ref(py))?;
    let refers = ns_b.getattr("__clj_refers__")?;
    let refers_dict = refers.cast::<PyDict>()?;
    refers_dict.set_item(target_sym, var)?;
    Ok(())
}

/// `(alias ns alias-sym target-ns)` — record the alias in `__clj_aliases__`.
#[pyfunction]
pub fn alias(py: Python<'_>, ns: PyObject, alias_sym: Py<Symbol>, target_ns: PyObject) -> PyResult<()> {
    let ns_b = ns.bind(py);
    let aliases = ns_b.getattr("__clj_aliases__")?;
    let aliases_dict = aliases.cast::<PyDict>()?;
    aliases_dict.set_item(alias_sym, target_ns)?;
    Ok(())
}

/// `(import ns alias-sym cls)` — record the import in `__clj_imports__`.
#[pyfunction]
pub fn import_cls(py: Python<'_>, ns: PyObject, alias_sym: Py<Symbol>, cls: PyObject) -> PyResult<()> {
    let ns_b = ns.bind(py);
    let imports = ns_b.getattr("__clj_imports__")?;
    let imports_dict = imports.cast::<PyDict>()?;
    imports_dict.set_item(alias_sym, cls)?;
    Ok(())
}

/// `(ns-map ns)` — a dict of `{sym: var}` for every Var-valued attribute of `ns`.
#[pyfunction]
pub fn ns_map(py: Python<'_>, ns: PyObject) -> PyResult<Py<PyDict>> {
    let ns_b = ns.bind(py);
    let d = ns_b.getattr("__dict__")?;
    let d = d.cast::<PyDict>()?;
    let out = PyDict::new(py);
    for (k, v) in d.iter() {
        if v.is_instance_of::<Var>() {
            let key: String = k.extract()?;
            // Skip namespace dunders (__clj_*, __name__, etc.) — they're never Vars anyway,
            // but it's cheap to be defensive.
            if key.starts_with("__") && key.ends_with("__") {
                continue;
            }
            let sym = Py::new(py, Symbol::new(None, std::sync::Arc::from(key.as_str())))?;
            out.set_item(sym, v)?;
        }
    }
    Ok(out.unbind())
}

#[pyfunction]
pub fn ns_aliases(py: Python<'_>, ns: PyObject) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_aliases__")?.unbind())
}

#[pyfunction]
pub fn ns_refers(py: Python<'_>, ns: PyObject) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_refers__")?.unbind())
}

#[pyfunction]
pub fn ns_imports(py: Python<'_>, ns: PyObject) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_imports__")?.unbind())
}

#[pyfunction]
pub fn ns_meta(py: Python<'_>, ns: PyObject) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_ns_meta__")?.unbind())
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(intern, m)?)?;
    m.add_function(wrap_pyfunction!(refer, m)?)?;
    m.add_function(wrap_pyfunction!(alias, m)?)?;
    m.add_function(wrap_pyfunction!(import_cls, m)?)?;
    m.add_function(wrap_pyfunction!(ns_map, m)?)?;
    m.add_function(wrap_pyfunction!(ns_aliases, m)?)?;
    m.add_function(wrap_pyfunction!(ns_refers, m)?)?;
    m.add_function(wrap_pyfunction!(ns_imports, m)?)?;
    m.add_function(wrap_pyfunction!(ns_meta, m)?)?;
    Ok(())
}
