//! `inventory`-driven static-init walker. The proc-macros emit submissions
//! of these types; `clojure_rt::init()` walks them at startup.

use core::alloc::Layout;
use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Once};

use once_cell::sync::OnceCell;

use crate::dispatch::perfect_hash::PerTypeTable;
use crate::header::Header;
use crate::gc::{allocator_uninstalled, install_allocator};
use crate::protocol::ProtocolMethod;
use crate::type_registry::{register_static_type, get};
use crate::value::TypeId;

/// One static type registration submitted by `register_type!`.
pub struct StaticTypeRegistration {
    pub name:     &'static str,
    pub layout:   Layout,
    pub destruct: unsafe fn(*mut Header),
    pub id_cell:  &'static OnceCell<TypeId>,
}

inventory::collect!(StaticTypeRegistration);

/// One static protocol registration submitted by `protocol!`.
pub struct StaticProtocolRegistration {
    pub name:    &'static str,
    pub methods: &'static [StaticProtocolMethodEntry],
}

pub struct StaticProtocolMethodEntry {
    pub name:           &'static str,
    pub method:         &'static ProtocolMethod,
    pub method_id_cell: &'static OnceCell<u32>,
}

inventory::collect!(StaticProtocolRegistration);

/// One static impl registration submitted by `implements!`.
pub struct StaticImplRegistration {
    pub type_cell:      &'static OnceCell<TypeId>,
    pub method_id_cell: &'static OnceCell<u32>,
    pub method_version: &'static AtomicU32,    // bumped after install
    pub fn_ptr:         *const (),
}

unsafe impl Sync for StaticImplRegistration {}

inventory::collect!(StaticImplRegistration);

static NEXT_PROTO_ID:  AtomicU32 = AtomicU32::new(1);
static NEXT_METHOD_ID: AtomicU32 = AtomicU32::new(1);   // 0 reserved for empty slot

static INIT: Once = Once::new();

/// Initialize the runtime: install allocator if absent, walk inventory,
/// assign IDs, build per-type tier-2 tables. Idempotent.
pub fn init() {
    INIT.call_once(|| {
        if allocator_uninstalled() {
            install_allocator(&crate::gc::rcimmix::RCIMMIX);
        }

        // 1. Protocols and methods.
        for proto in inventory::iter::<StaticProtocolRegistration> {
            let proto_id = NEXT_PROTO_ID.fetch_add(1, Ordering::AcqRel);
            for entry in proto.methods {
                let method_id = NEXT_METHOD_ID.fetch_add(1, Ordering::AcqRel);
                entry.method_id_cell.set(method_id).ok();
                entry.method.method_id.store(method_id, Ordering::Release);
                entry.method.proto_id.store(proto_id, Ordering::Release);
            }
        }

        // 1.5. Primitives — first-class type-id slots so impls targeting
        // Nil/Bool/Int64/etc. resolve through the same per-type table as
        // heap types.
        crate::primitives::init();

        // 2. Types.
        for ty in inventory::iter::<StaticTypeRegistration> {
            let id = register_static_type(ty.name, ty.layout, ty.destruct);
            ty.id_cell.set(id).ok();
        }

        // 3. Impls — group by type, build a perfect-hash table per type.
        let mut by_type: std::collections::HashMap<TypeId, Vec<(u32, *const ())>>
            = std::collections::HashMap::new();
        for imp in inventory::iter::<StaticImplRegistration> {
            let tid = *imp.type_cell.get().expect("type cell unset at init");
            let mid = *imp.method_id_cell.get().expect("method id cell unset at init");
            by_type.entry(tid).or_default().push((mid, imp.fn_ptr));
            // Bump version so any pre-init IC slots invalidate.
            imp.method_version.fetch_add(1, Ordering::Release);
        }
        for (tid, entries) in by_type {
            let table = Arc::new(PerTypeTable::build(&entries));
            get(tid).table.store(table);
        }
    });
}
