use pyo3::prelude::*;

mod dispatch;
mod exceptions;
mod ifn;
mod keyword;
mod protocol;
pub mod registry;
mod symbol;

pub use clojure_core_macros::{implements, protocol};
pub use exceptions::{ArityException, IllegalArgumentException, IllegalStateException};
pub use ifn::IFn;
pub use keyword::Keyword;
pub use protocol::{MethodCache, Protocol, ProtocolMethod};
pub use symbol::Symbol;

#[pymodule]
fn _core(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    exceptions::register(py, m)?;
    symbol::register(py, m)?;
    keyword::register(py, m)?;
    protocol::register(py, m)?;
    registry::register_all(py, m)?;
    Ok(())
}
