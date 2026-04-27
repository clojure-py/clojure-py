//! Property-based fuzz for `PersistentVector`. Random sequences of
//! `cons`, `pop`, and `assoc` are applied to both a `PersistentVector`
//! and a `Vec<i64>` reference oracle; on each step we cross-check
//! `count`, `nth`, and a full `seq`-walked materialization.
//!
//! Refcount hygiene matters: each test owns one ref to the current
//! vector and explicitly drops it when stepping forward.

use proptest::prelude::*;

use clojure_rt::{drop_value, init, rt, Value};

#[derive(Debug, Clone)]
enum Op {
    Cons(i64),
    Pop,
    Assoc(usize, i64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<i32>().prop_map(|n| Op::Cons(n as i64)),
        Just(Op::Pop),
        (any::<u16>(), any::<i32>()).prop_map(|(i, v)| Op::Assoc(i as usize, v as i64)),
    ]
}

fn materialize(v: Value) -> Vec<i64> {
    let mut out = Vec::new();
    let mut s = rt::seq(v);
    while !s.is_nil() {
        let f = rt::first(s);
        out.push(f.as_int().expect("int"));
        drop_value(f);
        let n = rt::next(s);
        drop_value(s);
        s = n;
    }
    drop_value(s);
    out
}

fn vec_to_native(v: Value) -> Vec<i64> {
    let n = rt::count(v).as_int().unwrap();
    (0..n)
        .map(|i| {
            let r = rt::nth(v, Value::int(i));
            let x = r.as_int().unwrap();
            drop_value(r);
            x
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Run a random op sequence in lockstep on a PersistentVector and
    /// a `Vec<i64>` oracle; assert `count`, `nth`, and seq
    /// materialization match at every step.
    #[test]
    fn random_op_sequence_matches_vec_oracle(
        ops in proptest::collection::vec(op_strategy(), 0..128)
    ) {
        init();
        let mut oracle: Vec<i64> = Vec::new();
        let mut v: Value = rt::vector(&[]);

        for op in &ops {
            match *op {
                Op::Cons(x) => {
                    oracle.push(x);
                    let nv = rt::conj(v, Value::int(x));
                    drop_value(v);
                    v = nv;
                }
                Op::Pop => {
                    if oracle.is_empty() {
                        // Pop on empty is an exception in our impl; oracle
                        // skips. Confirm exception, then carry on.
                        let r = rt::pop(v);
                        prop_assert!(r.is_exception());
                        drop_value(r);
                    } else {
                        oracle.pop();
                        let nv = rt::pop(v);
                        prop_assert!(!nv.is_exception(), "pop returned exception unexpectedly");
                        drop_value(v);
                        v = nv;
                    }
                }
                Op::Assoc(i, x) => {
                    if oracle.is_empty() {
                        // Assoc on empty at index 0 = extend; otherwise OOB.
                        if i == 0 {
                            oracle.push(x);
                            let nv = rt::assoc(v, Value::int(0), Value::int(x));
                            drop_value(v);
                            v = nv;
                        } else {
                            // skip — exception path
                        }
                    } else {
                        let i = i % (oracle.len() + 1);
                        if i == oracle.len() {
                            oracle.push(x);
                        } else {
                            oracle[i] = x;
                        }
                        let nv = rt::assoc(v, Value::int(i as i64), Value::int(x));
                        prop_assert!(!nv.is_exception(), "assoc i={i} len={}", oracle.len() - 1);
                        drop_value(v);
                        v = nv;
                    }
                }
            }
            // Cross-checks after every step.
            prop_assert_eq!(rt::count(v).as_int().unwrap(), oracle.len() as i64);
            prop_assert_eq!(vec_to_native(v), oracle.clone());
            prop_assert_eq!(materialize(v), oracle.clone());
        }
        drop_value(v);
    }
}
