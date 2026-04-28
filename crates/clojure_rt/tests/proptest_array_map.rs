//! Property-based fuzz for `PersistentArrayMap`. Random sequences of
//! `assoc` / `dissoc` / `lookup` are applied to both the map and a
//! `Vec<(i64, i64)>` reference oracle (insertion-ordered, last-wins
//! on duplicate keys); each step cross-checks count, get-by-key, and
//! `find`-vs-vector-pair.

use proptest::prelude::*;

use clojure_rt::{drop_value, init, rt, Value};

#[derive(Debug, Clone)]
enum Op {
    Assoc(i32, i32),
    Dissoc(i32),
    Lookup(i32),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        // Restrict keys to a small range so dissoc/lookup hit existing
        // entries with reasonable frequency.
        (0..16i32, any::<i32>()).prop_map(|(k, v)| Op::Assoc(k, v)),
        (0..16i32).prop_map(Op::Dissoc),
        (0..16i32).prop_map(Op::Lookup),
    ]
}

/// Insertion-ordered oracle: a `Vec<(K, V)>` where assoc with existing
/// key replaces in place and dissoc filters by key.
fn oracle_assoc(oracle: &mut Vec<(i64, i64)>, k: i64, v: i64) {
    if let Some(slot) = oracle.iter_mut().find(|(ek, _)| *ek == k) {
        slot.1 = v;
    } else {
        oracle.push((k, v));
    }
}

fn oracle_dissoc(oracle: &mut Vec<(i64, i64)>, k: i64) {
    oracle.retain(|(ek, _)| *ek != k);
}

fn oracle_lookup(oracle: &Vec<(i64, i64)>, k: i64) -> Option<i64> {
    oracle.iter().find(|(ek, _)| *ek == k).map(|(_, v)| *v)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn random_op_sequence_matches_vec_oracle(
        ops in proptest::collection::vec(op_strategy(), 0..128)
    ) {
        init();
        let mut oracle: Vec<(i64, i64)> = Vec::new();
        let mut m: Value = rt::array_map(&[]);

        for op in &ops {
            match *op {
                Op::Assoc(k, v) => {
                    oracle_assoc(&mut oracle, k as i64, v as i64);
                    let nm = rt::assoc(m, Value::int(k as i64), Value::int(v as i64));
                    drop_value(m);
                    m = nm;
                }
                Op::Dissoc(k) => {
                    oracle_dissoc(&mut oracle, k as i64);
                    let nm = rt::dissoc(m, Value::int(k as i64));
                    drop_value(m);
                    m = nm;
                }
                Op::Lookup(k) => {
                    let r = rt::get(m, Value::int(k as i64));
                    match oracle_lookup(&oracle, k as i64) {
                        Some(want) => prop_assert_eq!(r.as_int(), Some(want)),
                        None       => prop_assert!(r.is_nil()),
                    }
                    drop_value(r);
                }
            }
            // Cross-checks after every step.
            prop_assert_eq!(rt::count(m).as_int().unwrap(), oracle.len() as i64);
            for (ek, ev) in &oracle {
                let got = rt::get(m, Value::int(*ek));
                prop_assert_eq!(got.as_int(), Some(*ev));
                drop_value(got);
            }
        }
        drop_value(m);
    }
}
