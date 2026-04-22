use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IFn", extend_via_metadata = false)]
pub trait IFn {
    fn invoke0(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn invoke1(&self, py: Python<'_>, a0: PyObject) -> PyResult<PyObject>;
    fn invoke2(&self, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject>;
    fn invoke3(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject) -> PyResult<PyObject>;
    fn invoke4(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject) -> PyResult<PyObject>;
    fn invoke5(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject) -> PyResult<PyObject>;
    fn invoke6(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject) -> PyResult<PyObject>;
    fn invoke7(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject) -> PyResult<PyObject>;
    fn invoke8(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject) -> PyResult<PyObject>;
    fn invoke9(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject) -> PyResult<PyObject>;
    fn invoke10(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject) -> PyResult<PyObject>;
    fn invoke11(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject) -> PyResult<PyObject>;
    fn invoke12(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject) -> PyResult<PyObject>;
    fn invoke13(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject) -> PyResult<PyObject>;
    fn invoke14(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject) -> PyResult<PyObject>;
    fn invoke15(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject) -> PyResult<PyObject>;
    fn invoke16(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject) -> PyResult<PyObject>;
    fn invoke17(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject) -> PyResult<PyObject>;
    fn invoke18(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject) -> PyResult<PyObject>;
    fn invoke19(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject) -> PyResult<PyObject>;
    fn invoke20(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject, a4: PyObject, a5: PyObject, a6: PyObject, a7: PyObject, a8: PyObject, a9: PyObject, a10: PyObject, a11: PyObject, a12: PyObject, a13: PyObject, a14: PyObject, a15: PyObject, a16: PyObject, a17: PyObject, a18: PyObject, a19: PyObject) -> PyResult<PyObject>;
    fn invoke_variadic(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject>;
}
