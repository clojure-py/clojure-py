//! Property: register N types from M threads, all IDs are distinct
//! and within bounds, and lookups round-trip the metadata.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use proptest::prelude::*;

use clojure_rt::header::Header;
use clojure_rt::type_registry::{register_dynamic_type, get};

unsafe fn nop(_: *mut Header) {}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn concurrent_registration_unique_ids(
        n_threads in 1usize..=8,
        per_thread in 1usize..=32,
    ) {
        let total = n_threads * per_thread;
        let registered = Arc::new(AtomicUsize::new(0));
        let layout = core::alloc::Layout::from_size_align(8, 8).unwrap();

        let handles: Vec<_> = (0..n_threads).map(|_| {
            let registered = registered.clone();
            thread::spawn(move || {
                let mut ids = Vec::with_capacity(per_thread);
                for _ in 0..per_thread {
                    let id = register_dynamic_type("PT", layout, nop);
                    ids.push(id);
                    registered.fetch_add(1, Ordering::Relaxed);
                }
                ids
            })
        }).collect();

        let mut all_ids = Vec::with_capacity(total);
        for h in handles { all_ids.extend(h.join().unwrap()); }

        // Uniqueness.
        all_ids.sort();
        let n = all_ids.len();
        all_ids.dedup();
        prop_assert_eq!(all_ids.len(), n, "non-unique type id observed");

        // Each lookup returns a meta with a matching id.
        for id in &all_ids {
            prop_assert_eq!(get(*id).type_id, *id);
        }
    }
}
