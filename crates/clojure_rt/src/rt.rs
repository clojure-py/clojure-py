//! Thin static dispatchers — Clojure's `RT.*` cross-roads.
//!
//! Each helper is one line: a `dispatch!` through the corresponding
//! protocol method. Type-specific behavior lives in the protocol's
//! built-in fallback or in per-type `implements!` blocks, never here.
//! Helpers return `Value` uniformly so that throwable-Value exceptions
//! propagate unobstructed; numeric extraction (`.as_int()`) happens at
//! the leaf call site.

use crate::protocols::associative::IAssociative;
use crate::protocols::atom::IAtom;
use crate::protocols::chunked_seq::IChunkedSeq;
use crate::protocols::pending::IPending;
use crate::protocols::r#ref::IRef;
use crate::protocols::reference::IReference;
use crate::protocols::volatile::IVolatile;
use crate::protocols::watchable::IWatchable;
use crate::protocols::editable_collection::IEditableCollection;
use crate::protocols::collection::ICollection;
use crate::protocols::counted::ICounted;
use crate::protocols::deref::IDeref;
use crate::protocols::emptyable_collection::IEmptyableCollection;
use crate::protocols::equiv::IEquiv;
use crate::protocols::hash::IHash;
use crate::protocols::ifn::IFn;
use crate::protocols::indexed::IIndexed;
use crate::protocols::lookup::ILookup;
use crate::protocols::map::IMap;
use crate::protocols::map_entry::IMapEntry;
use crate::protocols::meta::{IMeta, IWithMeta};
use crate::protocols::named::INamed;
use crate::protocols::reduce::IReduce;
use crate::protocols::reversible::IReversible;
use crate::protocols::persistent_set::IPersistentSet;
use crate::protocols::seq::{INext, ISeq, ISeqable};
use crate::protocols::sequential::ISequential;
use crate::protocols::set::ISet;
use crate::protocols::stack::IStack;
use crate::protocols::transient_associative::ITransientAssociative;
use crate::protocols::transient_collection::ITransientCollection;
use crate::protocols::transient_map::ITransientMap;
use crate::protocols::transient_vector::ITransientVector;
use crate::types::reduced::Reduced;
use crate::types::array_map::PersistentArrayMap;
use crate::types::atom::Atom;
use crate::types::delay::Delay;
use crate::types::hash_set::PersistentHashSet;
use crate::types::keyword::KeywordObj;
use crate::types::list::PersistentList;
use crate::types::var::Var;
use crate::types::volatile::Volatile;
use crate::types::string::StringObj;
use crate::types::symbol::SymbolObj;
use crate::types::vector::PersistentVector;
use crate::value::Value;

#[inline]
pub fn count(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ICounted::count, &[v])
}

#[inline]
pub fn hash(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IHash::hash, &[v])
}

#[inline]
pub fn equiv(a: Value, b: Value) -> Value {
    clojure_rt_macros::dispatch!(IEquiv::equiv, &[a, b])
}

#[inline]
pub fn str_new(s: &str) -> Value {
    StringObj::new(s)
}

#[inline]
pub fn symbol(ns: Option<&str>, name: &str) -> Value {
    SymbolObj::intern(ns, name)
}

#[inline]
pub fn keyword(ns: Option<&str>, name: &str) -> Value {
    KeywordObj::intern(ns, name)
}

#[inline]
pub fn name(v: Value) -> Value {
    clojure_rt_macros::dispatch!(INamed::name, &[v])
}

#[inline]
pub fn namespace(v: Value) -> Value {
    clojure_rt_macros::dispatch!(INamed::namespace, &[v])
}

#[inline]
pub fn meta(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IMeta::meta, &[v])
}

#[inline]
pub fn with_meta(v: Value, m: Value) -> Value {
    clojure_rt_macros::dispatch!(IWithMeta::with_meta, &[v, m])
}

// --- Seq abstraction --------------------------------------------------------

#[inline]
pub fn seq(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ISeqable::seq, &[v])
}

#[inline]
pub fn first(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ISeq::first, &[v])
}

#[inline]
pub fn rest(v: Value) -> Value {
    clojure_rt_macros::dispatch!(ISeq::rest, &[v])
}

#[inline]
pub fn next(v: Value) -> Value {
    clojure_rt_macros::dispatch!(INext::next, &[v])
}

// --- Collection ops ---------------------------------------------------------

#[inline]
pub fn conj(coll: Value, x: Value) -> Value {
    clojure_rt_macros::dispatch!(ICollection::conj, &[coll, x])
}

#[inline]
pub fn empty(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IEmptyableCollection::empty, &[coll])
}

#[inline]
pub fn peek(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IStack::peek, &[coll])
}

#[inline]
pub fn pop(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IStack::pop, &[coll])
}

#[inline]
pub fn nth(coll: Value, n: Value) -> Value {
    clojure_rt_macros::dispatch!(IIndexed::nth, &[coll, n])
}

#[inline]
pub fn nth_default(coll: Value, n: Value, not_found: Value) -> Value {
    clojure_rt_macros::dispatch!(IIndexed::nth, &[coll, n, not_found])
}

// --- Lookup -----------------------------------------------------------------

#[inline]
pub fn get(coll: Value, k: Value) -> Value {
    clojure_rt_macros::dispatch!(ILookup::lookup, &[coll, k])
}

#[inline]
pub fn get_default(coll: Value, k: Value, not_found: Value) -> Value {
    clojure_rt_macros::dispatch!(ILookup::lookup, &[coll, k, not_found])
}

// --- Associative ------------------------------------------------------------

#[inline]
pub fn assoc(coll: Value, k: Value, v: Value) -> Value {
    clojure_rt_macros::dispatch!(IAssociative::assoc, &[coll, k, v])
}

/// `(contains? coll k)` — JVM `RT.contains` analog: nil → false; sets
/// route through `ISet::contains`; everything else through
/// `IAssociative::contains_key`.
#[inline]
pub fn contains_key(coll: Value, k: Value) -> Value {
    if coll.is_nil() {
        return Value::FALSE;
    }
    if crate::protocol::satisfies(&IPersistentSet::MARKER, coll) {
        return clojure_rt_macros::dispatch!(ISet::contains, &[coll, k]);
    }
    clojure_rt_macros::dispatch!(IAssociative::contains_key, &[coll, k])
}

#[inline]
pub fn find(coll: Value, k: Value) -> Value {
    clojure_rt_macros::dispatch!(IAssociative::find, &[coll, k])
}

// --- Reversible -------------------------------------------------------------

#[inline]
pub fn rseq(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IReversible::rseq, &[coll])
}

// --- Chunked seqs -----------------------------------------------------------

#[inline]
pub fn chunked_first(s: Value) -> Value {
    clojure_rt_macros::dispatch!(IChunkedSeq::chunked_first, &[s])
}

#[inline]
pub fn chunked_rest(s: Value) -> Value {
    clojure_rt_macros::dispatch!(IChunkedSeq::chunked_rest, &[s])
}

#[inline]
pub fn chunked_next(s: Value) -> Value {
    clojure_rt_macros::dispatch!(IChunkedSeq::chunked_next, &[s])
}

// --- IChunk -----------------------------------------------------------------

#[inline]
pub fn drop_first(chunk: Value) -> Value {
    clojure_rt_macros::dispatch!(crate::protocols::chunk::IChunk::drop_first, &[chunk])
}

// --- Transients -------------------------------------------------------------

/// `(transient coll)` — produce a single-thread mutable view of `coll`.
#[inline]
pub fn transient(coll: Value) -> Value {
    clojure_rt_macros::dispatch!(IEditableCollection::as_transient, &[coll])
}

/// `(persistent! t)` — freeze a transient back into a persistent
/// collection. The transient becomes invalid for further mutation.
#[inline]
pub fn persistent_(t: Value) -> Value {
    clojure_rt_macros::dispatch!(ITransientCollection::persistent_bang, &[t])
}

/// `(conj! t x)` — mutate-add `x` to a transient.
#[inline]
pub fn conj_bang(t: Value, x: Value) -> Value {
    clojure_rt_macros::dispatch!(ITransientCollection::conj_bang, &[t, x])
}

/// `(assoc! t k v)` — mutate-set `k` to `v` on a transient (vector
/// or map).
#[inline]
pub fn assoc_bang(t: Value, k: Value, v: Value) -> Value {
    clojure_rt_macros::dispatch!(ITransientAssociative::assoc_bang, &[t, k, v])
}

/// `(dissoc! t k)` — mutate-remove `k` from a transient map.
#[inline]
pub fn dissoc_bang(t: Value, k: Value) -> Value {
    clojure_rt_macros::dispatch!(ITransientMap::dissoc_bang, &[t, k])
}

/// `(pop! t)` — mutate-pop the rightmost element from a transient
/// vector.
#[inline]
pub fn pop_bang(t: Value) -> Value {
    clojure_rt_macros::dispatch!(ITransientVector::pop_bang, &[t])
}

// --- Map ops ----------------------------------------------------------------

/// `(dissoc m k)` — return `m` without the entry for `k`.
#[inline]
pub fn dissoc(coll: Value, k: Value) -> Value {
    clojure_rt_macros::dispatch!(IMap::dissoc, &[coll, k])
}

/// `(key e)` — first half of a map entry.
#[inline]
pub fn key(e: Value) -> Value {
    clojure_rt_macros::dispatch!(IMapEntry::key, &[e])
}

/// `(val e)` — second half of a map entry.
#[inline]
pub fn val(e: Value) -> Value {
    clojure_rt_macros::dispatch!(IMapEntry::val, &[e])
}

// --- List + vector constructors --------------------------------------------

/// Build a `PersistentList` from a slice of `Value`s.
#[inline]
pub fn list(items: &[Value]) -> Value {
    PersistentList::list(items)
}

/// Build a `PersistentVector` from a slice of `Value`s. Each element's
/// refcount is bumped once for the new vector's storage.
#[inline]
pub fn vector(items: &[Value]) -> Value {
    PersistentVector::from_slice(items)
}

/// Build a `PersistentArrayMap` from a flat `[k0, v0, k1, v1, …]`
/// slice. Caller's elements are dup'd into the new map's storage.
#[inline]
pub fn array_map(kvs: &[Value]) -> Value {
    PersistentArrayMap::from_kvs(kvs)
}

/// Build a `PersistentHashSet` from a slice of items. Borrow semantics:
/// caller's elements are dup'd into the set's storage. Duplicate items
/// (under `IEquiv`) collapse.
#[inline]
pub fn hash_set(items: &[Value]) -> Value {
    PersistentHashSet::from_items(items)
}

/// `(disj s k)` — return `s` without the element `k`.
#[inline]
pub fn disj(s: Value, k: Value) -> Value {
    clojure_rt_macros::dispatch!(ISet::disjoin, &[s, k])
}

// --- Atoms ------------------------------------------------------------------

/// `(atom x)` — wrap `x` in a fresh `Atom`. Borrow semantics on `x`.
#[inline]
pub fn atom(x: Value) -> Value {
    Atom::new(x)
}

/// `(reset! a x)` — install `x` as the atom's value, returning `x`.
#[inline]
pub fn reset_bang(a: Value, x: Value) -> Value {
    clojure_rt_macros::dispatch!(IAtom::reset, &[a, x])
}

/// `(compare-and-set! a old new)` — succeed iff the atom currently
/// holds a value equiv to `old`. Returns `Value::TRUE` / `Value::FALSE`.
#[inline]
pub fn compare_and_set(a: Value, old: Value, new: Value) -> Value {
    clojure_rt_macros::dispatch!(IAtom::compare_and_set, &[a, old, new])
}

/// `(swap! a f args…)` — atomically replace the atom's value with
/// `(apply f current args)` under CAS retry. Caps at 4 user args (0..=3
/// in `args`); extending follows IFn's pattern (one extra arity).
#[inline]
pub fn swap_bang(a: Value, f: Value, args: &[Value]) -> Value {
    match args.len() {
        0 => clojure_rt_macros::dispatch!(IAtom::swap, &[a, f]),
        1 => clojure_rt_macros::dispatch!(IAtom::swap, &[a, f, args[0]]),
        2 => clojure_rt_macros::dispatch!(IAtom::swap, &[a, f, args[0], args[1]]),
        3 => clojure_rt_macros::dispatch!(IAtom::swap, &[a, f, args[0], args[1], args[2]]),
        n => panic!(
            "rt::swap_bang: arity {} exceeds current IAtom::swap cap of 3 user args — extend protocols/atom.rs",
            n
        ),
    }
}

// --- IRef (validators) ------------------------------------------------------

/// `(set-validator! a f)` — install `f` as `a`'s validator (or `nil`
/// to clear). Throws if the current value would fail the new fn.
#[inline]
pub fn set_validator_bang(a: Value, f: Value) -> Value {
    clojure_rt_macros::dispatch!(IRef::set_validator, &[a, f])
}

/// `(get-validator a)` — return the current validator (or `nil`).
#[inline]
pub fn get_validator(a: Value) -> Value {
    clojure_rt_macros::dispatch!(IRef::get_validator, &[a])
}

// --- IWatchable -------------------------------------------------------------

/// `(add-watch a key f)` — install `f` as the watch under `key`.
/// `f` is called with `(key a old new)` after every successful
/// value transition.
#[inline]
pub fn add_watch(a: Value, key: Value, f: Value) -> Value {
    clojure_rt_macros::dispatch!(IWatchable::add_watch, &[a, key, f])
}

/// `(remove-watch a key)` — uninstall the watch for `key`.
#[inline]
pub fn remove_watch(a: Value, key: Value) -> Value {
    clojure_rt_macros::dispatch!(IWatchable::remove_watch, &[a, key])
}

/// Fire watches for the `(old, new)` transition. Internal helper for
/// custom reference-type implementors; built-in `Atom` calls this
/// from its commit paths automatically.
#[inline]
pub fn notify_watches(a: Value, old: Value, new: Value) -> Value {
    clojure_rt_macros::dispatch!(IWatchable::notify_watches, &[a, old, new])
}

// --- IReference (mutable meta) ---------------------------------------------

/// `(reset-meta! a m)` — overwrite `a`'s meta with `m`. Returns `m`.
#[inline]
pub fn reset_meta_bang(a: Value, m: Value) -> Value {
    clojure_rt_macros::dispatch!(IReference::reset_meta, &[a, m])
}

/// `(alter-meta! a f args…)` — atomically apply `f` to the current
/// meta plus `args`, install the result, return the new meta. Caps
/// at 3 user args mirroring `swap_bang`.
#[inline]
pub fn alter_meta_bang(a: Value, f: Value, args: &[Value]) -> Value {
    match args.len() {
        0 => clojure_rt_macros::dispatch!(IReference::alter_meta, &[a, f]),
        1 => clojure_rt_macros::dispatch!(IReference::alter_meta, &[a, f, args[0]]),
        2 => clojure_rt_macros::dispatch!(IReference::alter_meta, &[a, f, args[0], args[1]]),
        3 => clojure_rt_macros::dispatch!(IReference::alter_meta, &[a, f, args[0], args[1], args[2]]),
        n => panic!(
            "rt::alter_meta_bang: arity {} exceeds current IReference::alter_meta cap of 3 user args — extend protocols/reference.rs",
            n
        ),
    }
}

// --- Volatiles --------------------------------------------------------------

/// `(volatile! x)` — wrap `x` in a fresh single-thread mutable cell.
#[inline]
pub fn volatile(x: Value) -> Value {
    Volatile::new(x)
}

/// `(vreset! v x)` — overwrite the volatile's value, return `x`.
#[inline]
pub fn vreset_bang(v: Value, x: Value) -> Value {
    clojure_rt_macros::dispatch!(IVolatile::reset, &[v, x])
}

/// `(vswap! v f args…)` — read, apply `f` to the current value plus
/// `args`, write the result, return the new value. Single-threaded
/// by contract — no CAS retry; caller is responsible for not
/// concurrently mutating.
pub fn vswap_bang(v: Value, f: Value, args: &[Value]) -> Value {
    let cur = deref(v);
    let mut call_args: Vec<Value> = Vec::with_capacity(1 + args.len());
    call_args.push(cur);
    call_args.extend_from_slice(args);
    let new_v = invoke(f, &call_args);
    crate::rc::drop_value(cur);
    if new_v.is_exception() {
        return new_v;
    }
    let r = vreset_bang(v, new_v);
    crate::rc::drop_value(new_v);
    r
}

// --- Delays / IPending ------------------------------------------------------

/// `(delay & body)` — wrap a thunk in a memoizing one-shot cell.
/// The thunk runs on first `force`/`deref`; subsequent reads return
/// the cached result.
#[inline]
pub fn delay(thunk: Box<dyn Fn() -> Value + Send + Sync>) -> Value {
    Delay::from_fn(thunk)
}

/// `(force d)` — realize a delay (no-op for non-delays in JVM, but
/// here we route through `IDeref` which is the standard read path).
#[inline]
pub fn force(d: Value) -> Value {
    deref(d)
}

/// `(realized? x)` — has this deferred value been computed yet?
#[inline]
pub fn is_realized(x: Value) -> Value {
    clojure_rt_macros::dispatch!(IPending::is_realized, &[x])
}

// --- Vars -------------------------------------------------------------------

/// `(intern ns sym root)` — build a fresh `Var`. Borrow semantics
/// on all three args; `ns` and/or `sym` may be `Value::NIL` for an
/// anonymous var.
#[inline]
pub fn intern_var(ns: Value, sym: Value, root: Value) -> Value {
    Var::intern(ns, sym, root)
}

/// `(.setDynamic v)` — flip `v`'s dynamic flag so thread bindings
/// are honored on `deref`. Returns `v`.
#[inline]
pub fn set_dynamic(v: Value) -> Value {
    Var::set_dynamic(v)
}

/// `(.bindRoot v new)` — install a new root, firing watches.
/// Returns nil on success, an exception value on validator failure.
#[inline]
pub fn bind_root(v: Value, new_root: Value) -> Value {
    Var::bind_root(v, new_root)
}

/// `(alter-var-root v f args…)` — atomically apply `f` to the
/// current root + args, install the result, return the new root.
/// CAS retry under contention. Caps at 3 user args.
#[inline]
pub fn alter_var_root(v: Value, f: Value, args: &[Value]) -> Value {
    Var::alter_root(v, f, args)
}

/// `(push-thread-bindings m)` — push a new thread-binding frame
/// merging `m` (a map of `Var → value`) onto the previous frame.
#[inline]
pub fn push_thread_bindings(m: Value) {
    crate::types::var::push_thread_bindings(m)
}

/// `(pop-thread-bindings)` — restore the previous frame.
#[inline]
pub fn pop_thread_bindings() {
    crate::types::var::pop_thread_bindings()
}

/// `(cons x coll)`. Returns a `PersistentList` when `coll` is nil or
/// already a list (preserves count tracking); otherwise returns a
/// `Cons` cell wrapping `(seq coll)` (works for lazy/infinite seqs).
/// Borrow semantics on both arguments.
#[inline]
pub fn cons(x: Value, coll: Value) -> Value {
    if coll.is_nil() {
        return crate::types::list::PersistentList::list(&[x]);
    }
    let list_id = *crate::types::list::PERSISTENTLIST_TYPE_ID.get().unwrap_or(&0);
    let empty_list_id = *crate::types::list::EMPTYLIST_TYPE_ID.get().unwrap_or(&0);
    if coll.tag == list_id || coll.tag == empty_list_id {
        return PersistentList::cons(x, coll);
    }
    // General seqable: build a Cons cell over (seq coll).
    let s = seq(coll);
    if s.is_nil() {
        return crate::types::list::PersistentList::list(&[x]);
    }
    let result = crate::types::cons::Cons::new(x, s);
    crate::rc::drop_value(s);
    result
}

/// Build a `LazySeq` from a Rust closure. The closure runs at most
/// once per `LazySeq`; subsequent accesses use the cached result.
#[inline]
pub fn lazy_seq(thunk: Box<dyn Fn() -> Value + Send + Sync>) -> Value {
    crate::types::lazy_seq::LazySeq::from_fn(thunk)
}

/// `(sequential? x)` — does `x`'s type marker-implement `ISequential`?
#[inline]
pub fn sequential(v: Value) -> bool {
    crate::protocol::satisfies(&ISequential::MARKER, v)
}

// --- Reduce ----------------------------------------------------------------

/// `(reduce f coll)` — three dispatches, in order:
///
/// 1. If `coll` directly implements `IReduce`, call its `reduce_2`.
/// 2. Else, get `(seq coll)`. If the seq implements `IChunkedSeq`,
///    walk it chunk-by-chunk.
/// 3. Else, walk it element-by-element via `first`/`next`.
///
/// `Reduced` and exception Values short-circuit at every step.
pub fn reduce(coll: Value, f: Value) -> Value {
    if crate::protocol::satisfies(&IReduce::REDUCE_2, coll) {
        return clojure_rt_macros::dispatch!(IReduce::reduce, &[coll, f]);
    }
    let s = seq(coll);
    if s.is_nil() {
        crate::rc::drop_value(s);
        return clojure_rt_macros::dispatch!(IFn::invoke, &[f]);
    }
    let first_v = first(s);
    let rest_seq = next(s);
    crate::rc::drop_value(s);
    let r = reduce_seq_impl(rest_seq, f, first_v);
    r
}

/// `(reduce f init coll)` — same dispatch shape as 2-arity reduce
/// but with a caller-supplied seed.
pub fn reduce_init(coll: Value, f: Value, init: Value) -> Value {
    if crate::protocol::satisfies(&IReduce::REDUCE_3, coll) {
        return clojure_rt_macros::dispatch!(IReduce::reduce, &[coll, f, init]);
    }
    let s = seq(coll);
    reduce_seq_impl(s, f, init)
}

/// Walk a seq applying `f` to `acc` and each element. The seq is
/// consumed (this fn drops the entry-point seq Value when it
/// finishes). `acc` is consumed too — caller transferred one ref.
fn reduce_seq_impl(mut s: Value, f: Value, mut acc: Value) -> Value {
    while !s.is_nil() {
        if crate::protocol::satisfies(&IChunkedSeq::CHUNKED_FIRST_1, s) {
            let chunk = chunked_first(s);
            let cnt = count(chunk).as_int().expect("ICounted on chunk returns int");
            let mut i: i64 = 0;
            while i < cnt {
                let x = nth(chunk, Value::int(i));
                let new_acc = invoke(f, &[acc, x]);
                crate::rc::drop_value(acc);
                crate::rc::drop_value(x);
                acc = new_acc;
                if is_reduced(acc) {
                    crate::rc::drop_value(chunk);
                    crate::rc::drop_value(s);
                    return unreduced(acc);
                }
                if acc.is_exception() {
                    crate::rc::drop_value(chunk);
                    crate::rc::drop_value(s);
                    return acc;
                }
                i += 1;
            }
            crate::rc::drop_value(chunk);
            let next_s = chunked_next(s);
            crate::rc::drop_value(s);
            s = next_s;
            continue;
        }
        // Element-by-element fallback.
        let x = first(s);
        let new_acc = invoke(f, &[acc, x]);
        crate::rc::drop_value(acc);
        crate::rc::drop_value(x);
        acc = new_acc;
        if is_reduced(acc) {
            crate::rc::drop_value(s);
            return unreduced(acc);
        }
        if acc.is_exception() {
            crate::rc::drop_value(s);
            return acc;
        }
        let next_s = next(s);
        crate::rc::drop_value(s);
        s = next_s;
    }
    crate::rc::drop_value(s);
    acc
}

// --- Reduced + IDeref -------------------------------------------------------

/// Wrap `x` as a `Reduced` sentinel so a step function can short-
/// circuit a reduce. Caller transfers one ref of `x` to the wrapper.
#[inline]
pub fn reduced(x: Value) -> Value {
    Reduced::wrap(x)
}

/// `(reduced? x)` — true iff `x` is a `Reduced` sentinel.
#[inline]
pub fn is_reduced(x: Value) -> bool {
    if !x.is_heap() {
        return false;
    }
    Reduced::type_id() == x.tag
}

/// `(unreduced x)` — if reduced, deref the wrapper; else identity.
/// In the dereferenced case, transfers a fresh ref to the caller and
/// drops the wrapper's ref.
#[inline]
pub fn unreduced(x: Value) -> Value {
    if is_reduced(x) {
        let inner = deref(x);
        crate::rc::drop_value(x);
        inner
    } else {
        x
    }
}

#[inline]
pub fn deref(v: Value) -> Value {
    clojure_rt_macros::dispatch!(IDeref::deref, &[v])
}

// --- IFn invocation ---------------------------------------------------------

/// `(f a₁ … aₙ)` — invoke `f` with `args.len()` user-visible
/// arguments. Routes through `IFn::invoke_<args.len()+1>` (the +1 is
/// the receiver `f` prepended to the slice). Currently caps at 5
/// user args; longer arities will land via `apply_to` once we have
/// it. Panics for now if the arity exceeds the cap so the gap is
/// loud rather than silent.
#[inline]
pub fn invoke(f: Value, args: &[Value]) -> Value {
    match args.len() {
        0 => clojure_rt_macros::dispatch!(IFn::invoke, &[f]),
        1 => clojure_rt_macros::dispatch!(IFn::invoke, &[f, args[0]]),
        2 => clojure_rt_macros::dispatch!(IFn::invoke, &[f, args[0], args[1]]),
        3 => clojure_rt_macros::dispatch!(IFn::invoke, &[f, args[0], args[1], args[2]]),
        4 => clojure_rt_macros::dispatch!(IFn::invoke, &[f, args[0], args[1], args[2], args[3]]),
        5 => clojure_rt_macros::dispatch!(IFn::invoke, &[f, args[0], args[1], args[2], args[3], args[4]]),
        n => panic!(
            "rt::invoke: arity {} exceeds current IFn cap of 5 — extend protocols/ifn.rs",
            n
        ),
    }
}
