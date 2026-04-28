//! `Volatile` direct tests — round-trip, vreset!, vswap!.

use clojure_rt::{drop_value, init, rt, Value};

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

#[test]
fn volatile_round_trip() {
    init();
    let v = rt::volatile(Value::int(42));
    let r = rt::deref(v);
    assert_eq!(r.as_int(), Some(42));
    drop_all(&[r, v]);
}

#[test]
fn vreset_replaces_value_and_returns_new() {
    init();
    let v = rt::volatile(Value::int(0));
    let r = rt::vreset_bang(v, Value::int(7));
    assert_eq!(r.as_int(), Some(7));
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(7));
    drop_all(&[r, read, v]);
}

// --- vswap! ---------------------------------------------------------------

mod fns {
    use clojure_rt::value::Value;

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
        let name: &'static str = Box::leak(format!("v_test_fn_{id_seed}").into_boxed_str());
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

    pub fn inc() -> Value {
        make(&IFn::INVOKE_2, super::fns::inc_invoke_2)
    }
    pub fn add() -> Value {
        make(&IFn::INVOKE_3, super::fns::add_invoke_3)
    }
}

#[test]
fn vswap_arity_2_inc() {
    init();
    let v = rt::volatile(Value::int(0));
    let inc = test_fn_type::inc();
    for _ in 0..5 {
        let r = rt::vswap_bang(v, inc, &[]);
        drop_value(r);
    }
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(5));
    drop_all(&[read, v]);
}

#[test]
fn vswap_arity_3_add() {
    init();
    let v = rt::volatile(Value::int(10));
    let add = test_fn_type::add();
    let r = rt::vswap_bang(v, add, &[Value::int(5)]);
    assert_eq!(r.as_int(), Some(15));
    let read = rt::deref(v);
    assert_eq!(read.as_int(), Some(15));
    drop_all(&[r, read, v]);
}
