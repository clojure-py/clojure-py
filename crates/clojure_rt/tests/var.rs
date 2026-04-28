//! `Var` direct tests — root read/write, dynamic + thread
//! bindings (push/pop, nested, present-with-nil), alter-var-root,
//! watches, mutable meta, identity equiv/hash, plus the
//! static-global `OnceLock<Value>` pattern.

use std::sync::OnceLock;

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::types::var::Var;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn intern_round_trip_root_read() {
    init();
    let sym = rt::symbol(None, "foo");
    let v = rt::intern_var(Value::NIL, sym, Value::int(42));
    let r = rt::deref(v);
    assert_eq!(r.as_int(), Some(42));
    drop_all(&[r, v, sym]);
}

#[test]
fn ns_and_sym_round_trip() {
    init();
    let sym = rt::symbol(None, "the-name");
    let v = Var::intern(Value::NIL, sym, Value::NIL);
    let read_sym = Var::sym(v);
    assert!(rt::equiv(read_sym, sym).as_bool().unwrap_or(false));
    drop_all(&[read_sym, v, sym]);
}

#[test]
fn bind_root_installs_value() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(1));
    let r = Var::bind_root(v, Value::int(2));
    assert!(r.is_nil());
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(2));
    drop_all(&[read, v, sym]);
}

// --- alter-var-root -------------------------------------------------------

mod fns {
    use clojure_rt::value::Value;
    use std::cell::Cell;

    thread_local! {
        pub static LAST_WATCH: Cell<Option<(i64, i64)>> = const { Cell::new(None) };
    }

    /// `(fn [x] (inc x))`.
    pub unsafe extern "C" fn inc_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        Value::int(x.as_int().unwrap_or(0) + 1)
    }

    /// `(fn [x y] (+ x y))`.
    pub unsafe extern "C" fn add_invoke_3(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        let y = unsafe { *args.add(2) };
        Value::int(x.as_int().unwrap_or(0) + y.as_int().unwrap_or(0))
    }

    /// `(fn [x] (even? x))`.
    pub unsafe extern "C" fn even_q_invoke_2(args: *const Value, _n: usize) -> Value {
        let x = unsafe { *args.add(1) };
        if x.as_int().unwrap_or(0) % 2 == 0 { Value::TRUE } else { Value::FALSE }
    }

    /// `(fn [k r old new])` — record (old, new) into `LAST_WATCH`.
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

mod test_fn_type {
    use clojure_rt::dispatch::MethodFn;
    use clojure_rt::protocol::extend_type;
    use clojure_rt::protocols::ifn::IFn;
    use clojure_rt::type_registry;
    use clojure_rt::value::{TypeId, Value};
    use core::alloc::Layout;
    use core::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_tid() -> TypeId {
        let id_seed = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name: &'static str = Box::leak(format!("var_test_fn_{id_seed}").into_boxed_str());
        unsafe fn destruct(_h: *mut clojure_rt::Header) {}
        let layout = Layout::from_size_align(0, 1).unwrap();
        type_registry::register_dynamic_type(name, layout, destruct)
    }

    pub fn make(invoke_method: &clojure_rt::protocol::ProtocolMethod, fn_ptr: MethodFn) -> Value {
        clojure_rt::init();
        let tid = fresh_tid();
        extend_type(tid, invoke_method, fn_ptr);
        let layout = Layout::from_size_align(0, 1).unwrap();
        unsafe {
            let h = clojure_rt::gc::rcimmix::RCIMMIX.alloc_inline(layout, tid);
            Value::from_heap(h)
        }
    }

    pub fn inc() -> Value { make(&IFn::INVOKE_2, super::fns::inc_invoke_2) }
    pub fn add() -> Value { make(&IFn::INVOKE_3, super::fns::add_invoke_3) }
    pub fn even_q() -> Value { make(&IFn::INVOKE_2, super::fns::even_q_invoke_2) }
    pub fn watch_recorder() -> Value { make(&IFn::INVOKE_5, super::fns::watch_recorder_invoke_5) }
}

#[test]
fn alter_var_root_arity_2() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(0));
    let inc = test_fn_type::inc();
    let r = rt::alter_var_root(v, inc, &[]);
    assert_eq!(r.as_int(), Some(1));
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(1));
    drop_all(&[r, read, v, sym]);
}

#[test]
fn alter_var_root_arity_3_with_extra_arg() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(10));
    let add = test_fn_type::add();
    let r = rt::alter_var_root(v, add, &[Value::int(5)]);
    assert_eq!(r.as_int(), Some(15));
    drop_all(&[r, v, sym]);
}

// --- Validators + watches (parity with Atom) ----------------------------

#[test]
fn validator_rejects_alter_var_root() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(2));
    let even_q = test_fn_type::even_q();
    let _ = rt::set_validator_bang(v, even_q);
    // alter-var-root by inc → 3 (odd) — should be rejected.
    let inc = test_fn_type::inc();
    let r = rt::alter_var_root(v, inc, &[]);
    assert!(r.is_exception());
    drop_value(r);
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(2));
    drop_all(&[read, v, sym]);
}

#[test]
fn watches_fire_on_bind_root() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(0));
    let key = rt::keyword(None, "w");
    fns::LAST_WATCH.with(|c| c.set(None));
    let watch_fn = test_fn_type::watch_recorder();
    let _ = rt::add_watch(v, key, watch_fn);
    let _ = Var::bind_root(v, Value::int(99));
    assert_eq!(fns::LAST_WATCH.with(|c| c.get()), Some((0, 99)));
    drop_all(&[key, v, sym]);
}

// --- Mutable meta -----------------------------------------------------------

#[test]
fn meta_starts_nil_and_reset_meta_installs_in_place() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::NIL);
    assert!(rt::meta(v).is_nil());
    let m = rt::array_map(&[rt::keyword(None, "doc"), Value::int(7)]);
    let r = rt::reset_meta_bang(v, m);
    assert!(rt::equiv(r, m).as_bool().unwrap_or(false));
    drop_all(&[r, m, v, sym]);
}

// --- Dynamic + thread bindings ---------------------------------------------

#[test]
fn non_dynamic_var_ignores_thread_binding() {
    // (binding [v 99] (deref v)) — but v isn't ^:dynamic, so the
    // thread binding shouldn't be honored. JVM matches: setting a
    // thread binding on a non-dynamic var is a no-op for read.
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(1));
    let bindings = rt::array_map(&[v, Value::int(99)]);
    rt::push_thread_bindings(bindings);
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(1), "non-dynamic var sees only root");
    rt::pop_thread_bindings();
    drop_all(&[read, bindings, v, sym]);
}

#[test]
fn dynamic_var_honors_thread_binding() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(1));
    let _ = Var::set_dynamic(v);
    let bindings = rt::array_map(&[v, Value::int(99)]);
    rt::push_thread_bindings(bindings);
    let inside = rt::deref(v);
    assert_eq!(inside.as_int(), Some(99));
    rt::pop_thread_bindings();
    let outside = rt::deref(v);
    assert_eq!(outside.as_int(), Some(1));
    drop_all(&[inside, outside, bindings, v, sym]);
}

#[test]
fn nested_thread_bindings_inner_shadows_outer() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(1));
    let _ = Var::set_dynamic(v);
    let outer = rt::array_map(&[v, Value::int(10)]);
    let inner = rt::array_map(&[v, Value::int(20)]);
    rt::push_thread_bindings(outer);
    let r1 = rt::deref(v);
    assert_eq!(r1.as_int(), Some(10));
    rt::push_thread_bindings(inner);
    let r2 = rt::deref(v);
    assert_eq!(r2.as_int(), Some(20));
    rt::pop_thread_bindings();
    let r3 = rt::deref(v);
    assert_eq!(r3.as_int(), Some(10), "popping inner restores outer");
    rt::pop_thread_bindings();
    let r4 = rt::deref(v);
    assert_eq!(r4.as_int(), Some(1), "popping outer restores root");
    drop_all(&[r1, r2, r3, r4, outer, inner, v, sym]);
}

#[test]
fn unrelated_var_in_outer_frame_visible_in_inner() {
    // push-thread-bindings merges onto the previous frame so a
    // var bound in the outer frame remains visible inside an
    // inner frame that doesn't rebind it.
    init();
    let sym_a = rt::symbol(None, "a");
    let sym_b = rt::symbol(None, "b");
    let va = Var::intern(Value::NIL, sym_a, Value::int(0));
    let vb = Var::intern(Value::NIL, sym_b, Value::int(0));
    let _ = Var::set_dynamic(va);
    let _ = Var::set_dynamic(vb);
    let outer = rt::array_map(&[va, Value::int(7), vb, Value::int(8)]);
    let inner = rt::array_map(&[va, Value::int(70)]);
    rt::push_thread_bindings(outer);
    rt::push_thread_bindings(inner);
    let ra = rt::deref(va);
    let rb = rt::deref(vb);
    assert_eq!(ra.as_int(), Some(70), "inner rebound a");
    assert_eq!(rb.as_int(), Some(8), "outer's b inherited into inner");
    rt::pop_thread_bindings();
    rt::pop_thread_bindings();
    drop_all(&[ra, rb, outer, inner, va, vb, sym_a, sym_b]);
}

#[test]
fn thread_binding_with_nil_value_does_not_fall_through_to_root() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(42));
    let _ = Var::set_dynamic(v);
    let bindings = rt::array_map(&[v, Value::NIL]);
    rt::push_thread_bindings(bindings);
    let read = rt::deref(v);
    assert!(read.is_nil(), "binding to nil should override root");
    rt::pop_thread_bindings();
    drop_all(&[read, bindings, v, sym]);
}

// --- Identity equiv + hash --------------------------------------------------

#[test]
fn identity_equiv_distinguishes_distinct_vars_with_same_sym() {
    init();
    let sym = rt::symbol(None, "x");
    let a = Var::intern(Value::NIL, sym, Value::int(0));
    let b = Var::intern(Value::NIL, sym, Value::int(0));
    assert_eq!(rt::equiv(a, a).as_bool(), Some(true));
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false), "Var equality is identity");
    drop_all(&[a, b, sym]);
}

#[test]
fn var_works_as_phm_key_via_identity() {
    init();
    let sym = rt::symbol(None, "x");
    let v = Var::intern(Value::NIL, sym, Value::int(0));
    // Build a small map keyed by the var; lookup must find it.
    let m = rt::array_map(&[v, Value::int(123)]);
    let r = rt::get(m, v);
    assert_eq!(r.as_int(), Some(123));
    drop_all(&[r, m, v, sym]);
}

// --- Static-global pattern -------------------------------------------------

/// Demonstrates the JVM `public static final Var X = Var.intern(...)`
/// idiom translated to Rust: a `OnceLock<Value>` filled lazily on
/// first access. This is how the reader (and other bootstrap code)
/// will keep references to vars like `*data-readers*`, `*ns*`, etc.
fn star_data_readers() -> Value {
    static SLOT: OnceLock<Value> = OnceLock::new();
    *SLOT.get_or_init(|| {
        let sym = rt::symbol(None, "*data-readers*");
        let v = Var::intern(Value::NIL, sym, rt::array_map(&[]));
        let _ = Var::set_dynamic(v);
        // Drop the local sym ref — Var::intern dup'd it.
        drop_value(sym);
        v
    })
}

#[test]
fn static_global_var_round_trip_and_dynamic_binding() {
    init();
    let v = star_data_readers();
    // Default root: empty map.
    let r = rt::deref(v);
    assert_eq!(rt::count(r).as_int(), Some(0));
    drop_value(r);
    // Override with a thread binding.
    let new_readers = rt::array_map(&[
        rt::symbol(None, "tag"), rt::keyword(None, "fn"),
    ]);
    let bindings = rt::array_map(&[v, new_readers]);
    rt::push_thread_bindings(bindings);
    let inside = rt::deref(v);
    assert_eq!(rt::count(inside).as_int(), Some(1));
    drop_value(inside);
    rt::pop_thread_bindings();
    // Multiple lookups return the same Var instance — the
    // OnceLock is filled exactly once.
    let v2 = star_data_readers();
    assert!(rt::equiv(v, v2).as_bool().unwrap_or(false));
    // Note: we deliberately don't drop_value(v / v2) — the
    // OnceLock holds a permanent ref to the var (that's the whole
    // point of the static-global pattern).
    drop_all(&[bindings, new_readers]);
}
