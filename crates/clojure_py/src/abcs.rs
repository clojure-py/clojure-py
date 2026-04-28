//! Bootstrap of `collections.abc` ABCs as inheritance metadata for
//! protocol dispatch. Each ABC is interned via `crate::intern::register_abc`
//! so future per-class interning walks pick it up; impls are installed
//! against the ABC's TypeId via `clojure_rt::protocol::extend_type`.

use clojure_rt::protocol::{extend_type, ProtocolMethod};
use clojure_rt::protocols::persistent_map::IPersistentMap;
use clojure_rt::protocols::persistent_set::IPersistentSet;
use clojure_rt::value::{TypeId, Value};
use once_cell::sync::OnceCell;
use pyo3::types::PyAnyMethods;
use pyo3::Python;

use crate::intern::register_abc;

pub static SIZED_TYPE_ID:    OnceCell<TypeId> = OnceCell::new();
pub static CALLABLE_TYPE_ID: OnceCell<TypeId> = OnceCell::new();
pub static SET_TYPE_ID:      OnceCell<TypeId> = OnceCell::new();
pub static MAPPING_TYPE_ID:  OnceCell<TypeId> = OnceCell::new();

/// No-op fn pointer used to mark an ABC as satisfying a marker
/// protocol. Presence in the type's per-type table is what
/// `clojure_rt::protocol::satisfies` detects; the body never runs.
unsafe extern "C" fn marker_noop(_args: *const Value, _n: usize) -> Value {
    Value::NIL
}

#[inline]
fn install_marker(tid: TypeId, marker: &ProtocolMethod) {
    extend_type(tid, marker, marker_noop);
}

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

    // Marker bindings: Python set/frozenset → IPersistentSet (for
    // `set?`); Python dict → IPersistentMap (for `map?`). Mutating
    // operations stay unimplemented — these are pure predicate
    // affordances so the user-level type tests answer correctly.
    let set_abc = abc_module
        .getattr("Set")
        .expect("collections.abc.Set missing");
    let set_tid = register_abc(py, &set_abc);
    SET_TYPE_ID.set(set_tid).ok();
    install_marker(set_tid, &IPersistentSet::MARKER);

    let mapping = abc_module
        .getattr("Mapping")
        .expect("collections.abc.Mapping missing");
    let mapping_tid = register_abc(py, &mapping);
    MAPPING_TYPE_ID.set(mapping_tid).ok();
    install_marker(mapping_tid, &IPersistentMap::MARKER);
}
