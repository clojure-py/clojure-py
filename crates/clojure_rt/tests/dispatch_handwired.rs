//! End-to-end dispatch test. Hand-wired (no macros) so we can validate
//! the IC, tier-2 build, tier-3, and slow-path resolver in isolation.

use core::alloc::Layout;
use std::sync::Arc;

use clojure_rt::dispatch::ic::{ICSlot, want_key};
use clojure_rt::dispatch::perfect_hash::PerTypeTable;
use clojure_rt::dispatch::dispatch_fn;
use clojure_rt::gc::{install_allocator, GcAllocator};
use clojure_rt::gc::naive::NAIVE;
use clojure_rt::header::Header;
use clojure_rt::protocol::ProtocolMethod;
use clojure_rt::type_registry::{register_static_type, get};
use clojure_rt::Value;

unsafe fn no_destruct(_: *mut Header) {}

fn ensure_installed() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| install_allocator(&NAIVE));
}

unsafe extern "C" fn foo_greet(_args: *const Value, _n: usize) -> Value {
    Value::int(100)
}
unsafe extern "C" fn bar_greet(_args: *const Value, _n: usize) -> Value {
    Value::int(200)
}

#[test]
fn dispatch_hits_ic_after_first_resolve() {
    ensure_installed();

    static GREET: ProtocolMethod = ProtocolMethod::new("Greeter/greet");

    // Patch method_id at runtime (the macros do this via init).
    // SAFETY: single-threaded test setup before any dispatch.
    unsafe {
        let m: *const ProtocolMethod = &GREET;
        (*(m as *mut ProtocolMethod)).method_id = 7;
        (*(m as *mut ProtocolMethod)).proto_id  = 1;
    }

    let body = Layout::from_size_align(8, 8).unwrap();
    let foo_id = register_static_type("Foo", body, no_destruct);
    let bar_id = register_static_type("Bar", body, no_destruct);

    // Install perfect-hash tables for both types.
    get(foo_id).table.store(Arc::new(
        PerTypeTable::build(&[(7, foo_greet as *const ())])
    ));
    get(bar_id).table.store(Arc::new(
        PerTypeTable::build(&[(7, bar_greet as *const ())])
    ));

    // Construct heap values.
    let foo_val = unsafe {
        let h = NAIVE.alloc(body, foo_id);
        Value::from_heap(h)
    };
    let bar_val = unsafe {
        let h = NAIVE.alloc(body, bar_id);
        Value::from_heap(h)
    };

    static IC: ICSlot = ICSlot::EMPTY;

    // First call: IC miss, falls to slow path, fills IC and tier 3.
    let r1 = dispatch_fn(&IC, &GREET, &[foo_val]);
    assert_eq!(r1.as_int(), Some(100));

    // IC should now hit for foo.
    let want = want_key(foo_val, &GREET);
    assert!(IC.read(want).is_some(), "IC must be filled after first call");

    // Second call same type: tier 1 hit.
    let r2 = dispatch_fn(&IC, &GREET, &[foo_val]);
    assert_eq!(r2.as_int(), Some(100));

    // Different type: IC miss, slow path, but tier 3 may already be warm.
    let r3 = dispatch_fn(&IC, &GREET, &[bar_val]);
    assert_eq!(r3.as_int(), Some(200));
}

#[test]
fn slow_path_panics_on_unimplemented() {
    ensure_installed();

    static M: ProtocolMethod = ProtocolMethod::new("Missing/method");
    unsafe {
        let m: *const ProtocolMethod = &M;
        (*(m as *mut ProtocolMethod)).method_id = 9999;
        (*(m as *mut ProtocolMethod)).proto_id  = 99;
    }

    let body = Layout::from_size_align(8, 8).unwrap();
    let id = register_static_type("Empty", body, no_destruct);
    let v = unsafe { Value::from_heap(NAIVE.alloc(body, id)) };

    static IC: ICSlot = ICSlot::EMPTY;
    let result = std::panic::catch_unwind(|| {
        dispatch_fn(&IC, &M, &[v])
    });
    assert!(result.is_err(), "expected panic on unimplemented method");
}
