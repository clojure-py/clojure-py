use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core/IPersistentList", extend_via_metadata = false)]
pub trait IPersistentList {}
