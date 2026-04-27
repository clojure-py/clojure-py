//! Benches: lazy-cons walk under biased RC; same workload with every
//! cell escaped at alloc; and drop-to-zero throughput.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};

use clojure_rt::{init, register_type, share, Value};

register_type! {
    pub struct BCons { head: Value, tail: Value }
}
register_type! {
    pub struct BThunk { payload: Value }
}

unsafe fn step_biased(cur: Value) -> Value {
    unsafe {
        let h = cur.as_heap().unwrap() as *mut clojure_rt::Header;
        let body = h.add(1) as *const BCons;
        let thunk_v = (*body).tail;
        let th_h = thunk_v.as_heap().unwrap() as *mut clojure_rt::Header;
        let th_body = th_h.add(1) as *const BThunk;
        let next_int = (*th_body).payload.as_int().unwrap();
        let next = BCons::alloc(Value::int(next_int), BThunk::alloc(Value::int(next_int + 1)));
        clojure_rt::drop_value(cur);
        next
    }
}

unsafe fn step_escaped(cur: Value) -> Value {
    unsafe {
        let h = cur.as_heap().unwrap() as *mut clojure_rt::Header;
        let body = h.add(1) as *const BCons;
        let thunk_v = (*body).tail;
        let th_h = thunk_v.as_heap().unwrap() as *mut clojure_rt::Header;
        let th_body = th_h.add(1) as *const BThunk;
        let next_int = (*th_body).payload.as_int().unwrap();
        let nt = BThunk::alloc(Value::int(next_int + 1));
        share(nt);
        let next = BCons::alloc(Value::int(next_int), nt);
        share(next);
        clojure_rt::drop_value(cur);
        next
    }
}

fn bench_biased(c: &mut Criterion) {
    init();
    let mut g = c.benchmark_group("lazy_cons_biased");
    g.throughput(Throughput::Elements(1));
    g.bench_function("step", |b| {
        let mut cur = BCons::alloc(Value::int(0), BThunk::alloc(Value::int(1)));
        b.iter(|| {
            cur = unsafe { step_biased(cur) };
        });
        clojure_rt::drop_value(cur);
    });
    g.finish();
}

fn bench_escaped(c: &mut Criterion) {
    init();
    let mut g = c.benchmark_group("lazy_cons_escaped");
    g.throughput(Throughput::Elements(1));
    g.bench_function("step", |b| {
        let init_thunk = BThunk::alloc(Value::int(1));
        share(init_thunk);
        let init_cons = BCons::alloc(Value::int(0), init_thunk);
        share(init_cons);
        let mut cur = init_cons;
        b.iter(|| {
            cur = unsafe { step_escaped(cur) };
        });
        clojure_rt::drop_value(cur);
    });
    g.finish();
}

fn bench_drop_to_zero(c: &mut Criterion) {
    init();
    let mut g = c.benchmark_group("drop_to_zero");
    g.bench_function("alloc_then_drop", |b| {
        b.iter(|| {
            let v = BCons::alloc(Value::NIL, Value::NIL);
            clojure_rt::drop_value(v);
        });
    });
    g.finish();
}

criterion_group!(benches, bench_biased, bench_escaped, bench_drop_to_zero);
criterion_main!(benches);
