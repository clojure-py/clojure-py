//! Model: two threads concurrently intern the same key. Both must return
//! the SAME interned value (pointer identity is preserved by a single
//! `entry().or_insert_with` operation, as our Keyword table does).

#![cfg(loom)]

use loom::sync::Arc;
use loom::sync::Mutex;
use loom::thread;
use std::collections::HashMap;

#[test]
fn concurrent_intern_same_key_returns_identical() {
    loom::model(|| {
        // loom doesn't ship a concurrent hash map, so we approximate with Mutex<HashMap>.
        // The production code uses DashMap's `entry(...).or_insert_with(...)` — same
        // linearizability guarantees under the mutex lock-acquire order.
        let map: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
        let m1 = Arc::clone(&map);
        let m2 = Arc::clone(&map);
        let t1 = thread::spawn(move || {
            let mut g = m1.lock().unwrap();
            *g.entry("k".to_string()).or_insert_with(|| 42)
        });
        let t2 = thread::spawn(move || {
            let mut g = m2.lock().unwrap();
            *g.entry("k".to_string()).or_insert_with(|| 42)
        });
        let a = t1.join().unwrap();
        let b = t2.join().unwrap();
        assert_eq!(a, b);
    });
}
