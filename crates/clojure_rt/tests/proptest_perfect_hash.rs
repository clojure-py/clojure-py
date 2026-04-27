//! Property: for any set of distinct method_ids, the builder finds a
//! collision-free perfect-hash table and lookup returns the inserted
//! fn for every input.

use proptest::prelude::*;

use clojure_rt::dispatch::perfect_hash::PerTypeTable;
use clojure_rt::Value;

unsafe extern "C" fn fake(_: *const Value, _: usize) -> Value { Value::NIL }
fn fp() -> *const () { fake as *const () }

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn build_and_lookup(method_ids in proptest::collection::hash_set(1u32..=1_000_000, 1..=64)) {
        let entries: Vec<(u32, *const ())> = method_ids.iter().map(|m| (*m, fp())).collect();
        let table = PerTypeTable::build(&entries);
        for &mid in &method_ids {
            prop_assert!(table.lookup(mid).is_some(), "miss on {mid}");
        }
        // A method_id not in the set must miss (or coincidentally hit the
        // empty-slot guard).
        let absent = method_ids.iter().max().copied().unwrap_or(0).saturating_add(1);
        if !method_ids.contains(&absent) {
            prop_assert!(table.lookup(absent).is_none());
        }
    }
}
