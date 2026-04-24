//! bound-fn* — capture the current binding frame and convey it to another thread.

use crate::binding::{empty_frame, Frame, BINDING_STACK};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

#[pyclass(module = "clojure._core", name = "BoundFn", frozen)]
pub struct BoundFn {
    snapshot: Frame,
    f: PyObject,
}

#[pymethods]
impl BoundFn {
    #[pyo3(signature = (*args))]
    fn __call__(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        let snap = self.snapshot.clone_ref(py);
        BINDING_STACK.with(|s| s.borrow_mut().push(snap));
        let result = self.f.bind(py).call1(args).map(|b| b.unbind());
        BINDING_STACK.with(|s| {
            s.borrow_mut().pop();
        });
        result
    }
}

#[pyfunction]
pub fn bound_fn_star(py: Python<'_>, f: PyObject) -> PyResult<Py<BoundFn>> {
    let snap: Frame = BINDING_STACK
        .with(|s| s.borrow().last().map(|top| top.clone_ref(py)))
        .unwrap_or_else(|| empty_frame(py).unwrap());
    Py::new(py, BoundFn { snapshot: snap, f })
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BoundFn>()?;
    m.add_function(wrap_pyfunction!(bound_fn_star, m)?)?;
    Ok(())
}
