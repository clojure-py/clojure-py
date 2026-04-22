//! Tree-walking evaluator.

pub mod env;
pub mod errors;
pub mod fn_value;
pub mod invoke;
pub mod resolve;
pub mod special_forms;

use env::Env;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Core eval function — dispatches on form kind.
pub fn eval(py: Python<'_>, form: PyObject, env: &Env) -> PyResult<PyObject> {
    let b = form.bind(py);

    // Atoms self-evaluate: nil, bool, int, float, string, keyword.
    if form.is_none(py) { return Ok(form); }
    if b.downcast::<pyo3::types::PyBool>().is_ok() { return Ok(form); }
    if b.downcast::<pyo3::types::PyInt>().is_ok() { return Ok(form); }
    if b.downcast::<pyo3::types::PyFloat>().is_ok() { return Ok(form); }
    if b.downcast::<pyo3::types::PyString>().is_ok() { return Ok(form); }
    if b.downcast::<crate::keyword::Keyword>().is_ok() { return Ok(form); }

    // Symbol: resolve via env (locals; ns fallback in E3).
    if b.downcast::<crate::symbol::Symbol>().is_ok() {
        return resolve::resolve_symbol(py, form, env);
    }

    // PersistentVector: eval each element, build new vector.
    if let Ok(pv) = b.downcast::<crate::collections::pvector::PersistentVector>() {
        let v = pv.get();
        let mut evald: Vec<PyObject> = Vec::with_capacity(v.cnt as usize);
        for i in 0..(v.cnt as usize) {
            let e = v.nth_internal_pub(py, i)?;
            evald.push(eval(py, e, env)?);
        }
        let tup = pyo3::types::PyTuple::new(py, &evald)?;
        return Ok(crate::collections::pvector::vector(py, tup)?.into_any());
    }

    // PersistentHashMap / PArrayMap: eval each k and v.
    if b.downcast::<crate::collections::phashmap::PersistentHashMap>().is_ok()
        || b.downcast::<crate::collections::parraymap::PersistentArrayMap>().is_ok()
    {
        let mut pairs: Vec<PyObject> = Vec::new();
        let iter = b.try_iter()?;
        for item in iter {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            pairs.push(eval(py, k, env)?);
            pairs.push(eval(py, v, env)?);
        }
        let tup = pyo3::types::PyTuple::new(py, &pairs)?;
        return crate::collections::parraymap::array_map(py, tup);
    }

    // PersistentHashSet: eval each element.
    if b.downcast::<crate::collections::phashset::PersistentHashSet>().is_ok() {
        let mut items: Vec<PyObject> = Vec::new();
        let iter = b.try_iter()?;
        for item in iter {
            items.push(eval(py, item?.unbind(), env)?);
        }
        let tup = pyo3::types::PyTuple::new(py, &items)?;
        return Ok(crate::collections::phashset::hash_set(py, tup)?.into_any());
    }

    // PersistentList: special form OR invocation.
    if let Ok(pl) = b.downcast::<crate::collections::plist::PersistentList>() {
        let head = pl.get().head.clone_ref(py);
        // Check special form.
        if let Some(name) = special_forms::lookup(&head, py) {
            return special_forms::dispatch(py, name, form, env);
        }
        // Plain invocation.
        return invoke::eval_invocation(py, form, env);
    }

    // EmptyList evals to itself.
    if b.downcast::<crate::collections::plist::EmptyList>().is_ok() {
        return Ok(form);
    }

    // Everything else evals to itself (best-effort).
    Ok(form)
}

/// Create an Env with clojure.user as the current namespace.
fn default_env(py: Python<'_>) -> PyResult<Env> {
    // Ensure clojure.user exists.
    let sym = crate::symbol::Symbol::new(None, std::sync::Arc::from("clojure.user"));
    let sym_py = Py::new(py, sym)?;
    let ns = crate::namespace::create_ns(py, sym_py)?;
    Ok(Env::new(ns))
}

#[pyfunction]
#[pyo3(name = "eval")]
pub fn py_eval(py: Python<'_>, form: PyObject) -> PyResult<PyObject> {
    let env = default_env(py)?;
    eval(py, form, &env)
}

#[pyfunction]
#[pyo3(name = "eval_string")]
pub fn py_eval_string(py: Python<'_>, source: &str) -> PyResult<PyObject> {
    let form = crate::reader::read_string_py(py, source)?;
    let env = default_env(py)?;
    eval(py, form, &env)
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(py, m)?;
    fn_value::register(py, m)?;
    m.add_function(wrap_pyfunction!(py_eval, m)?)?;
    m.add_function(wrap_pyfunction!(py_eval_string, m)?)?;
    Ok(())
}
