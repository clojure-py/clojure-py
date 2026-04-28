//! Cross-check: random op sequences applied through a
//! `TransientHashMap` should match the same sequence applied through
//! the persistent API.

use proptest::prelude::*;

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::hash_map::PersistentHashMap;

#[derive(Debug, Clone)]
enum Op {
    Assoc(i64, i64),
    Dissoc(i64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (-128..128i64, any::<i32>()).prop_map(|(k, v)| Op::Assoc(k, v as i64)),
        (-128..128i64).prop_map(Op::Dissoc),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn hash_map_transient_matches_persistent(
        ops in proptest::collection::vec(op_strategy(), 0..128)
    ) {
        init();

        // Persistent path.
        let mut p: Value = PersistentHashMap::from_kvs(&[]);
        for op in &ops {
            match *op {
                Op::Assoc(k, v) => {
                    let np = PersistentHashMap::assoc_kv(p, Value::int(k), Value::int(v));
                    drop_value(p);
                    p = np;
                }
                Op::Dissoc(k) => {
                    let np = PersistentHashMap::dissoc_k(p, Value::int(k));
                    drop_value(p);
                    p = np;
                }
            }
        }

        // Transient path.
        let mut t: Value = rt::transient(PersistentHashMap::from_kvs(&[]));
        for op in &ops {
            match *op {
                Op::Assoc(k, v) => {
                    let nt = rt::assoc_bang(t, Value::int(k), Value::int(v));
                    drop_value(t);
                    t = nt;
                }
                Op::Dissoc(k) => {
                    let nt = rt::dissoc_bang(t, Value::int(k));
                    drop_value(t);
                    t = nt;
                }
            }
        }
        let frozen = rt::persistent_(t);
        drop_value(t);

        prop_assert!(rt::equiv(p, frozen).as_bool().unwrap_or(false));
        drop_value(p);
        drop_value(frozen);
    }
}
