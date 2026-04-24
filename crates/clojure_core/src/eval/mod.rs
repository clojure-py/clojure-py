//! Evaluation entry points. The tree walker is gone — `eval` compiles the
//! form to bytecode and runs it on the VM.

pub mod core_shims;
pub mod errors;
pub mod fn_value;
pub mod load;
pub mod rt_ns;

use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Compile and run a single form. The form is wrapped as a 0-arity method.
///
/// Top-level `(do f1 f2 …)` is special-cased: each sub-form is eval'd
/// sequentially, matching vanilla's `Compiler.eval` — this lets forms like
/// `(defmacro m [] …)` take effect before the next form compiles against
/// them, and `(in-ns …)` / `(def …)` side-effect properly when chained.
/// Nested `do` inside expressions still compiles as one unit (handled by
/// `compile_do` in the bytecode compiler).
pub fn eval(py: Python<'_>, form: PyObject, current_ns: PyObject) -> PyResult<PyObject> {
    if let Some(forms) = unwrap_top_level_do(py, &form)? {
        let mut last: PyObject = py.None();
        for f in forms {
            // Re-resolve current_ns each iteration — an earlier form may
            // have been `(in-ns ...)` / `(ns ...)`, promoting a new ns.
            let ns = current_compilation_ns(py)?;
            let (method, pool) = crate::compiler::compile_top_level(py, f, ns)?;
            last = crate::vm::run(py, &method, &pool, &[], &[], None)?;
            promote_pending_ns_switch(py);
        }
        let _ = current_ns; // unused in the split path
        return Ok(last);
    }
    let (method, pool) = crate::compiler::compile_top_level(py, form, current_ns)?;
    crate::vm::run(py, &method, &pool, &[], &[], None)
}

/// Returns `Some(rest-of-forms)` when `form` is a `(do f1 f2 ...)` list,
/// else None. Fully-qualified (`clojure.core/do`) heads are NOT treated as
/// top-level `do` splits — only the bare symbol matches the compiler's
/// special-form recognition, and that's what vanilla splits on too.
fn unwrap_top_level_do(py: Python<'_>, form: &PyObject) -> PyResult<Option<Vec<PyObject>>> {
    let b = form.bind(py);
    let Ok(pl) = b.cast::<crate::collections::plist::PersistentList>() else {
        return Ok(None);
    };
    let head = pl.get().head.clone_ref(py);
    let hb = head.bind(py);
    let Ok(sym_ref) = hb.cast::<crate::symbol::Symbol>() else {
        return Ok(None);
    };
    let s = sym_ref.get();
    if s.ns.is_some() || s.name.as_ref() != "do" {
        return Ok(None);
    }
    let mut out: Vec<PyObject> = Vec::new();
    let mut cur: PyObject = pl.get().tail.clone_ref(py);
    loop {
        let sb = crate::rt::seq(py, cur.clone_ref(py))?;
        if sb.is_none(py) { break; }
        out.push(crate::rt::first(py, sb.clone_ref(py))?);
        cur = crate::rt::next_(py, sb)?;
        if cur.is_none(py) { break; }
    }
    Ok(Some(out))
}

fn default_ns(py: Python<'_>) -> PyResult<PyObject> {
    let sym = crate::symbol::Symbol::new(None, std::sync::Arc::from("clojure.user"));
    let sym_py = Py::new(py, sym)?;
    let ns = crate::namespace::create_ns(py, sym_py)?;
    // Auto-refer clojure.core (matches vanilla's REPL bootstrap) so plain
    // `+`, `map`, `let`, …, as well as syntax-quote resolution of those
    // names, find the core Vars. Only refer once — repeated calls to
    // `default_ns` are a no-op if clojure.user already has refers.
    let refers_dict = ns.bind(py).getattr("__clj_refers__")?;
    let refers_is_empty: bool = refers_dict.call_method0("__len__")?.extract::<usize>()? == 0;
    if refers_is_empty {
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        if let Ok(core) = modules.get_item("clojure.core") {
            let core_dict = core.getattr("__dict__")?;
            let core_dict = core_dict.cast::<pyo3::types::PyDict>()?;
            for (k, v) in core_dict.iter() {
                let key_s: String = match k.extract() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if key_s.starts_with("__") && key_s.ends_with("__") { continue; }
                if let Ok(var) = v.cast::<crate::var::Var>() {
                    // Use ns_ops::refer so __clj_refers__ is populated too.
                    let sym = crate::symbol::Symbol::new(None, std::sync::Arc::from(key_s.as_str()));
                    let sym_py = Py::new(py, sym)?;
                    crate::ns_ops::refer(py, ns.clone_ref(py), sym_py, var.clone().unbind())?;
                }
            }
        }
    }
    Ok(ns)
}

/// Fetch the current compilation namespace — either the one most
/// recently set by `(in-ns ...)` / `(ns ...)` (observed via the
/// `CURRENT_LOAD_NS` thread-local) or clojure.user as a fallback.
/// Also seeds `CURRENT_LOAD_NS` on first use so subsequent reads see
/// a stable ns.
fn current_compilation_ns(py: Python<'_>) -> PyResult<PyObject> {
    if let Some(ns) = load::CURRENT_LOAD_NS.with(|c| c.borrow().as_ref().map(|n| n.clone_ref(py))) {
        return Ok(ns);
    }
    let ns = default_ns(py)?;
    load::CURRENT_LOAD_NS.with(|c| *c.borrow_mut() = Some(ns.clone_ref(py)));
    Ok(ns)
}

/// After evaluating a form, check whether `(in-ns ...)` / `(ns ...)`
/// asked for a namespace switch. If so, promote LOAD_NS_OVERRIDE into
/// CURRENT_LOAD_NS and `*ns*`'s root so subsequent evals compile against it.
fn promote_pending_ns_switch(py: Python<'_>) {
    if let Some(new_ns) = load::LOAD_NS_OVERRIDE.with(|c| c.borrow_mut().take()) {
        load::CURRENT_LOAD_NS.with(|c| *c.borrow_mut() = Some(new_ns.clone_ref(py)));
        if let Ok(v) = load::ns_var(py) {
            let _ = v.bind(py).call_method1("bind_root", (new_ns,));
        }
    }
}

/// Ensure `*ns*`'s Var root matches the target compilation ns. Does NOT touch
/// the thread-binding stack — we don't want eval_string to interfere with
/// user-driven `binding` / `clone-thread-binding-frame` / `reset-thread-binding-frame`.
/// Callers who want stacked bindings (e.g. `load_clj_string`) push their
/// own frames.
fn sync_ns_root(py: Python<'_>, ns: &PyObject) -> PyResult<()> {
    let v = load::ns_var(py)?;
    let current = v.bind(py).call_method0("deref")?;
    if !crate::rt::identical(py, current.unbind(), ns.clone_ref(py)) {
        v.bind(py).call_method1("bind_root", (ns.clone_ref(py),))?;
    }
    Ok(())
}

#[pyfunction]
#[pyo3(name = "eval")]
pub fn py_eval(py: Python<'_>, form: PyObject) -> PyResult<PyObject> {
    let ns = current_compilation_ns(py)?;
    sync_ns_root(py, &ns)?;
    let result = eval(py, form, ns)?;
    promote_pending_ns_switch(py);
    Ok(result)
}

#[pyfunction]
#[pyo3(name = "eval_string")]
pub fn py_eval_string(py: Python<'_>, source: &str) -> PyResult<PyObject> {
    // Ensure `*ns*` (root) matches the compilation ns BEFORE reading, so the
    // reader's ::kw auto-resolution picks up the right namespace.
    let ns = current_compilation_ns(py)?;
    sync_ns_root(py, &ns)?;
    let form = crate::reader::read_string_py(py, source)?;
    let result = eval(py, form, ns)?;
    promote_pending_ns_switch(py);
    Ok(result)
}

/// Return the name (as a Python string) of the current compilation ns.
/// Defaults to `"clojure.user"` if no REPL-style eval has switched it yet.
#[pyfunction]
#[pyo3(name = "current_ns_name")]
pub fn py_current_ns_name(py: Python<'_>) -> PyResult<String> {
    let ns = current_compilation_ns(py)?;
    let name = ns.bind(py).getattr("__name__")?;
    Ok(name.extract::<String>()?)
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(py, m)?;
    fn_value::register(py, m)?;
    m.add_function(wrap_pyfunction!(py_eval, m)?)?;
    m.add_function(wrap_pyfunction!(py_eval_string, m)?)?;
    m.add_function(wrap_pyfunction!(py_current_ns_name, m)?)?;
    core_shims::init(py, m)?;
    rt_ns::init(py, m)?;
    load::register(py, m)?;
    load::load_core_clj(py)?;
    Ok(())
}
