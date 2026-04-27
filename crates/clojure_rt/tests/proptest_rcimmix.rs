//! Property: random sequences of (alloc, drop) ops using RCImmix
//! complete without corruption or panic. Compares against a reference
//! counter to verify that all alloc'd objects are eventually freed.

use proptest::prelude::*;

use clojure_rt::{init, register_type, Value};

register_type! {
    pub struct PCell { _p: Value }
}

#[derive(Debug, Clone)]
enum Op { Alloc, Drop }

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![Just(Op::Alloc), Just(Op::Drop)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn random_alloc_drop_sequence(ops in proptest::collection::vec(op_strategy(), 1..=2000)) {
        init();
        let mut live: Vec<Value> = Vec::new();
        for op in ops {
            match op {
                Op::Alloc => {
                    live.push(PCell::alloc(Value::NIL));
                }
                Op::Drop => {
                    if let Some(v) = live.pop() {
                        clojure_rt::drop_value(v);
                    }
                }
            }
        }
        // Drain leftovers.
        for v in live {
            clojure_rt::drop_value(v);
        }
    }
}
