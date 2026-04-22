use pyo3::prelude::*;

mod binding;
mod bound_fn;
mod dispatch;
mod exceptions;
mod ifn;
mod ilookup;
mod keyword;
mod namespace;
mod ns_ops;
pub(crate) mod pmap;
mod protocol;
pub mod registry;
pub mod rt;
mod symbol;
mod test_protocols;
mod var;

pub use clojure_core_macros::{implements, protocol};
pub use exceptions::{ArityException, IllegalArgumentException, IllegalStateException};
pub use ifn::IFn;
pub use keyword::Keyword;
pub use protocol::{MethodCache, Protocol, ProtocolMethod};
pub use symbol::Symbol;
pub use var::Var;

#[pymodule]
fn _core(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    exceptions::register(py, m)?;
    symbol::register(py, m)?;
    keyword::register(py, m)?;
    protocol::register(py, m)?;
    var::register(py, m)?;
    namespace::register(py, m)?;
    ns_ops::register(py, m)?;
    binding::register(py, m)?;
    bound_fn::register(py, m)?;
    registry::register_all(py, m)?;
    registry::install_all_extends(py, m)?;
    ilookup::install_builtin_fallback(py, m)?;
    rt::init(py, m)?;
    ifn::install_builtin_fallback(py, m)?;
    Ok(())
}
