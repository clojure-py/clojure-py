//! Cross-check: random op sequences applied through transients
//! should produce the same result as the same sequence applied
//! through the persistent API.

use proptest::prelude::*;

use clojure_rt::{drop_value, init, rt, Value};

#[derive(Debug, Clone)]
enum VOp {
    Conj(i32),
    Pop,
    Assoc(usize, i32),
}

fn vop_strategy() -> impl Strategy<Value = VOp> {
    prop_oneof![
        any::<i32>().prop_map(VOp::Conj),
        Just(VOp::Pop),
        (any::<u16>(), any::<i32>()).prop_map(|(i, v)| VOp::Assoc(i as usize, v)),
    ]
}

#[derive(Debug, Clone)]
enum MOp {
    Assoc(i32, i32),
    Dissoc(i32),
}

fn mop_strategy() -> impl Strategy<Value = MOp> {
    prop_oneof![
        (0..16i32, any::<i32>()).prop_map(|(k, v)| MOp::Assoc(k, v)),
        (0..16i32).prop_map(MOp::Dissoc),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Vectors: persistent op-by-op vs transient-then-persistent.
    /// Both should produce equiv results.
    #[test]
    fn vector_transient_matches_persistent(
        ops in proptest::collection::vec(vop_strategy(), 0..64)
    ) {
        init();

        // Persistent path.
        let mut p: Value = rt::vector(&[]);
        let mut p_count: i64 = 0;
        for op in &ops {
            match *op {
                VOp::Conj(x) => {
                    let np = rt::conj(p, Value::int(x as i64));
                    drop_value(p);
                    p = np;
                    p_count += 1;
                }
                VOp::Pop => {
                    if p_count > 0 {
                        let np = rt::pop(p);
                        prop_assert!(!np.is_exception());
                        drop_value(p);
                        p = np;
                        p_count -= 1;
                    }
                }
                VOp::Assoc(i, x) => {
                    if p_count > 0 {
                        let i = (i as i64) % p_count;
                        let np = rt::assoc(p, Value::int(i), Value::int(x as i64));
                        drop_value(p);
                        p = np;
                    }
                }
            }
        }

        // Transient path.
        let mut t: Value = rt::transient(rt::vector(&[]));
        let mut t_count: i64 = 0;
        for op in &ops {
            match *op {
                VOp::Conj(x) => {
                    let nt = rt::conj_bang(t, Value::int(x as i64));
                    drop_value(t);
                    t = nt;
                    t_count += 1;
                }
                VOp::Pop => {
                    if t_count > 0 {
                        let nt = rt::pop_bang(t);
                        prop_assert!(!nt.is_exception());
                        drop_value(t);
                        t = nt;
                        t_count -= 1;
                    }
                }
                VOp::Assoc(i, x) => {
                    if t_count > 0 {
                        let i = (i as i64) % t_count;
                        let nt = rt::assoc_bang(t, Value::int(i), Value::int(x as i64));
                        drop_value(t);
                        t = nt;
                    }
                }
            }
        }
        let frozen = rt::persistent_(t);
        drop_value(t);

        prop_assert!(rt::equiv(p, frozen).as_bool().unwrap_or(false),
                     "transient batch result differs from persistent op-by-op");
        drop_value(p);
        drop_value(frozen);
    }

    /// ArrayMaps: same cross-check.
    #[test]
    fn array_map_transient_matches_persistent(
        ops in proptest::collection::vec(mop_strategy(), 0..64)
    ) {
        init();

        // Persistent path.
        let mut p: Value = rt::array_map(&[]);
        for op in &ops {
            match *op {
                MOp::Assoc(k, v) => {
                    let np = rt::assoc(p, Value::int(k as i64), Value::int(v as i64));
                    drop_value(p);
                    p = np;
                }
                MOp::Dissoc(k) => {
                    let np = rt::dissoc(p, Value::int(k as i64));
                    drop_value(p);
                    p = np;
                }
            }
        }

        // Transient path.
        let mut t: Value = rt::transient(rt::array_map(&[]));
        for op in &ops {
            match *op {
                MOp::Assoc(k, v) => {
                    let nt = rt::assoc_bang(
                        t, Value::int(k as i64), Value::int(v as i64),
                    );
                    drop_value(t);
                    t = nt;
                }
                MOp::Dissoc(k) => {
                    let nt = rt::dissoc_bang(t, Value::int(k as i64));
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
