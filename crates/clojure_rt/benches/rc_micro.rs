//! Bench: gc::share op cost (CAS biased->shared).

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use clojure_rt::{init, register_type, share, Value};

register_type! { pub struct Esc { _p: Value } }

fn bench_share(c: &mut Criterion) {
    init();
    c.bench_function("rc_share_op", |b| {
        b.iter(|| {
            let v = Esc::alloc(Value::NIL);
            share(v);
            black_box(v);
            clojure_rt::drop_value(v);
        });
    });
}

criterion_group!(benches, bench_share);
criterion_main!(benches);
