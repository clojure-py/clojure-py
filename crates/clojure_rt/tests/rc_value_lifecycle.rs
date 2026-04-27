//! End-to-end: register a type, alloc through the global allocator,
//! dup/drop_value at the Value layer, watch the live counter return to 0.

use core::alloc::Layout;

use clojure_rt::gc::naive::NAIVE;
use clojure_rt::gc::{install_allocator, GcAllocator};
use clojure_rt::header::Header;
use clojure_rt::type_registry::register_static_type;
use clojure_rt::{Value, dup, drop_value};

unsafe fn empty_destruct(_: *mut Header) {}

fn ensure_installed() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| install_allocator(&NAIVE));
}

#[test]
fn alloc_dup_drop_returns_to_zero() {
    ensure_installed();
    let body = Layout::from_size_align(16, 8).unwrap();
    let id = register_static_type("LifecycleTest", body, empty_destruct);

    let before = NAIVE.live_count();
    unsafe {
        let h = NAIVE.alloc(body, id);
        let v = Value::from_heap(h);
        assert_eq!(NAIVE.live_count(), before + 1);

        dup(v);                                  // rc: -1 -> -2
        drop_value(v);                           // rc: -2 -> -1
        assert_eq!(NAIVE.live_count(), before + 1);

        drop_value(v);                           // rc: -1 -> 0, destruct + dealloc
        assert_eq!(NAIVE.live_count(), before);
    }
}
