//! `Namespace` tests — global registry, intern_var idempotence,
//! mappings/aliases CAS, identity equality, mutable meta, plus
//! the bootstrap fixtures (`*ns*`, `*data-readers*`, gensym,
//! source-pos meta).

use clojure_rt::{drop_value, init, rt, Value};
use clojure_rt::bootstrap;
use clojure_rt::types::namespace::Namespace;
use clojure_rt::types::var::Var;

fn drop_all(vs: &[Value]) { for &v in vs { drop_value(v); } }

// --- Registry --------------------------------------------------------------

#[test]
fn find_or_create_returns_same_instance_for_same_name() {
    init();
    let sym = rt::symbol(None, "ns.find-or-create.same");
    let a = Namespace::find_or_create(sym);
    let b = Namespace::find_or_create(sym);
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    Namespace::remove(sym);
    drop_all(&[a, b, sym]);
}

#[test]
fn find_returns_none_for_unknown_name() {
    init();
    let sym = rt::symbol(None, "ns.find.never-interned-zzz");
    assert!(Namespace::find(sym).is_none());
    drop_value(sym);
}

#[test]
fn find_returns_some_after_create() {
    init();
    let sym = rt::symbol(None, "ns.find.after-create");
    let a = Namespace::find_or_create(sym);
    let b = Namespace::find(sym).expect("should be present");
    assert!(rt::equiv(a, b).as_bool().unwrap_or(false));
    Namespace::remove(sym);
    drop_all(&[a, b, sym]);
}

#[test]
fn remove_drops_registry_ref() {
    init();
    let sym = rt::symbol(None, "ns.remove.drops");
    let _ = Namespace::find_or_create(sym);
    let removed = Namespace::remove(sym);
    assert!(removed);
    let removed_again = Namespace::remove(sym);
    assert!(!removed_again, "second remove is a no-op");
    drop_value(sym);
}

#[test]
fn name_round_trips() {
    init();
    let sym = rt::symbol(None, "ns.name.round-trip");
    let ns = Namespace::find_or_create(sym);
    let read = Namespace::name(ns);
    assert!(rt::equiv(read, sym).as_bool().unwrap_or(false));
    Namespace::remove(sym);
    drop_all(&[read, ns, sym]);
}

// --- intern_var -----------------------------------------------------------

#[test]
fn intern_var_creates_and_registers() {
    init();
    let ns_sym = rt::symbol(None, "ns.intern-var.creates");
    let ns = Namespace::find_or_create(ns_sym);
    let var_sym = rt::symbol(None, "x");
    let v = Namespace::intern_var(ns, var_sym, Value::int(42));
    // Var is now mapped under var_sym.
    let mapped = Namespace::get_mapping(ns, var_sym);
    assert!(rt::equiv(v, mapped).as_bool().unwrap_or(false));
    // Reading the var's root.
    let r = rt::deref(v);
    assert_eq!(r.as_int(), Some(42));
    Namespace::remove(ns_sym);
    drop_all(&[r, mapped, v, var_sym, ns, ns_sym]);
}

#[test]
fn intern_var_is_idempotent() {
    // Re-interning the same symbol returns the same Var, leaving
    // the previously-set root untouched.
    init();
    let ns_sym = rt::symbol(None, "ns.intern-var.idempotent");
    let ns = Namespace::find_or_create(ns_sym);
    let var_sym = rt::symbol(None, "x");
    let v1 = Namespace::intern_var(ns, var_sym, Value::int(1));
    let v2 = Namespace::intern_var(ns, var_sym, Value::int(99));
    assert!(rt::equiv(v1, v2).as_bool().unwrap_or(false), "same Var");
    let r = rt::deref(v1);
    assert_eq!(r.as_int(), Some(1), "root preserved on re-intern");
    Namespace::remove(ns_sym);
    drop_all(&[r, v1, v2, var_sym, ns, ns_sym]);
}

#[test]
fn get_mapping_returns_nil_for_unmapped() {
    init();
    let ns_sym = rt::symbol(None, "ns.get-mapping.nil");
    let ns = Namespace::find_or_create(ns_sym);
    let unknown = rt::symbol(None, "no-such-thing");
    let r = Namespace::get_mapping(ns, unknown);
    assert!(r.is_nil());
    Namespace::remove(ns_sym);
    drop_all(&[unknown, ns, ns_sym]);
}

// --- Aliases ---------------------------------------------------------------

#[test]
fn add_alias_and_lookup() {
    init();
    let ns_sym = rt::symbol(None, "ns.aliases.add");
    let target_sym = rt::symbol(None, "ns.aliases.target");
    let ns = Namespace::find_or_create(ns_sym);
    let target = Namespace::find_or_create(target_sym);
    let alias = rt::symbol(None, "t");
    let _ = Namespace::add_alias(ns, alias, target);
    let looked_up = Namespace::lookup_alias(ns, alias);
    assert!(rt::equiv(looked_up, target).as_bool().unwrap_or(false));
    Namespace::remove(ns_sym);
    Namespace::remove(target_sym);
    drop_all(&[looked_up, alias, target, ns, ns_sym, target_sym]);
}

#[test]
fn lookup_alias_nil_when_absent() {
    init();
    let ns_sym = rt::symbol(None, "ns.aliases.absent");
    let ns = Namespace::find_or_create(ns_sym);
    let unknown_alias = rt::symbol(None, "no-alias");
    let r = Namespace::lookup_alias(ns, unknown_alias);
    assert!(r.is_nil());
    Namespace::remove(ns_sym);
    drop_all(&[unknown_alias, ns, ns_sym]);
}

// --- Identity --------------------------------------------------------------

#[test]
fn identity_equiv_same_instance_only() {
    init();
    let sym_a = rt::symbol(None, "ns.identity.a");
    let sym_b = rt::symbol(None, "ns.identity.b");
    let ns_a = Namespace::find_or_create(sym_a);
    let ns_a2 = Namespace::find_or_create(sym_a);
    let ns_b = Namespace::find_or_create(sym_b);
    assert_eq!(rt::equiv(ns_a, ns_a2).as_bool(), Some(true));
    assert_eq!(rt::equiv(ns_a, ns_b).as_bool(), Some(false));
    Namespace::remove(sym_a);
    Namespace::remove(sym_b);
    drop_all(&[ns_a, ns_a2, ns_b, sym_a, sym_b]);
}

// --- Bootstrap fixtures ----------------------------------------------------

#[test]
fn gensym_produces_distinct_symbols_with_prefix() {
    init();
    bootstrap::reset_gensym_counter_for_tests();
    let a = bootstrap::gensym("foo");
    let b = bootstrap::gensym("foo");
    let c = bootstrap::gensym("bar");
    assert_eq!(rt::equiv(a, b).as_bool(), Some(false));
    assert_eq!(rt::equiv(a, c).as_bool(), Some(false));
    drop_all(&[a, b, c]);
}

#[test]
fn current_ns_var_returns_namespace_at_root() {
    init();
    let v = bootstrap::current_ns_var();
    let root = rt::deref(v);
    // Root is the clojure.core namespace.
    let core_sym = rt::symbol(None, "clojure.core");
    let core_ns = Namespace::find(core_sym).expect("clojure.core registered");
    assert!(rt::equiv(root, core_ns).as_bool().unwrap_or(false));
    drop_all(&[root, core_ns, core_sym]);
}

#[test]
fn current_ns_var_can_be_thread_bound() {
    init();
    let v = bootstrap::current_ns_var();
    let other_sym = rt::symbol(None, "ns.thread-bound.other");
    let other = Namespace::find_or_create(other_sym);
    let bindings = rt::array_map(&[v, other]);
    rt::push_thread_bindings(bindings);
    let bound = rt::deref(v);
    assert!(rt::equiv(bound, other).as_bool().unwrap_or(false));
    rt::pop_thread_bindings();
    Namespace::remove(other_sym);
    drop_all(&[bound, bindings, other, other_sym]);
}

#[test]
fn data_readers_var_starts_empty_and_dynamic() {
    init();
    let v = bootstrap::data_readers_var();
    let root = rt::deref(v);
    assert_eq!(rt::count(root).as_int(), Some(0));
    assert!(Var::is_dynamic(v));
    drop_value(root);
}

#[test]
fn reader_features_var_includes_cljr() {
    init();
    let v = bootstrap::reader_features_var();
    let root = rt::deref(v);
    let cljr = rt::keyword(None, "cljr");
    assert_eq!(rt::contains_key(root, cljr).as_bool(), Some(true));
    drop_all(&[root, cljr]);
}

#[test]
fn source_pos_meta_with_file() {
    init();
    let m = bootstrap::source_pos_meta(7, 13, "core.clj");
    let line_kw = rt::keyword(None, "line");
    let col_kw = rt::keyword(None, "column");
    let file_kw = rt::keyword(None, "file");
    assert_eq!(rt::get(m, line_kw).as_int(), Some(7));
    assert_eq!(rt::get(m, col_kw).as_int(), Some(13));
    let file = rt::get(m, file_kw);
    assert!(!file.is_nil(), ":file present");
    drop_all(&[file, m, line_kw, col_kw, file_kw]);
}

#[test]
fn source_pos_meta_without_file_omits_key() {
    init();
    let m = bootstrap::source_pos_meta(2, 4, "");
    assert_eq!(rt::count(m).as_int(), Some(2), ":file omitted");
    drop_value(m);
}

#[test]
fn with_source_pos_attaches_meta_to_imeta_form() {
    init();
    // PersistentList satisfies IWithMeta.
    let lst = rt::list(&[Value::int(1), Value::int(2)]);
    let with_pos = bootstrap::with_source_pos(lst, 5, 10, "x.clj");
    let m = rt::meta(with_pos);
    let line_kw = rt::keyword(None, "line");
    assert_eq!(rt::get(m, line_kw).as_int(), Some(5));
    drop_all(&[m, with_pos, lst, line_kw]);
}

#[test]
fn with_source_pos_no_op_on_non_imeta_form() {
    init();
    // Integer doesn't satisfy IWithMeta; helper falls through.
    let r = bootstrap::with_source_pos(Value::int(42), 1, 1, "");
    assert_eq!(r.as_int(), Some(42));
}
