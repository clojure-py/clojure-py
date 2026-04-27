//! Bench: remote_free worst-case. Producer allocates+shares; consumer
//! drops via channel. Measure producer-side alloc latency under
//! concurrent remote frees.

use std::sync::mpsc;
use std::thread;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use clojure_rt::{init, register_type, share, Value};

register_type! {
    pub struct BCell { _p: Value }
}

fn bench_remote_free_worst_case(c: &mut Criterion) {
    init();
    c.bench_function("remote_free_producer_alloc", |b| {
        let (tx, rx) = mpsc::channel::<Value>();
        let consumer = thread::spawn(move || {
            while let Ok(v) = rx.recv() {
                clojure_rt::drop_value(v);
            }
        });

        b.iter(|| {
            let v = BCell::alloc(Value::NIL);
            share(v);
            tx.send(v).ok();
            black_box(())
        });

        drop(tx);
        consumer.join().unwrap();
    });
}

criterion_group!(benches, bench_remote_free_worst_case);
criterion_main!(benches);
