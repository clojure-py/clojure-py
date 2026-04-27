//! `Counted` impl for the `PyObject` primitive — bridges to Python's
//! `__len__`. Borrowed reference semantics: the Value's payload is a
//! non-owning pointer; the calling Python frame holds the actual ref.

use clojure_rt::primitives::*;
use clojure_rt::protocols::counted::Counted;
use clojure_rt::Value;
use pyo3::ffi as pyffi;
use pyo3::types::PyAnyMethods;
use pyo3::{Bound, PyAny, Python};

clojure_rt_macros::implements! {
    impl Counted for PyObject {
        fn count(this: Value) -> Value {
            Python::attach(|py| {
                let ptr = this.payload as *mut pyffi::PyObject;
                if ptr.is_null() {
                    return clojure_rt::exception::make_foreign(
                        "Counted/count: null Python object pointer".to_string(),
                    );
                }
                // `from_borrowed_ptr` increfs and gives us a Bound that
                // decrefs on drop — leaving the caller's reference
                // intact. One atomic pair per dispatch; acceptable.
                let obj: Bound<'_, PyAny> = unsafe {
                    Bound::from_borrowed_ptr(py, ptr)
                };
                match obj.len() {
                    Ok(n)    => Value::int(n as i64),
                    Err(err) => crate::exception::pyerr_to_value(py, err),
                }
            })
        }
    }
}
