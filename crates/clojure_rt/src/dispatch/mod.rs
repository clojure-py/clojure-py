//! Polymorphic dispatch: tier-1 IC, tier-2 per-type perfect-hash table,
//! tier-3 global stub cache, plus the slow-path resolver.

pub mod ic;
pub mod perfect_hash;
pub mod stub_cache;

use crate::value::Value;

pub type MethodFn = unsafe extern "C" fn(args: *const Value, n: usize) -> Value;
