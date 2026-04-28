//! `Atom` direct tests — deref, reset!, swap! at multiple arities,
//! compare-and-set!, with-meta, plus a multi-thread stress.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::protocols::deref::IDeref;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn atom_deref_round_trip() {
    init();
    let a = rt::atom(Value::int(7));
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(7));
    drop_all(&[v, a]);
}

#[test]
fn reset_replaces_value() {
    init();
    let a = rt::atom(Value::int(1));
    let r = rt::reset_bang(a, Value::int(42));
    assert_eq!(r.as_int(), Some(42));
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(42));
    drop_all(&[r, v, a]);
}

#[test]
fn compare_and_set_succeeds_on_match() {
    init();
    let a = rt::atom(Value::int(10));
    let ok = rt::compare_and_set(a, Value::int(10), Value::int(11));
    assert_eq!(ok.as_bool(), Some(true));
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(11));
    drop_all(&[v, a]);
}

#[test]
fn compare_and_set_fails_on_mismatch() {
    init();
    let a = rt::atom(Value::int(10));
    let ok = rt::compare_and_set(a, Value::int(99), Value::int(11));
    assert_eq!(ok.as_bool(), Some(false));
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(10));
    drop_all(&[v, a]);
}

// --- swap! ----------------------------------------------------------------

/// Helper IFn: identity on the first arg. Used to test swap! arity 2
/// (no extra args) without dragging in a heavyweight test fixture.
mod fns {
    use clojure_rt::value::Value;

    /// `(fn [x] x)` — pure copy-out for swap_2 sanity.
    pub unsafe extern "C" fn id_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        clojure_rt::rc::dup(x);
        x
    }

    /// `(fn [x] (inc x))` — increments the int, otherwise no-op.
    pub unsafe extern "C" fn inc_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        Value::int(x.as_int().unwrap_or(0) + 1)
    }

    /// `(fn [x y] (+ x y))` — adds the two int args.
    pub unsafe extern "C" fn add_invoke_3(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        let y = unsafe { *args.add(2) };
        Value::int(x.as_int().unwrap_or(0) + y.as_int().unwrap_or(0))
    }

    /// `(fn [x y z] (+ x y z))`.
    pub unsafe extern "C" fn add3_invoke_4(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        let y = unsafe { *args.add(2) };
        let z = unsafe { *args.add(3) };
        Value::int(
            x.as_int().unwrap_or(0)
                + y.as_int().unwrap_or(0)
                + z.as_int().unwrap_or(0),
        )
    }
}

/// Synthetic foreign type tagged with one of the test fns above.
/// Mirrors the pattern used elsewhere in `tests/synthetic_fixtures.rs`
/// when we need to inject a callable Value without standing up a
/// full IFn-bearing host type.
mod test_fn_type {
    use clojure_rt::dispatch::MethodFn;
    use clojure_rt::protocol::extend_type;
    use clojure_rt::protocols::ifn::IFn;
    use clojure_rt::type_registry;
    use clojure_rt::value::{TypeId, Value};
    use core::alloc::Layout;
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_tid() -> TypeId {
        // Each call gets a new dynamic TypeId so installing a
        // different impl doesn't collide.
        let id_seed = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name: &'static str = Box::leak(format!("test_fn_{id_seed}").into_boxed_str());
        unsafe fn destruct(_h: *mut clojure_rt::Header) {}
        type_registry::register_dynamic_type(name, Layout::new::<()>(), destruct)
    }

    /// Returns a `Value` whose tag is a fresh dynamic TypeId, bound
    /// to `invoke_fn` as the implementation of `IFn::invoke_<arity>`.
    /// The returned Value carries no payload (foreign-tag, no heap).
    pub fn make(invoke_method: &clojure_rt::protocol::ProtocolMethod, fn_ptr: MethodFn) -> Value {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| { clojure_rt::init(); });
        let tid = fresh_tid();
        extend_type(tid, invoke_method, fn_ptr);
        // Construct a non-heap Value with this tag. We use the
        // pseudo-foreign shape: tag = tid, payload = 0.
        Value { tag: tid, _pad: 0, payload: 0 }
    }

    pub fn id() -> Value {
        make(&IFn::INVOKE_2, super::fns::id_invoke_2)
    }
    pub fn inc() -> Value {
        make(&IFn::INVOKE_2, super::fns::inc_invoke_2)
    }
    pub fn add() -> Value {
        make(&IFn::INVOKE_3, super::fns::add_invoke_3)
    }
    pub fn add3() -> Value {
        make(&IFn::INVOKE_4, super::fns::add3_invoke_4)
    }
}

#[test]
fn swap_arity_2_identity() {
    init();
    let a = rt::atom(Value::int(7));
    let f = test_fn_type::id();
    let r = rt::swap_bang(a, f, &[]);
    assert_eq!(r.as_int(), Some(7));
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(7));
    drop_all(&[r, v, a]);
}

#[test]
fn swap_arity_2_inc() {
    init();
    let a = rt::atom(Value::int(0));
    let f = test_fn_type::inc();
    for _ in 0..5 {
        let r = rt::swap_bang(a, f, &[]);
        drop_value(r);
    }
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(5));
    drop_all(&[v, a]);
}

#[test]
fn swap_arity_3_add() {
    init();
    let a = rt::atom(Value::int(10));
    let f = test_fn_type::add();
    let r = rt::swap_bang(a, f, &[Value::int(5)]);
    assert_eq!(r.as_int(), Some(15));
    drop_all(&[r, a]);
}

#[test]
fn swap_arity_4_add3() {
    init();
    let a = rt::atom(Value::int(1));
    let f = test_fn_type::add3();
    let r = rt::swap_bang(a, f, &[Value::int(2), Value::int(3)]);
    assert_eq!(r.as_int(), Some(6));
    drop_all(&[r, a]);
}

#[test]
fn deref_via_protocol() {
    init();
    let a = rt::atom(Value::int(99));
    // Confirm the IDeref protocol path is the same as rt::deref.
    let v = clojure_rt_macros::dispatch!(IDeref::deref, &[a]);
    assert_eq!(v.as_int(), Some(99));
    drop_all(&[v, a]);
}

#[test]
fn with_meta_returns_distinct_atom_with_same_value() {
    init();
    let a = rt::atom(Value::int(123));
    let meta = rt::array_map(&[rt::keyword(None, "tag"), Value::int(7)]);
    let a2 = rt::with_meta(a, meta);
    // Same value snapshot.
    let v = rt::deref(a2);
    assert_eq!(v.as_int(), Some(123));
    // Mutating one doesn't affect the other.
    let _ = rt::reset_bang(a2, Value::int(0));
    let v_orig = rt::deref(a);
    assert_eq!(v_orig.as_int(), Some(123));
    drop_all(&[v, v_orig, a, a2, meta]);
}

// --- Concurrent stress ----------------------------------------------------

#[test]
fn swap_concurrent_increments_are_lossless() {
    init();
    const THREADS: usize = 8;
    const ITERS: usize = 5_000;
    let a = rt::atom(Value::int(0));
    let f = test_fn_type::inc();
    // Atom Value is `Copy`; we can move it into closures freely.
    // The Atom heap object is share-mode (set up in Atom::new), so
    // dup/drop from non-owner threads is safe.
    let counter_witness = Arc::new(AtomicI64::new(0));
    let mut handles = Vec::with_capacity(THREADS);
    for _ in 0..THREADS {
        let cw = Arc::clone(&counter_witness);
        handles.push(thread::spawn(move || {
            for _ in 0..ITERS {
                let r = rt::swap_bang(a, f, &[]);
                drop_value(r);
                cw.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in handles { h.join().unwrap(); }
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some((THREADS * ITERS) as i64));
    assert_eq!(
        counter_witness.load(Ordering::Relaxed) as usize,
        THREADS * ITERS,
        "control: thread loop ran the expected number of times",
    );
    drop_all(&[v, a]);
}
