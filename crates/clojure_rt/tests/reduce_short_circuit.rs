//! Short-circuit behavior of `reduce`. We need a step function that
//! actually returns a `Reduced` Value mid-walk; Python lambdas can't
//! construct `Reduced` directly, so we use a tiny test-fixture
//! type whose `IFn::invoke_3` implementation hard-codes the
//! short-circuit predicate.

use clojure_rt::{drop_value, implements, init, register_type, rt, Value};
use clojure_rt::protocols::ifn::IFn;

register_type! {
    pub struct WrapAtThreeFn {
        _placeholder: Value,
    }
}

// Step fn: `acc + x`, but wraps in `reduced(...)` once the running
// sum reaches 3. Useful for verifying that reduce stops walking and
// yields the unwrapped value.
implements! {
    impl IFn for WrapAtThreeFn {
        fn invoke_3(this: Value, acc: Value, x: Value) -> Value {
            let _ = this;
            let acc_n = acc.as_int().expect("acc int");
            let x_n   = x.as_int().expect("x int");
            let sum = Value::int(acc_n + x_n);
            if acc_n + x_n >= 3 {
                rt::reduced(sum)
            } else {
                sum
            }
        }
    }
}

#[test]
fn reduce_init_short_circuits_on_reduced_acc() {
    init();
    let f = WrapAtThreeFn::alloc(Value::NIL);
    // (((0 + 1) + 2) → reduced(3); the rest of the vector is skipped.
    let v = rt::vector(&[Value::int(1), Value::int(2), Value::int(99), Value::int(99)]);
    let r = rt::reduce_init(v, f, Value::int(0));
    assert_eq!(r.as_int(), Some(3), "reduce did not short-circuit on Reduced");
    drop_value(r);
    drop_value(v);
    drop_value(f);
}

#[test]
fn reduce_no_init_short_circuits_on_reduced_acc() {
    init();
    let f = WrapAtThreeFn::alloc(Value::NIL);
    // Seed = first elem (1). (1 + 2) → reduced(3); rest skipped.
    let v = rt::vector(&[Value::int(1), Value::int(2), Value::int(99), Value::int(99)]);
    let r = rt::reduce(v, f);
    assert_eq!(r.as_int(), Some(3));
    drop_value(r);
    drop_value(v);
    drop_value(f);
}

#[test]
fn reduce_walks_to_completion_when_no_reduced() {
    init();
    let f = WrapAtThreeFn::alloc(Value::NIL);
    // Total stays under threshold: (((0 + (-1)) + 1) + 1) = 1.
    let v = rt::vector(&[Value::int(-1), Value::int(1), Value::int(1)]);
    let r = rt::reduce_init(v, f, Value::int(0));
    assert_eq!(r.as_int(), Some(1));
    drop_value(r);
    drop_value(v);
    drop_value(f);
}
