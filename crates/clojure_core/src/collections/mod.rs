//! Persistent collections.

pub mod plist;
pub mod pvector;
pub mod pvector_node;
pub mod map_entry;
pub mod phashmap;
pub mod phashmap_node;

pub use plist::{EmptyList, PersistentList};
pub use pvector::PersistentVector;
pub use map_entry::MapEntry;
pub use phashmap::PersistentHashMap;

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    plist::register(py, m)?;
    pvector::register(py, m)?;
    map_entry::register(py, m)?;
    phashmap::register(py, m)?;
    Ok(())
}
