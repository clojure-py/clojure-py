//! `Namespace` — a named container of `Symbol → Var` mappings plus
//! `Symbol → Namespace` aliases. Mirrors JVM `clojure.lang.Namespace`.
//!
//! # Storage
//! Three `ArcSwap<NsCell>` slots holding persistent maps:
//! - `mappings` — interned `Var`s plus referrals from other ns's
//! - `aliases`  — short names for other namespaces (used by
//!                `(alias 'a 'foo.bar)` and the reader's keyword
//!                auto-resolve `::a/x`)
//! - `meta`     — mutable meta (matches `clojure.lang.IReference`)
//!
//! Plus immutable `name: Value` (a `Symbol`) for identification.
//!
//! # Global registry
//! `Namespace::find_or_create(sym)` interns a namespace into a
//! process-global `parking_lot::Mutex<HashMap<…>>` keyed by the
//! namespace's name string. `find(sym)` looks it up; `remove(sym)`
//! drops it. The registry holds one ref per interned namespace so
//! the namespace stays alive as long as it's findable by name —
//! same shape as JVM's `Namespace.namespaces`.
//!
//! # Identity
//! Two `Namespace` values are equal iff they're the same heap
//! object. Hash is identity-based. Matches JVM (Namespace doesn't
//! override `equals`/`hashCode`).
//!
//! # Var lifecycle
//! `Namespace::intern_var(ns, sym, root)` creates a fresh `Var`
//! whose `ns`/`sym` fields point at this namespace and the given
//! symbol, and installs it under that symbol in `ns`'s mappings.
//! Re-interning the same name returns the same `Var` (with the
//! root left untouched — JVM behavior).

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use parking_lot::Mutex;

use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::meta::IMeta;
use crate::protocols::reference::IReference;
use crate::types::string::StringObj;
use crate::types::var::Var;
use crate::value::Value;

pub(crate) struct NsCell {
    pub(crate) v: Value,
}

impl Drop for NsCell {
    fn drop(&mut self) {
        crate::rc::drop_value(self.v);
    }
}

clojure_rt_macros::register_type! {
    pub struct Namespace {
        mappings: ArcSwap<NsCell>,
        aliases:  ArcSwap<NsCell>,
        meta:     ArcSwap<NsCell>,
        name:     Value,    // a Symbol
    }
}

#[inline]
fn cell(v: Value) -> Arc<NsCell> {
    Arc::new(NsCell { v })
}

#[inline]
fn cell_dup(v: Value) -> Arc<NsCell> {
    crate::rc::dup(v);
    Arc::new(NsCell { v })
}

// --- Global registry --------------------------------------------------------

/// Process-wide registry of interned namespaces, keyed by name
/// string. The map holds one ref per namespace — they stay alive
/// for as long as they're findable by name.
fn registry() -> &'static Mutex<HashMap<String, Value>> {
    static REGISTRY: once_cell::sync::OnceCell<Mutex<HashMap<String, Value>>>
        = once_cell::sync::OnceCell::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

// --- Namespace construction + lookup ---------------------------------------

impl Namespace {
    /// Find the namespace named by `sym` in the global registry.
    /// Returns `None` if absent (does not auto-create — use
    /// `find_or_create` for that). Borrow semantics: caller still
    /// owns `sym`; the returned Value (when `Some`) carries a fresh
    /// +1 ref.
    pub fn find(sym: Value) -> Option<Value> {
        let key = sym_to_key(sym)?;
        let g = registry().lock();
        let v = g.get(&key)?;
        crate::rc::dup(*v);
        Some(*v)
    }

    /// Find-or-create the namespace named by `sym`. If the name is
    /// already interned, returns the existing instance; otherwise
    /// creates a fresh empty namespace and installs it. Borrow
    /// semantics on `sym`.
    pub fn find_or_create(sym: Value) -> Value {
        let key = sym_to_key(sym)
            .expect("Namespace::find_or_create: sym must be a Symbol with a name");
        let mut g = registry().lock();
        if let Some(existing) = g.get(&key) {
            crate::rc::dup(*existing);
            return *existing;
        }
        crate::rc::dup(sym);
        crate::rc::share(sym);
        let ns = Namespace::alloc(
            ArcSwap::from(cell(crate::rt::array_map(&[]))),
            ArcSwap::from(cell(crate::rt::array_map(&[]))),
            ArcSwap::from(cell(Value::NIL)),
            sym,
        );
        crate::rc::share(ns);
        // The registry holds one ref; hand out a separate +1 to
        // the caller.
        crate::rc::dup(ns);
        g.insert(key, ns);
        ns
    }

    /// Remove `sym`'s namespace from the registry, dropping the
    /// registry's ref. The namespace itself lives on as long as
    /// other refs exist. Returns `true` if a namespace was
    /// removed, `false` if no entry was present.
    pub fn remove(sym: Value) -> bool {
        let Some(key) = sym_to_key(sym) else { return false; };
        let mut g = registry().lock();
        if let Some(v) = g.remove(&key) {
            crate::rc::drop_value(v);
            true
        } else {
            false
        }
    }

    pub fn name(ns: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        crate::rc::dup(body.name);
        body.name
    }

    /// Snapshot the current mappings (a PAM/PHM of `Symbol → Var`).
    /// Borrow semantics: caller gets a fresh +1.
    pub fn mappings(ns: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        let snap = body.mappings.load_full();
        let v = snap.v;
        crate::rc::dup(v);
        v
    }

    /// `(get-mapping ns sym)` — look up a symbol's binding (a
    /// `Var`) in `ns`'s mappings, or `Value::NIL` if unmapped.
    /// Borrow semantics on both args.
    pub fn get_mapping(ns: Value, sym: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        let snap = body.mappings.load_full();
        crate::rt::get(snap.v, sym)
    }

    /// `(intern ns sym)` / `(intern ns sym val)` — find-or-create
    /// a `Var` named `sym` in `ns`. If the symbol is already
    /// mapped, returns the existing `Var` without touching its
    /// root (matches JVM `Namespace.intern` behavior). Otherwise
    /// creates a fresh `Var` and registers it.
    ///
    /// `root` is the initial value for a newly-created var; it's
    /// ignored when the symbol is already interned. Borrow
    /// semantics on all three args.
    pub fn intern_var(ns: Value, sym: Value, root: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        loop {
            let snap = body.mappings.load_full();
            let cur = snap.v;
            let existing = crate::rt::get(cur, sym);
            if !existing.is_nil() {
                return existing;
            }
            crate::rc::drop_value(existing);
            let new_var = Var::intern(ns, sym, root);
            crate::rc::dup(cur);
            let new_map = crate::rt::assoc(cur, sym, new_var);
            crate::rc::drop_value(cur);
            crate::rc::share(new_map);
            let new_arc = cell(new_map);
            let witness = body.mappings.compare_and_swap(&snap, new_arc);
            if Arc::ptr_eq(&witness, &snap) {
                // CAS succeeded; new_var is owned by both the map
                // and our return slot — bump for the caller.
                return new_var;
            }
            // CAS lost: the new_var we minted is unused. Drop it.
            // (In a future refinement we could re-try without
            // re-creating the Var, but this is the simple shape.)
            crate::rc::drop_value(new_var);
        }
    }

    /// Snapshot the current aliases (`Symbol → Namespace`).
    pub fn aliases(ns: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        let snap = body.aliases.load_full();
        let v = snap.v;
        crate::rc::dup(v);
        v
    }

    /// `(alias alias-sym target-ns)` on `ns` — install a name
    /// shortcut from `alias_sym` to `target_ns`. Re-aliasing the
    /// same alias to a different namespace is allowed (JVM behavior
    /// is to throw; we match the looser cljs convention so reader
    /// `::alias/foo` resolution stays simple). Borrow semantics.
    pub fn add_alias(ns: Value, alias_sym: Value, target: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        loop {
            let snap = body.aliases.load_full();
            let cur = snap.v;
            crate::rc::dup(cur);
            let new_map = crate::rt::assoc(cur, alias_sym, target);
            crate::rc::drop_value(cur);
            crate::rc::share(new_map);
            let new_arc = cell(new_map);
            let witness = body.aliases.compare_and_swap(&snap, new_arc);
            if Arc::ptr_eq(&witness, &snap) {
                return Value::NIL;
            }
        }
    }

    /// `(ns-aliases ns)` lookup — returns the target namespace for
    /// `alias_sym`, or `Value::NIL` when unmapped.
    pub fn lookup_alias(ns: Value, alias_sym: Value) -> Value {
        let body = unsafe { Namespace::body(ns) };
        let snap = body.aliases.load_full();
        crate::rt::get(snap.v, alias_sym)
    }
}

/// Extract a Symbol's fully-qualified name as a registry key
/// (`ns/name` if `ns` is present, otherwise just `name`). Returns
/// `None` for non-symbol values. Uses `INamed` rather than poking
/// at private fields.
fn sym_to_key(sym: Value) -> Option<String> {
    let sym_tid = *crate::types::symbol::SYMBOLOBJ_TYPE_ID.get()?;
    if sym.tag != sym_tid {
        return None;
    }
    let name_v = crate::rt::name(sym);
    let name = unsafe { StringObj::as_str_unchecked(name_v) }.to_string();
    let ns_v = crate::rt::namespace(sym);
    let key = if ns_v.is_nil() {
        name
    } else {
        let ns = unsafe { StringObj::as_str_unchecked(ns_v) }.to_string();
        format!("{ns}/{name}")
    };
    crate::rc::drop_value(name_v);
    crate::rc::drop_value(ns_v);
    Some(key)
}

// --- Identity equiv + hash --------------------------------------------------

clojure_rt_macros::implements! {
    impl IEquiv for Namespace {
        fn equiv(this: Value, other: Value) -> Value {
            if this.tag == other.tag && this.payload == other.payload {
                Value::TRUE
            } else {
                Value::FALSE
            }
        }
    }
}

clojure_rt_macros::implements! {
    impl IHash for Namespace {
        fn hash(this: Value) -> Value {
            let p = this.payload as u64;
            let h = (p ^ (p >> 32)) as i32;
            Value::int(h as i64)
        }
    }
}

// --- IMeta + IReference (mutable meta) -------------------------------------

clojure_rt_macros::implements! {
    impl IMeta for Namespace {
        fn meta(this: Value) -> Value {
            let body = unsafe { Namespace::body(this) };
            let snap = body.meta.load_full();
            let m = snap.v;
            crate::rc::dup(m);
            m
        }
    }
}

clojure_rt_macros::implements! {
    impl IReference for Namespace {
        fn reset_meta(this: Value, m: Value) -> Value {
            let body = unsafe { Namespace::body(this) };
            crate::rc::share(m);
            body.meta.store(cell_dup(m));
            crate::rc::dup(m);
            m
        }
        fn alter_meta_2(this: Value, f: Value) -> Value {
            alter_meta_impl(this, f, &[])
        }
        fn alter_meta_3(this: Value, f: Value, a1: Value) -> Value {
            alter_meta_impl(this, f, &[a1])
        }
        fn alter_meta_4(this: Value, f: Value, a1: Value, a2: Value) -> Value {
            alter_meta_impl(this, f, &[a1, a2])
        }
        fn alter_meta_5(this: Value, f: Value, a1: Value, a2: Value, a3: Value) -> Value {
            alter_meta_impl(this, f, &[a1, a2, a3])
        }
    }
}

fn alter_meta_impl(this: Value, f: Value, extra: &[Value]) -> Value {
    let body = unsafe { Namespace::body(this) };
    loop {
        let snap = body.meta.load_full();
        let cur = snap.v;
        let mut args: Vec<Value> = Vec::with_capacity(1 + extra.len());
        args.push(cur);
        args.extend_from_slice(extra);
        let new_m = crate::rt::invoke(f, &args);
        if new_m.is_exception() {
            return new_m;
        }
        crate::rc::share(new_m);
        let new_arc = cell(new_m);
        let witness = body.meta.compare_and_swap(&snap, new_arc);
        if Arc::ptr_eq(&witness, &snap) {
            crate::rc::dup(new_m);
            return new_m;
        }
    }
}
