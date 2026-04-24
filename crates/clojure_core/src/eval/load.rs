//! Clojure source loader — read-eval loop over a string of Clojure code.
//!
//! Used at module init to bring up the Clojure-defined half of `clojure.core`
//! by reading the canonical port of `core.clj` (embedded via `include_str!`).

use crate::eval;
use crate::eval::errors;
use crate::reader::{dispatch, source::Source};
use crate::symbol::Symbol;
use crate::var::Var;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use std::cell::RefCell;

type PyObject = Py<PyAny>;

/// Look up the `*ns*` Var in `clojure.core`. Returns the Var so callers can
/// push/update/pop its thread-binding as the load/eval loop advances.
pub(crate) fn ns_var(py: Python<'_>) -> PyResult<Py<Var>> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    let core_ns = modules
        .get_item("clojure.core")
        .map_err(|_| errors::err("ns_var: clojure.core namespace not found"))?;
    let v = core_ns.getattr("*ns*")?;
    Ok(v.cast::<Var>()?.clone().unbind())
}

/// Embedded Clojure source. Evaluated in order at module init.
const CORE_CLJ: &str = include_str!("../../clj/clojure/core.clj");

/// Per-thread "next namespace for load to switch to" hook. Set by
/// `(in-ns SYM)` or by the `(ns SYM ...)` macro; `load_clj_string`
/// observes and applies it between top-level forms.
thread_local! {
    pub(crate) static LOAD_NS_OVERRIDE: RefCell<Option<PyObject>> = const { RefCell::new(None) };
}

/// Per-thread "the namespace currently being loaded into". Updated by
/// `load_clj_string` as it switches targets in response to `(in-ns ...)`.
/// Read by Clojure `(current-load-ns)` so `require` and friends can
/// alias / refer into the right namespace.
thread_local! {
    pub(crate) static CURRENT_LOAD_NS: RefCell<Option<PyObject>> = const { RefCell::new(None) };
}

/// Read every top-level form from `source` and `eval` each into the current
/// target namespace. The target starts as `ns`; if a form calls
/// `(in-ns SYM)` (which sets `LOAD_NS_OVERRIDE`), subsequent forms eval
/// into the new namespace. Errors carry form-index context so a failure
/// deep in core.clj is easy to locate.
///
/// `CURRENT_LOAD_NS` is updated alongside `current_ns` so user-level
/// `require` / `use` / `(ns …)` can read it and target the right namespace.
/// On exit (success or error) the previous value is restored.
///
/// Returns the terminal `current_ns` — i.e. wherever the load ended up after
/// any `(ns …)` / `in-ns` switches. Callers that need to reconcile the
/// caller-provided `ns` with the authoritative ns declared by the file
/// (e.g. the Python import-machinery loader) use the return value.
pub fn load_clj_string(py: Python<'_>, source: &str, ns: &PyObject) -> PyResult<PyObject> {
    let mut src = Source::new(source);
    let mut current_ns = ns.clone_ref(py);
    let mut form_index: usize = 0;
    // Save both load-tracking slots so a nested load (triggered by, e.g.,
    // `require` inside this load) doesn't clobber the outer load's state.
    let prev_override = LOAD_NS_OVERRIDE.with(|c| c.borrow_mut().take());
    let prev_load_ns = CURRENT_LOAD_NS.with(|c| c.borrow_mut().replace(current_ns.clone_ref(py)));
    // Push a thread-binding for `*ns*` so user code + the reader's ::kw
    // auto-resolution see the currently-loading namespace.
    let ns_var_py: PyObject = ns_var(py)?.into_any();
    let pushed_ns_binding = {
        let d = pyo3::types::PyDict::new(py);
        d.set_item(ns_var_py.clone_ref(py), current_ns.clone_ref(py))?;
        crate::binding::push_thread_bindings(py, d.unbind().into_any())?;
        true
    };
    let result: PyResult<()> = (|| {
        loop {
            dispatch::skip_ws_and_comments(&mut src);
            if src.at_eof() {
                return Ok(());
            }
            let form = match dispatch::read_one(&mut src, py) {
                Ok(f) => f,
                Err(e) => {
                    return Err(errors::err(format!(
                        "load: reader error on form #{} at {}:{}: {}",
                        form_index,
                        src.line(),
                        src.column(),
                        e,
                    )));
                }
            };
            form_index += 1;
            if let Err(e) = eval::eval(py, form, current_ns.clone_ref(py)) {
                return Err(errors::err(format!(
                    "load: eval error on form #{}: {}",
                    form_index, e,
                )));
            }
            if let Some(new_ns) = LOAD_NS_OVERRIDE.with(|c| c.borrow_mut().take()) {
                current_ns = new_ns;
                CURRENT_LOAD_NS.with(|c| {
                    *c.borrow_mut() = Some(current_ns.clone_ref(py));
                });
                crate::binding::set_binding(py, &ns_var_py, current_ns.clone_ref(py))?;
            }
        }
    })();
    LOAD_NS_OVERRIDE.with(|c| { *c.borrow_mut() = prev_override; });
    CURRENT_LOAD_NS.with(|c| { *c.borrow_mut() = prev_load_ns; });
    if pushed_ns_binding {
        let _ = crate::binding::pop_thread_bindings();
    }
    result.map(|()| current_ns)
}

/// Load the embedded `core.clj` into the `clojure.core` namespace. The
/// namespace is expected to already exist (populated by `core_shims::init`).
pub fn load_core_clj(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    let core_ns = modules
        .get_item("clojure.core")
        .map_err(|_| errors::err("load_core_clj: clojure.core namespace not found"))?
        .unbind();
    load_clj_string(py, CORE_CLJ, &core_ns).map(|_| ())
}

/// Walk `sys.path` looking for the .clj file that corresponds to `sym`.
/// Dotted-name → slash-separated path, with Clojure's `-` → `_` munging.
/// Returns the absolute path as a String, or None if not found.
pub fn find_source_file_path(py: Python<'_>, sym: &Symbol) -> PyResult<Option<String>> {
    let name = sym.name.replace('.', "/").replace('-', "_");
    let rel = format!("{}.clj", name);
    let sys = py.import("sys")?;
    let path_list = sys.getattr("path")?;
    let path_iter = path_list.try_iter()?;
    for entry in path_iter {
        let entry_str: String = entry?.extract()?;
        let candidate = std::path::Path::new(&entry_str).join(&rel);
        if candidate.is_file() {
            return Ok(Some(candidate.to_string_lossy().into_owned()));
        }
    }
    Ok(None)
}

/// Python-callable: resolve a Symbol to an absolute `.clj` path on `sys.path`,
/// or None. Used by the Python import-machinery finder.
#[pyfunction]
#[pyo3(name = "find_source_file")]
pub fn py_find_source_file(py: Python<'_>, sym: Py<Symbol>) -> PyResult<Option<String>> {
    find_source_file_path(py, sym.bind(py).get())
}

/// Python-callable: read `path` and evaluate every top-level form into `ns`.
/// Returns the terminal namespace — the one the load ended up in after any
/// `(ns …)` / `in-ns` switches. The Python loader uses the return value to
/// rewire `sys.modules[fullname]` when the file's declared ns differs from
/// the Python import name (e.g. `my_lib.clj` declaring `(ns my-lib)`).
#[pyfunction]
#[pyo3(name = "load_file_into_ns")]
pub fn py_load_file_into_ns(py: Python<'_>, path: &str, ns: PyObject) -> PyResult<PyObject> {
    let src = std::fs::read_to_string(path).map_err(|e| {
        errors::err(format!("load_file_into_ns: {}: {}", path, e))
    })?;
    load_clj_string(py, &src, &ns)
}

/// Python-callable: read the first complete top-level form from `path` and
/// return it (e.g. the `(ns …)` form). Returns None on EOF before a form.
/// Used by the pytest plugin for ns-suffix discovery without evaluating the
/// whole file.
#[pyfunction]
#[pyo3(name = "read_first_form_from_file")]
pub fn py_read_first_form_from_file(py: Python<'_>, path: &str) -> PyResult<Option<PyObject>> {
    let src = std::fs::read_to_string(path).map_err(|e| {
        errors::err(format!("read_first_form_from_file: {}: {}", path, e))
    })?;
    let mut s = Source::new(&src);
    dispatch::skip_ws_and_comments(&mut s);
    if s.at_eof() {
        return Ok(None);
    }
    let form = dispatch::read_one(&mut s, py)?;
    Ok(Some(form))
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_find_source_file, m)?)?;
    m.add_function(wrap_pyfunction!(py_load_file_into_ns, m)?)?;
    m.add_function(wrap_pyfunction!(py_read_first_form_from_file, m)?)?;
    Ok(())
}
