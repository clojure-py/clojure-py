//! Proptest: random op sequences on `PersistentHashMap` cross-checked
//! against a `std::collections::HashMap<i64, i64>` reference oracle.

use std::collections::HashMap;

use proptest::prelude::*;

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::hash_map::PersistentHashMap;

#[derive(Debug, Clone)]
enum Op {
    Assoc(i64, i64),
    Dissoc(i64),
    Lookup(i64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        // Use a wide-enough key range to actually populate the trie.
        (-128..128i64, any::<i32>()).prop_map(|(k, v)| Op::Assoc(k, v as i64)),
        (-128..128i64).prop_map(Op::Dissoc),
        (-128..128i64).prop_map(Op::Lookup),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn random_op_sequence_matches_std_hashmap(
        ops in proptest::collection::vec(op_strategy(), 0..256)
    ) {
        init();
        let mut oracle: HashMap<i64, i64> = HashMap::new();
        let mut m: Value = PersistentHashMap::from_kvs(&[]);

        for op in &ops {
            match *op {
                Op::Assoc(k, v) => {
                    oracle.insert(k, v);
                    let nm = PersistentHashMap::assoc_kv(m, Value::int(k), Value::int(v));
                    drop_value(m);
                    m = nm;
                }
                Op::Dissoc(k) => {
                    oracle.remove(&k);
                    let nm = PersistentHashMap::dissoc_k(m, Value::int(k));
                    drop_value(m);
                    m = nm;
                }
                Op::Lookup(k) => {
                    let r = rt::get(m, Value::int(k));
                    match oracle.get(&k) {
                        Some(want) => prop_assert_eq!(r.as_int(), Some(*want)),
                        None       => prop_assert!(r.is_nil()),
                    }
                    drop_value(r);
                }
            }
            // Cross-check after every step.
            prop_assert_eq!(rt::count(m).as_int().unwrap(), oracle.len() as i64);
            for (ek, ev) in &oracle {
                let r = rt::get(m, Value::int(*ek));
                prop_assert_eq!(r.as_int(), Some(*ev));
                drop_value(r);
            }
        }
        drop_value(m);
    }
}
