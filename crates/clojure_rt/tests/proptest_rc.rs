//! Property: sequences of dup/drop on a single object end up at the
//! same logical count as a reference counter, and drop-to-zero is
//! reached iff the reference reaches zero.

use proptest::prelude::*;

use clojure_rt::header::Header;
use core::sync::atomic::{AtomicI32, Ordering};

#[derive(Debug, Clone)]
enum Op { Dup, Drop }

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![Just(Op::Dup), Just(Op::Drop)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn random_sequence_matches_reference(ops in proptest::collection::vec(op_strategy(), 1..200)) {
        let h = Header {
            type_id: 16, flags: 0,
            rc: AtomicI32::new(Header::INITIAL_RC),
            owner_tid: clojure_rt::gc::rcimmix::tid::current_owner_tid(),
        };
        let mut rc_ref: i32 = 1;            // reference live count
        let mut zeroed = false;
        for op in &ops {
            if zeroed { break; }
            match op {
                Op::Dup => {
                    rc_ref += 1;
                    unsafe { clojure_rt::rc::dup_heap(&h); }
                }
                Op::Drop => {
                    rc_ref -= 1;
                    if unsafe { clojure_rt::rc::drop_heap(&h) } { zeroed = true; }
                    if rc_ref == 0 { prop_assert!(zeroed); }
                    if rc_ref < 0 { break; }
                }
            }
        }
        if !zeroed && rc_ref > 0 {
            // Logical count = -rc (biased) or +rc (shared)
            let r = h.rc.load(Ordering::Relaxed);
            let live = if r < 0 { -r } else { r };
            prop_assert_eq!(live, rc_ref);
        }
    }
}
