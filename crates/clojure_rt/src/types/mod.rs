//! First-class Clojure value types built atop the heap layer.
//!
//! Each module here is a heap-allocated type with `register_type!`,
//! a public constructor, and `implements!` blocks for the protocols
//! it satisfies. Reference semantics follow the substrate: each
//! `Value` of one of these types holds one ref; `clojure_rt::dup` /
//! `clojure_rt::drop_value` manage it.

pub mod keyword;
pub mod list;
pub mod string;
pub mod symbol;
