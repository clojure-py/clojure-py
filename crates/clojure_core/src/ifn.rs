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

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pyo3::types::{PyCFunction, PyDict};

    let ifn_any = m.getattr("IFn")?;
    let ifn_proto: &Bound<'_, crate::Protocol> = ifn_any.downcast()?;

    let fallback = PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = args.py();
            // args = (protocol, method_key, target)
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.downcast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            if !target.is_callable() {
                return Ok(py.None());
            }

            // Build the generic invoke_variadic impl: it receives (self, *a) and calls self(*a).
            let inv = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner_args: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner_args.py();
                    let self_obj = inner_args.get_item(0)?;
                    let n = inner_args.len();
                    let mut rest: Vec<Py<PyAny>> = Vec::with_capacity(n.saturating_sub(1));
                    for i in 1..n {
                        rest.push(inner_args.get_item(i)?.unbind());
                    }
                    let rest_tup = PyTuple::new(py, &rest)?;
                    Ok(self_obj.call1(rest_tup)?.unbind())
                },
            )?;

            // Install the same callable under every invoke* name. For fixed arities, the
            // generated dispatch wrapper unpacks positional args, so all arities resolve
            // to the same variadic body.
            let impls = PyDict::new(py);
            for key in [
                "invoke0", "invoke1", "invoke2", "invoke3", "invoke4", "invoke5",
                "invoke6", "invoke7", "invoke8", "invoke9", "invoke10", "invoke11",
                "invoke12", "invoke13", "invoke14", "invoke15", "invoke16", "invoke17",
                "invoke18", "invoke19", "invoke20", "invoke_variadic",
            ] {
                impls.set_item(key, &inv)?;
            }

            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    ifn_proto.call_method1("set_fallback", (fallback,))?;
    Ok(())
}
