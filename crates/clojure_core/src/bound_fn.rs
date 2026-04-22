//! bound-fn* — capture the current binding frame and convey it to another thread.

use crate::binding::BINDING_STACK;
use crate::binding_pmap::PMap;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "BoundFn", frozen)]
pub struct BoundFn {
    snapshot: PMap,
    f: PyObject,
}

#[pymethods]
impl BoundFn {
    #[pyo3(signature = (*args))]
    fn __call__(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        BINDING_STACK.with(|s| s.borrow_mut().push(self.snapshot.clone()));
        let result = self.f.bind(py).call1(args).map(|b| b.unbind());
        BINDING_STACK.with(|s| {
            s.borrow_mut().pop();
        });
        result
    }
}

#[pyfunction]
pub fn bound_fn_star(py: Python<'_>, f: PyObject) -> PyResult<Py<BoundFn>> {
    let snap = BINDING_STACK.with(|s| s.borrow().last().cloned().unwrap_or_default());
    Py::new(
        py,
        BoundFn {
            snapshot: snap,
            f,
        },
    )
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BoundFn>()?;
    m.add_function(wrap_pyfunction!(bound_fn_star, m)?)?;
    Ok(())
}
