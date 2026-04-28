//! Bootstrap fixtures the reader (and later the evaluator) depend
//! on: a process-global `gensym` counter, the small set of static
//! `Var` slots that act as reader configuration knobs, and the
//! source-position-meta helper used to attach `:line`/`:column`
//! to forms.
//!
//! These live here rather than in their respective type modules
//! because they're cross-cutting glue — the reader threads them
//! together but doesn't *own* any of them. Each piece is small,
//! and keeping them in one file makes it easy to see what the
//! pre-reader runtime exposes.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::namespace::Namespace;
use crate::types::var::Var;
use crate::value::Value;

// --- gensym -----------------------------------------------------------------

static GENSYM_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Produce a fresh `Symbol` of the form `<prefix>__<n>__auto__`
/// where `<n>` is monotonically increasing across the process.
/// The reader uses this for syntax-quote `foo#` auto-syms; we
/// expose it as a free fn so the same counter can be reused by
/// macroexpansion when that lands.
///
/// Borrow semantics: returned Value carries one fresh ref.
pub fn gensym(prefix: &str) -> Value {
    let n = GENSYM_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!("{prefix}__{n}__auto__");
    crate::rt::symbol(None, &name)
}

/// Reset the gensym counter — only useful in tests that want
/// deterministic symbol names. Not thread-safe; tests calling
/// this should be single-threaded.
#[doc(hidden)]
pub fn reset_gensym_counter_for_tests() {
    GENSYM_COUNTER.store(0, Ordering::Relaxed);
}

// --- Static-global Vars -----------------------------------------------------

/// `clojure.core` namespace, find-or-created lazily. The reader's
/// `*ns*` defaults to this when no caller has pushed a thread
/// binding; the evaluator will replace this with the user's
/// current namespace once that exists.
fn clojure_core_ns() -> Value {
    static SLOT: OnceLock<Value> = OnceLock::new();
    *SLOT.get_or_init(|| {
        let sym = crate::rt::symbol(None, "clojure.core");
        let ns = Namespace::find_or_create(sym);
        crate::rc::drop_value(sym);
        ns
    })
}

/// `*ns*` — the current namespace. Reader-keyword auto-resolve
/// (`::foo` → `:current-ns/foo`) and syntax-quote auto-qualify
/// (`` `foo `` → `current-ns/foo` for unmapped symbols) consult
/// this Var. Initial root: the `clojure.core` namespace.
pub fn current_ns_var() -> Value {
    static SLOT: OnceLock<Value> = OnceLock::new();
    *SLOT.get_or_init(|| {
        let sym = crate::rt::symbol(None, "*ns*");
        let v = Var::intern(Value::NIL, sym, clojure_core_ns());
        let _ = Var::set_dynamic(v);
        // Var::intern dup'd sym; drop our local.
        crate::rc::drop_value(sym);
        v
    })
}

/// `*data-readers*` — map of tag-symbol → reader-fn. The reader's
/// `#tag form` path looks here first; `*default-data-readers*` is
/// the fallback. Initial root: the empty-map singleton (already
/// shared, so cross-thread `deref` + `dup` round-trip safely).
pub fn data_readers_var() -> Value {
    static SLOT: OnceLock<Value> = OnceLock::new();
    *SLOT.get_or_init(|| {
        let sym = crate::rt::symbol(None, "*data-readers*");
        let v = Var::intern(
            Value::NIL,
            sym,
            crate::types::array_map::empty_array_map(),
        );
        let _ = Var::set_dynamic(v);
        crate::rc::drop_value(sym);
        v
    })
}

/// `*default-data-readers*` — the fallback map consulted after
/// `*data-readers*` for `#tag form`. Initial root: the empty-map
/// singleton.
pub fn default_data_readers_var() -> Value {
    static SLOT: OnceLock<Value> = OnceLock::new();
    *SLOT.get_or_init(|| {
        let sym = crate::rt::symbol(None, "*default-data-readers*");
        let v = Var::intern(
            Value::NIL,
            sym,
            crate::types::array_map::empty_array_map(),
        );
        let _ = Var::set_dynamic(v);
        crate::rc::drop_value(sym);
        v
    })
}

/// `*reader-features*` — the set of feature keywords reader
/// conditionals (`#?(:k1 e1 :k2 e2)`) match against. Initial
/// root: `#{:cljr}` (our platform tag — distinct from `:clj` /
/// `:cljs`).
pub fn reader_features_var() -> Value {
    static SLOT: OnceLock<Value> = OnceLock::new();
    *SLOT.get_or_init(|| {
        let sym = crate::rt::symbol(None, "*reader-features*");
        let cljr = crate::rt::keyword(None, "cljr");
        let initial = crate::rt::hash_set(&[cljr]);
        // Pre-share the set + its element so cross-thread reads
        // can dup safely. Keywords are already interned-shared,
        // but cover them too for the discipline.
        crate::rc::share(initial);
        crate::rc::share(cljr);
        let v = Var::intern(Value::NIL, sym, initial);
        let _ = Var::set_dynamic(v);
        crate::rc::drop_value(sym);
        crate::rc::drop_value(cljr);
        crate::rc::drop_value(initial);
        v
    })
}

// --- Source-position meta ---------------------------------------------------

/// Build a `{:line L :column C :file F}` meta map. `file` may be
/// the empty string when reading from a non-file source (e.g.
/// `read-string`); it's omitted from the map in that case to
/// match JVM behavior. Borrow semantics — returned map carries
/// one fresh ref.
pub fn source_pos_meta(line: i64, column: i64, file: &str) -> Value {
    let line_kw = crate::rt::keyword(None, "line");
    let col_kw = crate::rt::keyword(None, "column");
    let file_kw = crate::rt::keyword(None, "file");
    let m = if file.is_empty() {
        let kvs = [line_kw, Value::int(line), col_kw, Value::int(column)];
        crate::rt::array_map(&kvs)
    } else {
        let file_str = crate::rt::str_new(file);
        let kvs = [
            line_kw,
            Value::int(line),
            col_kw,
            Value::int(column),
            file_kw,
            file_str,
        ];
        let m = crate::rt::array_map(&kvs);
        crate::rc::drop_value(file_str);
        m
    };
    crate::rc::drop_value(line_kw);
    crate::rc::drop_value(col_kw);
    crate::rc::drop_value(file_kw);
    m
}

/// Attach `(:line :column :file)` source-position metadata to
/// `form`. Existing meta on the form (e.g., from a `^...` reader
/// macro) is preserved — user-set keys win over source-pos keys
/// when they collide. Falls through to `form` (with a dup) if
/// `form` doesn't satisfy `IWithMeta` (primitives, strings,
/// keywords).
pub fn with_source_pos(form: Value, line: i64, column: i64, file: &str) -> Value {
    if !crate::protocol::satisfies(
        &crate::protocols::meta::IWithMeta::WITH_META_2,
        form,
    ) {
        crate::rc::dup(form);
        return form;
    }
    let pos_meta = source_pos_meta(line, column, file);
    let cur_meta = crate::rt::meta(form);
    let merged = if cur_meta.is_nil() {
        crate::rc::drop_value(cur_meta);
        pos_meta
    } else {
        // Merge cur_meta over pos_meta — user-set keys override
        // source-pos keys.
        let mut acc = pos_meta;
        let mut s = crate::rt::seq(cur_meta);
        while !s.is_nil() {
            let entry = crate::rt::first(s);
            let k = crate::rt::key(entry);
            let v = crate::rt::val(entry);
            let next = crate::rt::assoc(acc, k, v);
            crate::rc::drop_value(acc);
            crate::rc::drop_value(k);
            crate::rc::drop_value(v);
            crate::rc::drop_value(entry);
            acc = next;
            let n = crate::rt::next(s);
            crate::rc::drop_value(s);
            s = n;
        }
        crate::rc::drop_value(s);
        crate::rc::drop_value(cur_meta);
        acc
    };
    let r = crate::rt::with_meta(form, merged);
    crate::rc::drop_value(merged);
    r
}
