use pyo3::prelude::*;

mod associative;
mod binding;
mod bound_fn;
pub mod collections;
mod counted;
mod dispatch;
mod exceptions;
mod ieditable_collection;
mod iequiv;
mod ifn;
mod ihasheq;
mod ilookup;
mod imeta;
mod indexed;
mod ipersistent_collection;
mod ipersistent_list;
mod ipersistent_map;
mod ipersistent_set;
mod ipersistent_stack;
mod ipersistent_vector;
mod iseq;
mod iseqable;
mod itransient_associative;
mod itransient_collection;
mod itransient_map;
mod itransient_set;
mod itransient_vector;
mod keyword;
mod namespace;
mod ns_ops;
pub(crate) mod binding_pmap;
mod printer;
mod protocol;
mod reader;
pub mod registry;
pub mod rt;
mod sequential;
mod seqs;
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
pub fn _core(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
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
    rt::register(py, m)?;
    ifn::install_builtin_fallback(py, m)?;
    iequiv::install_builtin_fallback(py, m)?;
    ihasheq::install_builtin_fallback(py, m)?;
    collections::register(py, m)?;
    seqs::register(py, m)?;
    reader::register(py, m)?;
    printer::register(py, m)?;
    Ok(())
}
