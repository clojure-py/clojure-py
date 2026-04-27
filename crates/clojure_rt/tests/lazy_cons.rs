//! Cons + Thunk lazy-cons fixture. Each Cons holds (head, tail) where
//! tail is initially a Thunk. Forcing the thunk produces the next Cons.
//! Walking the seq drops the prior cell so steady-state has 1 live cell.

use clojure_rt::{init, register_type, Value};
use clojure_rt::gc::naive::NAIVE;

register_type! {
    pub struct Cons {
        head: Value,
        tail: Value,
    }
}

register_type! {
    pub struct Thunk {
        // Encoded "next int generator": next head = Value::int(payload as i64),
        // new tail = Thunk{payload + 1}. Real closures come later.
        payload: Value,
    }
}

/// Force a thunk-tailed Cons: read head, read tail (which is a Thunk),
/// allocate the next Cons, drop the old Cons, return new Cons.
unsafe fn step(cons_v: Value) -> Value {
    unsafe {
        let h = cons_v.as_heap().unwrap() as *mut clojure_rt::header::Header;
        let body = h.add(1) as *const Cons;
        let head = (*body).head;
        let thunk_v = (*body).tail;

        // Compute next head from thunk payload.
        let th_h = thunk_v.as_heap().unwrap() as *mut clojure_rt::header::Header;
        let th_body = th_h.add(1) as *const Thunk;
        let next_int = (*th_body).payload.as_int().unwrap();

        let next_thunk = Thunk::alloc(Value::int(next_int + 1));
        let next_cons = Cons::alloc(Value::int(next_int), next_thunk);

        // Drop the old cons (which drops its fields including the old thunk).
        clojure_rt::drop_value(cons_v);
        let _ = head;
        next_cons
    }
}

#[test]
fn walk_million_cells_no_leak() {
    init();
    let before = NAIVE.live_count();

    let initial_thunk = Thunk::alloc(Value::int(0));
    let mut cur = Cons::alloc(Value::int(-1), initial_thunk);

    let n = 1_000_000;
    for _ in 0..n {
        cur = unsafe { step(cur) };
    }

    clojure_rt::drop_value(cur);
    assert_eq!(NAIVE.live_count(), before, "leak detected after walking {n} cells");
}
