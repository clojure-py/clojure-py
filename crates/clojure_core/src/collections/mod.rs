//! Persistent collections.

pub mod plist;
pub mod pvector;
pub mod pvector_node;

pub use plist::{EmptyList, PersistentList};
pub use pvector::PersistentVector;

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    plist::register(py, m)?;
    pvector::register(py, m)?;
    Ok(())
}
