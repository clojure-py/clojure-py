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

/// Helper IFn bodies. Pure fns are stateless `extern "C"` slots; the
/// test fns that need closure-style state (`constantly`,
/// `watch_recorder`) read from a thread-local set up by the test
/// before the call.
mod fns {
    use clojure_rt::value::Value;
    use std::cell::Cell;

    thread_local! {
        /// Last `(old, new)` int pair observed by `watch_recorder_5`.
        /// Reset to `None` by tests before installing the watch.
        pub static LAST_WATCH: Cell<Option<(i64, i64)>> = const { Cell::new(None) };

        /// Return value for `constantly_invoke_2`. The Value bytes
        /// are stored uninterpreted; the caller is responsible for
        /// keeping the underlying heap object alive while the fn is
        /// in scope.
        pub static CONSTANT_RETURN: Cell<Value> = const { Cell::new(Value::NIL) };
    }

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

    /// `(fn [x] (even? x))`.
    pub unsafe extern "C" fn even_q_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        if x.as_int().unwrap_or(0) % 2 == 0 { Value::TRUE } else { Value::FALSE }
    }

    /// `(fn [x] (pos? x))`.
    pub unsafe extern "C" fn positive_q_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        if x.as_int().unwrap_or(0) > 0 { Value::TRUE } else { Value::FALSE }
    }

    /// `(fn [x] (neg? x))`.
    pub unsafe extern "C" fn negative_q_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        if x.as_int().unwrap_or(0) < 0 { Value::TRUE } else { Value::FALSE }
    }

    /// `(fn [_] CONSTANT_RETURN)` — returns whatever the test stashed
    /// in the `CONSTANT_RETURN` thread-local, dup'd for the caller.
    pub unsafe extern "C" fn constantly_invoke_2(_args: *const Value, _n: usize) -> Value {
        let v = CONSTANT_RETURN.with(|c| c.get());
        clojure_rt::rc::dup(v);
        v
    }

    /// `(fn [k r old new])` — records `(old.as_int(), new.as_int())`
    /// into `LAST_WATCH` and returns nil. Watch arity is 5 (receiver
    /// + 4 args), so the slot is `invoke_5`.
    pub unsafe extern "C" fn watch_recorder_invoke_5(args: *const Value, _n: usize) -> Value {
        let old = unsafe { *args.add(3) };
        let new_v = unsafe { *args.add(4) };
        LAST_WATCH.with(|c| {
            c.set(Some((
                old.as_int().unwrap_or(0),
                new_v.as_int().unwrap_or(0),
            )));
        });
        Value::NIL
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
        // Zero-byte body — the test fn carries no per-instance state.
        let layout = Layout::from_size_align(0, 1).unwrap();
        type_registry::register_dynamic_type(name, layout, destruct)
    }

    /// Returns a `Value` whose tag is a fresh dynamic TypeId, bound
    /// to `fn_ptr` as the implementation of `invoke_method`. Backed
    /// by a real heap allocation (zero-byte body) so `rc::share` /
    /// `dup` / `drop` work the same as on any other heap Value —
    /// matters when the synthetic fn is published into an atom's
    /// validator/watches slot, which forces a `share` call.
    pub fn make(invoke_method: &clojure_rt::protocol::ProtocolMethod, fn_ptr: MethodFn) -> Value {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| { clojure_rt::init(); });
        let tid = fresh_tid();
        extend_type(tid, invoke_method, fn_ptr);
        let layout = Layout::from_size_align(0, 1).unwrap();
        unsafe {
            let h = clojure_rt::gc::rcimmix::RCIMMIX.alloc_inline(layout, tid);
            Value::from_heap(h)
        }
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
    pub fn even_q() -> Value {
        make(&IFn::INVOKE_2, super::fns::even_q_invoke_2)
    }
    pub fn positive_q() -> Value {
        make(&IFn::INVOKE_2, super::fns::positive_q_invoke_2)
    }
    pub fn negative_q() -> Value {
        make(&IFn::INVOKE_2, super::fns::negative_q_invoke_2)
    }
    /// Caller pre-stashes the desired return value into the
    /// `CONSTANT_RETURN` thread-local; this fn pulls it back out.
    pub fn constantly() -> Value {
        make(&IFn::INVOKE_2, super::fns::constantly_invoke_2)
    }
    /// Watch fn — its receiver-plus-four-arg arity is 5.
    pub fn watch_recorder() -> Value {
        make(&IFn::INVOKE_5, super::fns::watch_recorder_invoke_5)
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

// --- Mutable meta ---------------------------------------------------------

#[test]
fn meta_starts_nil_and_reset_meta_installs_in_place() {
    init();
    let a = rt::atom(Value::int(0));
    assert!(rt::meta(a).is_nil());
    let m = rt::array_map(&[rt::keyword(None, "tag"), Value::int(7)]);
    let returned = rt::reset_meta_bang(a, m);
    // reset-meta! returns the new meta.
    assert!(rt::equiv(returned, m).as_bool().unwrap_or(false));
    // (meta a) now reflects the install — no new atom created.
    let read = rt::meta(a);
    assert!(rt::equiv(read, m).as_bool().unwrap_or(false));
    drop_all(&[returned, read, m, a]);
}

#[test]
fn alter_meta_arity_2_applies_fn_to_current() {
    // (alter-meta! a (constantly {:k 1})) — the no-extra-args path.
    init();
    let a = rt::atom(Value::int(0));
    let new_m = rt::array_map(&[rt::keyword(None, "k"), Value::int(1)]);
    fns::CONSTANT_RETURN.with(|c| c.set(new_m));
    let constantly = test_fn_type::constantly();
    let r = rt::alter_meta_bang(a, constantly, &[]);
    assert!(rt::equiv(r, new_m).as_bool().unwrap_or(false));
    let read = rt::meta(a);
    assert!(rt::equiv(read, new_m).as_bool().unwrap_or(false));
    drop_all(&[r, read, new_m, a]);
}

// --- Validators -----------------------------------------------------------

#[test]
fn set_validator_rejects_install_when_current_value_fails() {
    init();
    let a = rt::atom(Value::int(0));
    // Validator: even? — current value 0 is even, install OK.
    let even_q = test_fn_type::even_q();
    let _ = rt::set_validator_bang(a, even_q);
    // Now make the current value odd via reset (skipping validator
    // briefly is impossible; reset! goes through validator). Set
    // first, then change the value through the validator.
    let r1 = rt::reset_bang(a, Value::int(2));
    assert_eq!(r1.as_int(), Some(2));
    drop_value(r1);
    // Try to install a validator that the current value (2) would
    // pass — should succeed.
    let positive = test_fn_type::positive_q();
    let r = rt::set_validator_bang(a, positive);
    assert!(r.is_nil());
    // Now install one that the current value would FAIL.
    let neg_q = test_fn_type::negative_q();
    let r = rt::set_validator_bang(a, neg_q);
    assert!(r.is_exception(), "set-validator! should reject");
    drop_value(r);
    drop_value(a);
}

#[test]
fn validator_rejects_reset_when_predicate_fails() {
    init();
    let a = rt::atom(Value::int(2));
    let even_q = test_fn_type::even_q();
    let _ = rt::set_validator_bang(a, even_q);
    // Try to reset to an odd value — should error, value unchanged.
    let r = rt::reset_bang(a, Value::int(3));
    assert!(r.is_exception(), "validator should reject odd reset");
    drop_value(r);
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(2));
    drop_all(&[v, a]);
}

#[test]
fn validator_rejects_swap_without_advancing_value() {
    init();
    let a = rt::atom(Value::int(2));
    let even_q = test_fn_type::even_q();
    let _ = rt::set_validator_bang(a, even_q);
    // swap! by inc — current 2 → 3 fails the validator.
    let inc = test_fn_type::inc();
    let r = rt::swap_bang(a, inc, &[]);
    assert!(r.is_exception(), "validator should reject odd swap result");
    drop_value(r);
    let v = rt::deref(a);
    assert_eq!(v.as_int(), Some(2));
    drop_all(&[v, a]);
}

#[test]
fn get_validator_returns_installed_or_nil() {
    init();
    let a = rt::atom(Value::int(0));
    assert!(rt::get_validator(a).is_nil());
    let even_q = test_fn_type::even_q();
    let _ = rt::set_validator_bang(a, even_q);
    let v = rt::get_validator(a);
    assert!(!v.is_nil());
    drop_value(v);
    // Clear it.
    let _ = rt::set_validator_bang(a, Value::NIL);
    assert!(rt::get_validator(a).is_nil());
    drop_value(a);
}

// --- Watches --------------------------------------------------------------

#[test]
fn add_watch_fires_on_reset_with_old_and_new() {
    init();
    let a = rt::atom(Value::int(10));
    let key = rt::keyword(None, "w1");
    fns::LAST_WATCH.with(|c| c.set(None));
    let watch_fn = test_fn_type::watch_recorder();
    let _ = rt::add_watch(a, key, watch_fn);
    let r = rt::reset_bang(a, Value::int(20));
    drop_value(r);
    let pair = fns::LAST_WATCH.with(|c| c.get());
    assert_eq!(pair, Some((10, 20)));
    drop_all(&[key, a]);
}

#[test]
fn remove_watch_stops_firing() {
    init();
    let a = rt::atom(Value::int(0));
    let key = rt::keyword(None, "w1");
    fns::LAST_WATCH.with(|c| c.set(None));
    let watch_fn = test_fn_type::watch_recorder();
    let _ = rt::add_watch(a, key, watch_fn);
    let _ = rt::reset_bang(a, Value::int(1));
    let _ = rt::remove_watch(a, key);
    let _ = rt::reset_bang(a, Value::int(99));
    // Last-fired pair must reflect the pre-removal transition only.
    let pair = fns::LAST_WATCH.with(|c| c.get());
    assert_eq!(pair, Some((0, 1)));
    drop_all(&[key, a]);
}

#[test]
fn watches_fire_on_swap_and_compare_and_set() {
    init();
    let a = rt::atom(Value::int(0));
    let key = rt::keyword(None, "w1");
    fns::LAST_WATCH.with(|c| c.set(None));
    let watch_fn = test_fn_type::watch_recorder();
    let _ = rt::add_watch(a, key, watch_fn);
    // swap! +5 → from 0 to 5.
    let f = test_fn_type::add();
    let _ = rt::swap_bang(a, f, &[Value::int(5)]);
    assert_eq!(fns::LAST_WATCH.with(|c| c.get()), Some((0, 5)));
    // compare-and-set! 5 → 7.
    let _ = rt::compare_and_set(a, Value::int(5), Value::int(7));
    assert_eq!(fns::LAST_WATCH.with(|c| c.get()), Some((5, 7)));
    drop_all(&[key, a]);
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
