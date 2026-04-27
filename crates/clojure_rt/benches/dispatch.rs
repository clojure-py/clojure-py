//! Benches: dispatch tier 1 (monomorphic IC hits), tier 2 cold fill,
//! and megamorphic (alternating types).

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use clojure_rt::{init, register_type, protocol, implements, dispatch, Value};

protocol! {
    pub trait DGreeter {
        fn greet(this: Value) -> Value;
    }
}

register_type! { pub struct DA { _p: Value } }
register_type! { pub struct DB { _p: Value } }
register_type! { pub struct DC { _p: Value } }
register_type! { pub struct DD { _p: Value } }

implements! { impl DGreeter for DA { fn greet(this: Value) -> Value { let _ = this; Value::int(1) } } }
implements! { impl DGreeter for DB { fn greet(this: Value) -> Value { let _ = this; Value::int(2) } } }
implements! { impl DGreeter for DC { fn greet(this: Value) -> Value { let _ = this; Value::int(3) } } }
implements! { impl DGreeter for DD { fn greet(this: Value) -> Value { let _ = this; Value::int(4) } } }

fn bench_tier1(c: &mut Criterion) {
    init();
    let v = DA::alloc(Value::NIL);
    // Warm the IC.
    let _ = dispatch!(DGreeter::greet, &[v]);
    c.bench_function("dispatch_tier1_hit", |b| {
        b.iter(|| {
            let r = dispatch!(DGreeter::greet, &[v]);
            black_box(r);
        });
    });
    clojure_rt::drop_value(v);
}

fn bench_megamorphic(c: &mut Criterion) {
    init();
    let vs = [
        DA::alloc(Value::NIL),
        DB::alloc(Value::NIL),
        DC::alloc(Value::NIL),
        DD::alloc(Value::NIL),
    ];
    c.bench_function("dispatch_megamorphic_4way", |b| {
        let mut i = 0usize;
        b.iter(|| {
            let v = vs[i & 3];
            i = i.wrapping_add(1);
            let r = dispatch!(DGreeter::greet, &[v]);
            black_box(r);
        });
    });
    for v in vs { clojure_rt::drop_value(v); }
}

criterion_group!(benches, bench_tier1, bench_megamorphic);
criterion_main!(benches);
