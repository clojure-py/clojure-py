//! Multi-threaded RCImmix correctness: remote-free eventually drains,
//! no double-decrement, contention behavior, thread-exit orphan handling.

use std::sync::mpsc;
use std::thread;

use clojure_rt::{init, register_type, share, Value};

register_type! {
    pub struct RCell { tag: Value }
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
