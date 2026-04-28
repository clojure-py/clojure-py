//! First-class Clojure value types built atop the heap layer.
//!
//! Each module here is a heap-allocated type with `register_type!`,
//! a public constructor, and `implements!` blocks for the protocols
//! it satisfies. Reference semantics follow the substrate: each
//! `Value` of one of these types holds one ref; `clojure_rt::dup` /
//! `clojure_rt::drop_value` manage it.

pub mod array_chunk;
pub mod array_map;
pub mod array_map_seq;
pub mod cons;
pub mod hash_map;
pub mod hash_map_seq;
pub mod keyword;
pub mod lazy_seq;
pub mod list;
pub mod map_entry;
pub mod reduced;
pub mod string;
pub mod symbol;
pub mod transient_array_map;
pub mod transient_hash_map;
pub mod transient_vector;
pub mod vec_seq;
pub mod vector;
