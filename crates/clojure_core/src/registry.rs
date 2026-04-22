use pyo3::prelude::*;

/// A single protocol's registration entry. Collected via the `inventory`
/// crate at link time; iterated at module-init to build each Protocol and
/// install it on the Python module.
pub struct ProtocolRegistration {
    pub build_and_register: fn(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()>,
}

inventory::collect!(ProtocolRegistration);

pub fn register_all(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    for r in inventory::iter::<ProtocolRegistration> {
        (r.build_and_register)(py, m)?;
    }
    Ok(())
}

/// Per-type-extension registration. Collected alongside ProtocolRegistrations;
/// installed AFTER all protocols exist (so lookup by protocol name works).
pub struct ExtendRegistration {
    pub install: fn(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()>,
}

inventory::collect!(ExtendRegistration);

pub fn install_all_extends(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    for r in inventory::iter::<ExtendRegistration> {
        (r.install)(py, m)?;
    }
    Ok(())
}
