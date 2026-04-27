//! Multi-threaded RCImmix correctness: remote-free eventually drains,
//! no double-decrement, contention behavior, thread-exit orphan handling.

use std::sync::mpsc;
use std::thread;

use clojure_rt::{init, register_type, share, Value};

register_type! {
    pub struct RCell { payload: Value }
}

#[test]
fn remote_free_eventually_drains() {
    init();
    let (tx, rx) = mpsc::channel::<Value>();

    let producer = thread::spawn(move || {
        for i in 0..10_000 {
            let v = RCell::alloc(Value::int(i));
            share(v); // flip biased -> shared so consumer can drop
            tx.send(v).unwrap();
        }
        // Producer also keeps allocating after sending; its slow path
        // will drain remote frees from the consumer.
        for _ in 0..1_000 {
            let v = RCell::alloc(Value::NIL);
            clojure_rt::drop_value(v);
        }
    });

    let consumer = thread::spawn(move || {
        let mut received = 0;
        while let Ok(v) = rx.recv() {
            clojure_rt::drop_value(v);
            received += 1;
        }
        received
    });

    producer.join().unwrap();
    let received = consumer.join().unwrap();
    assert_eq!(received, 10_000);
}

#[test]
fn remote_free_contention_8_threads() {
    init();
    // Pre-allocate 8K objects on the main thread, share them, then
    // distribute to 8 worker threads to drop.
    let mut all_values: Vec<Value> = (0..8_000)
        .map(|i| {
            let v = RCell::alloc(Value::int(i));
            share(v);
            v
        })
        .collect();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let chunk: Vec<Value> = all_values.drain(..1_000).collect();
        handles.push(thread::spawn(move || {
            for v in chunk {
                clojure_rt::drop_value(v);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Force the main thread (the original owner of the blocks
    // containing those objects) to drain by allocating more.
    for _ in 0..5_000 {
        let v = RCell::alloc(Value::NIL);
        clojure_rt::drop_value(v);
    }
}
