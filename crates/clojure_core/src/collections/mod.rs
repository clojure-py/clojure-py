//! Persistent collections.

pub mod plist;

pub use plist::{EmptyList, PersistentList};

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    plist::register(py, m)?;
    Ok(())
}
