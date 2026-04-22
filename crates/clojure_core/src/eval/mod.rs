//! Evaluation entry points. The tree walker is gone — `eval` compiles the
//! form to bytecode and runs it on the VM.

pub mod core_shims;
pub mod errors;
pub mod fn_value;
pub mod macros;

use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// Compile and run a single form. The form is wrapped as a 0-arity method.
pub fn eval(py: Python<'_>, form: PyObject, current_ns: PyObject) -> PyResult<PyObject> {
    let (method, pool) = crate::compiler::compile_top_level(py, form, current_ns)?;
    crate::vm::run(py, &method, &pool, &[], &[])
}

fn default_ns(py: Python<'_>) -> PyResult<PyObject> {
    let sym = crate::symbol::Symbol::new(None, std::sync::Arc::from("clojure.user"));
    let sym_py = Py::new(py, sym)?;
    let ns = crate::namespace::create_ns(py, sym_py)?;
    Ok(ns)
}

#[pyfunction]
#[pyo3(name = "eval")]
pub fn py_eval(py: Python<'_>, form: PyObject) -> PyResult<PyObject> {
    let ns = default_ns(py)?;
    eval(py, form, ns)
}

#[pyfunction]
#[pyo3(name = "eval_string")]
pub fn py_eval_string(py: Python<'_>, source: &str) -> PyResult<PyObject> {
    let form = crate::reader::read_string_py(py, source)?;
    let ns = default_ns(py)?;
    eval(py, form, ns)
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(py, m)?;
    fn_value::register(py, m)?;
    m.add_function(wrap_pyfunction!(py_eval, m)?)?;
    m.add_function(wrap_pyfunction!(py_eval_string, m)?)?;
    core_shims::init(py, m)?;
    Ok(())
}
