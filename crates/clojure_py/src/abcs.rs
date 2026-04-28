//! Bootstrap of `collections.abc` ABCs as inheritance metadata for
//! protocol dispatch. Each ABC is interned via `crate::intern::register_abc`
//! so future per-class interning walks pick it up; impls are installed
//! against the ABC's TypeId via `clojure_rt::protocol::extend_type`.

use clojure_rt::value::TypeId;
use once_cell::sync::OnceCell;
use pyo3::types::PyAnyMethods;
use pyo3::Python;

use crate::intern::register_abc;

pub static SIZED_TYPE_ID:    OnceCell<TypeId> = OnceCell::new();
pub static CALLABLE_TYPE_ID: OnceCell<TypeId> = OnceCell::new();

/// Intern the ABCs we map to clojure_rt protocols, then install the
/// protocol impls against those ABC TypeIds. GIL-required.
pub fn init(py: Python<'_>) {
    let abc_module = py
        .import("collections.abc")
        .expect("collections.abc unavailable");

    let sized = abc_module
        .getattr("Sized")
        .expect("collections.abc.Sized missing");
    let sized_tid = register_abc(py, &sized);
    SIZED_TYPE_ID.set(sized_tid).ok();
    crate::counted::install(sized_tid);

    let callable = abc_module
        .getattr("Callable")
        .expect("collections.abc.Callable missing");
    let callable_tid = register_abc(py, &callable);
    CALLABLE_TYPE_ID.set(callable_tid).ok();
    crate::ifn::install(callable_tid);
}
