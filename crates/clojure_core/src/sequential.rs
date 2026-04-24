use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core/Sequential", extend_via_metadata = false, emit_fn_primary = true)]
pub trait Sequential {}
