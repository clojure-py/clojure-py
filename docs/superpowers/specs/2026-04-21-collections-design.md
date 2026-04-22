# Persistent Collections — Design Spec

**Date:** 2026-04-21
**Status:** Draft for implementation
**Scope:** Second sub-project of the clojure-py revival, following core-abstractions. Delivers the main-line Clojure persistent collections — list, vector, array-map, hash-map, hash-set — plus the seq types (`Cons`, `LazySeq`, `ChunkedCons`/`ChunkedSeq`, `IteratorSeq`) and transient variants. Everything is protocol-routed through `rt::*`.

---

## 1. Goal

Deliver the persistent-collections layer that the rest of the Clojure runtime depends on. After this spec, any user or downstream subsystem (reader, evaluator, core.clj bootstrap) can produce and consume `[1 2 3]`, `(1 2 3)`, `{:a 1}`, `#{:a}`, and build big collections efficiently with transients.

**Core design tenets:**

1. **All equality, hashing, and iteration goes through protocols.** `IEquiv` / `IHashEq` / `ISeq` / `Counted` — never raw Rust `Eq`/`Hash`, never direct Python `==`/`__hash__` at HAMT leaves. The protocol system is the router; Python-type handling is the built-in fallback on each protocol.
2. **HAMT + transients are hand-rolled** in Rust, ported from Clojure-JVM's `PersistentHashMap.java` / `PersistentVector.java` / `PersistentHashSet.java`. No off-the-shelf crate — the protocol-routing requirement makes standard-trait-based containers unusable.
3. **Structural sharing via `Arc`** on every internal node. `assoc`/`dissoc` path-copy only the O(log₃₂ n) nodes on the mutation path.
4. **Transients match Clojure's safety semantics:** `alive: AtomicBool` + owner-thread check. `persistent_bang` flips `alive`; further ops raise `IllegalStateException`. Cross-thread misuse also raises, rather than corrupting.
5. **Python-facing pyclasses for everything the user can hold** — persistent collections, transients, seq types. Each exposes Python dunders (`__iter__`, `__len__`, `__eq__`, `__hash__`, `__getitem__`, `__contains__`, `__call__` where applicable) that delegate to `rt::*`.

Explicitly **not** in scope: `PersistentTreeMap`/`PersistentTreeSet` (sorted via red-black tree — rarely used, separate spec), `PersistentQueue` (peripheral), reducers (`IReduce`/`IReduceInit`), exotic seq producers (`Cycle`/`Iterate`/`Repeat`), the `Sorted` protocol, cross-type sequential equality (will be added with reader/printer spec), Clojure-JVM-compatible hash codes beyond what follows from `hash_eq` structural combination, `print-method` multimethod.

---

## 2. Architecture

### 2.1 File layout

```
crates/clojure_core/src/
  collections/
    mod.rs                     # Re-exports + registry init for all collection pyclasses
    plist.rs                   # PersistentList + EmptyList singleton
    pvector.rs                 # PersistentVector (HAMT + tail) + TransientVector
    pvector_node.rs            # Internal vector HAMT nodes
    parraymap.rs               # PersistentArrayMap + TransientArrayMap (auto-promotes)
    phashmap.rs                # PersistentHashMap (HAMT) + TransientHashMap
    phashmap_node.rs           # Internal map HAMT nodes (Bitmap, Array, Collision)
    phashset.rs                # PersistentHashSet + TransientHashSet (wraps phashmap)
    map_entry.rs               # MapEntry — 2-field struct for map iteration
  seqs/
    mod.rs                     # Re-exports + registry
    cons.rs                    # Cons (basic non-lazy cell)
    lazy_seq.rs                # LazySeq (thunk-cached)
    chunked_cons.rs            # ChunkedCons + ChunkedSeq (32-wide for vector)
    iterator_seq.rs            # Wraps Python iterators
  iseq.rs                      # ISeq protocol
  iseqable.rs                  # ISeqable
  counted.rs                   # Counted
  iequiv.rs                    # IEquiv
  ihasheq.rs                   # IHashEq
  imeta.rs                     # IMeta (merged with IObj — we don't split read-only meta)
  sequential.rs                # Sequential (marker)
  indexed.rs                   # Indexed
  associative.rs               # Associative
  ipersistent_collection.rs    # IPersistentCollection
  ipersistent_list.rs          # IPersistentList (marker)
  ipersistent_vector.rs        # IPersistentVector
  ipersistent_map.rs           # IPersistentMap
  ipersistent_set.rs           # IPersistentSet
  ipersistent_stack.rs         # IPersistentStack (peek/pop)
  ieditable_collection.rs      # IEditableCollection (as_transient)
  itransient_collection.rs     # ITransientCollection (conj!, persistent!)
  itransient_associative.rs    # ITransientAssociative (assoc!)
  itransient_vector.rs         # ITransientVector (pop!)
  itransient_map.rs            # ITransientMap (dissoc!)
  itransient_set.rs            # ITransientSet (disj!, contains!, get!)

  rt.rs                        # EXTENDED with: equiv, hash_eq, seq, first, next, rest,
                               # count, conj, assoc, dissoc, nth, contains, empty, transient,
                               # persistent_bang, conj_bang, assoc_bang, dissoc_bang,
                               # disj_bang, pop_bang

  binding_pmap.rs              # RENAMED from pmap.rs (the existing internal binding-frame map)
```

**Renames:** the existing internal `src/pmap.rs` becomes `src/binding_pmap.rs`. It was always internal (no Python exposure), so the rename is mechanical. Its callers (`binding.rs`, `bound_fn.rs`) update to the new path.

### 2.2 Module dependencies

```
exceptions ─────────────────────────────────────────┐
                                                    ▼
protocols (iseq, iseqable, counted, iequiv, ihasheq, imeta, etc.)
    │
    │ (each protocol declared via #[protocol] — registers at module init)
    ▼
rt (extended helpers — route through protocols)
    ▲           ▲
    │           │
    │           └── seqs/ (cons, lazy_seq, chunked, iterator_seq)
    │
collections/ (plist, pvector, parraymap, phashmap, phashset)
    │
    │ (each collection #[implements] its protocols + has Python dunders that delegate to rt)
    ▼
transients (in each collection's file) — implement ITransient* protocols
```

The protocols are declared first so `#[implements]` on the collection types resolves them. `rt` depends on the protocols being registered. Module init order in `lib.rs` adds the protocol modules, then registry, then rt::init (which caches protocol Py handles), then collections, then seqs.

---

## 3. Protocol Surface

### 3.1 Seq layer

**`ISeq`** — Clojure's seq abstraction.

```rust
#[protocol(name = "clojure.core/ISeq", extend_via_metadata = false)]
pub trait ISeq {
    fn first(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn next(&self, py: Python<'_>) -> PyResult<PyObject>;   // rest-or-nil
    fn more(&self, py: Python<'_>) -> PyResult<PyObject>;   // rest-or-empty
    fn cons(&self, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;  // prepend — `cons` because `cons` is a keyword in Rust 2024
}
```

**`ISeqable`** — anything that can produce an `ISeq`.

```rust
#[protocol(name = "clojure.core/ISeqable", extend_via_metadata = false)]
pub trait ISeqable {
    fn seq(&self, py: Python<'_>) -> PyResult<PyObject>;  // returns ISeq or nil
}
```

**`Counted`** — O(1) count.

```rust
#[protocol(name = "clojure.core/Counted", extend_via_metadata = false)]
pub trait Counted {
    fn count(&self, py: Python<'_>) -> PyResult<usize>;
}
```

### 3.2 Collection algebra

**`IPersistentCollection`** — base.

```rust
#[protocol(name = "clojure.core/IPersistentCollection", extend_via_metadata = false)]
pub trait IPersistentCollection {
    fn count(&self, py: Python<'_>) -> PyResult<usize>;
    fn cons(&self, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
    fn empty(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn equiv(&self, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}
```

**`IPersistentList`** — marker protocol. No methods.

**`IPersistentVector`**:

```rust
#[protocol(name = "clojure.core/IPersistentVector", extend_via_metadata = false)]
pub trait IPersistentVector {
    fn length(&self, py: Python<'_>) -> PyResult<usize>;
    fn assoc_n(&self, py: Python<'_>, i: usize, x: PyObject) -> PyResult<PyObject>;
}
```

**`IPersistentMap`**:

```rust
#[protocol(name = "clojure.core/IPersistentMap", extend_via_metadata = false)]
pub trait IPersistentMap {
    fn assoc(&self, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
    fn without(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_key(&self, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;  // MapEntry or nil
}
```

**`IPersistentSet`**:

```rust
#[protocol(name = "clojure.core/IPersistentSet", extend_via_metadata = false)]
pub trait IPersistentSet {
    fn disjoin(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains(&self, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;  // the value if present, else nil
}
```

**`IPersistentStack`** — peek/pop.

```rust
#[protocol(name = "clojure.core/IPersistentStack", extend_via_metadata = false)]
pub trait IPersistentStack {
    fn peek(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn pop(&self, py: Python<'_>) -> PyResult<PyObject>;
}
```

(PersistentList peeks/pops from the head; PersistentVector peeks/pops from the tail.)

**`Indexed`** — O(1) nth.

```rust
#[protocol(name = "clojure.core/Indexed", extend_via_metadata = false)]
pub trait Indexed {
    fn nth(&self, py: Python<'_>, i: usize) -> PyResult<PyObject>;
    fn nth_or_default(&self, py: Python<'_>, i: usize, default: PyObject) -> PyResult<PyObject>;
}
```

**`Associative`** — maps + vectors.

```rust
#[protocol(name = "clojure.core/Associative", extend_via_metadata = false)]
pub trait Associative {
    fn contains_key(&self, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn assoc(&self, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
}
```

### 3.3 Equality / hash / meta

**`IEquiv`** — Clojure `=` semantics.

```rust
#[protocol(name = "clojure.core/IEquiv", extend_via_metadata = false)]
pub trait IEquiv {
    fn equiv(&self, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}
```

**Built-in fallback (registered at module init):** for a target without an explicit impl, fall back to Python `a == b`. This makes `(rt::equiv 1 1)` work on integers without us extending `IEquiv` to `int`.

**`IHashEq`** — Clojure-compatible structural hash.

```rust
#[protocol(name = "clojure.core/IHashEq", extend_via_metadata = false)]
pub trait IHashEq {
    fn hash_eq(&self, py: Python<'_>) -> PyResult<i64>;
}
```

**Built-in fallback:** Python `hash(obj)` returns an `isize`; we fold to `i64`. Our own types override with structural hashes.

Elements inside a HAMT hash via `rt::hash_eq`, which goes through `IHashEq`. For Keywords this hits their precomputed `hash_cache`; for plain Python integers, the fallback returns `hash(n)`; for another nested `PersistentVector`, our `IHashEq` impl folds element hashes.

**`IMeta`** — metadata. Merged with Clojure's `IObj` (we don't split read-only vs writable meta).

```rust
#[protocol(name = "clojure.core/IMeta", extend_via_metadata = false)]
pub trait IMeta {
    fn meta(&self, py: Python<'_>) -> PyResult<PyObject>;  // Clojure map or nil
    fn with_meta(&self, py: Python<'_>, meta: PyObject) -> PyResult<PyObject>;
}
```

### 3.4 Marker protocols

- **`Sequential`** — `PersistentList`, `PersistentVector`, `Cons`, `LazySeq`, `ChunkedCons`/`ChunkedSeq`, `IteratorSeq` all satisfy this. `PersistentHashMap`/`PersistentArrayMap`/`PersistentHashSet` do not. Used by (future) cross-type equality logic.

### 3.5 Transient protocols

**`IEditableCollection`** — go persistent → transient.

```rust
#[protocol(name = "clojure.core/IEditableCollection", extend_via_metadata = false)]
pub trait IEditableCollection {
    fn as_transient(&self, py: Python<'_>) -> PyResult<PyObject>;
}
```

**`ITransientCollection`** — base.

```rust
#[protocol(name = "clojure.core/ITransientCollection", extend_via_metadata = false)]
pub trait ITransientCollection {
    fn conj_bang(&self, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
    fn persistent_bang(&self, py: Python<'_>) -> PyResult<PyObject>;
}
```

**`ITransientAssociative`** — extends `ITransientCollection`.

```rust
#[protocol(name = "clojure.core/ITransientAssociative", extend_via_metadata = false)]
pub trait ITransientAssociative {
    fn assoc_bang(&self, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
}
```

**`ITransientVector`** — extends `ITransientAssociative`.

```rust
#[protocol(name = "clojure.core/ITransientVector", extend_via_metadata = false)]
pub trait ITransientVector {
    fn pop_bang(&self, py: Python<'_>) -> PyResult<PyObject>;
}
```

**`ITransientMap`** — extends `ITransientAssociative`.

```rust
#[protocol(name = "clojure.core/ITransientMap", extend_via_metadata = false)]
pub trait ITransientMap {
    fn dissoc_bang(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
```

**`ITransientSet`** — extends `ITransientCollection`.

```rust
#[protocol(name = "clojure.core/ITransientSet", extend_via_metadata = false)]
pub trait ITransientSet {
    fn disj_bang(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_bang(&self, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get_bang(&self, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
```

**Note on "protocol extension"**: the Clojure-JVM `interface B extends A` relation isn't directly modeled by our protocols — each is flat. What matters is that a type implements all the protocols whose methods it supports (e.g., `TransientVector` implements `ITransientCollection`, `ITransientAssociative`, and `ITransientVector`). Dispatch is per-protocol; there's no hierarchy lookup.

### 3.6 rt helpers

Extend `rt.rs` with these. All protocol-routed. All cache their `Py<Protocol>` in a `OnceCell` populated by `rt::init`.

| Helper | Protocol dispatched | Notes |
|--------|---------------------|-------|
| `rt::equiv(a, b)` | IEquiv | Built-in fallback: Python `a == b` |
| `rt::hash_eq(x)` | IHashEq | Built-in fallback: Python `hash(x)` |
| `rt::seq(coll)` | ISeqable | Returns nil-or-ISeq |
| `rt::first(coll)` | ISeq (via rt::seq if needed) | |
| `rt::next(coll)` | ISeq | |
| `rt::rest(coll)` | ISeq (`more`) | |
| `rt::count(coll)` | Counted, fallback to iterate | |
| `rt::conj(coll, x)` | IPersistentCollection.cons | |
| `rt::assoc(coll, k, v)` | Associative.assoc | |
| `rt::dissoc(map, k)` | IPersistentMap.without | |
| `rt::nth(coll, i)` | Indexed.nth | |
| `rt::contains(coll, k)` | Associative.contains_key (or IPersistentSet.contains for sets) | |
| `rt::empty(coll)` | IPersistentCollection.empty | |
| `rt::transient(coll)` | IEditableCollection.as_transient | |
| `rt::persistent_bang(t)` | ITransientCollection.persistent_bang | |
| `rt::conj_bang(t, x)` | ITransientCollection.conj_bang | |
| `rt::assoc_bang(t, k, v)` | ITransientAssociative.assoc_bang | |
| `rt::dissoc_bang(t, k)` | ITransientMap.dissoc_bang | |
| `rt::disj_bang(t, k)` | ITransientSet.disj_bang | |
| `rt::pop_bang(t)` | ITransientVector.pop_bang | |

---

## 4. Collection Types

Each section states: storage shape, protocols implemented, Python dunders, notable operations, deferred bits.

### 4.1 PersistentList

```rust
#[pyclass(module = "clojure._core", name = "PersistentList", frozen)]
pub struct PersistentList {
    head: PyObject,
    tail: Py<PyAny>,              // another PersistentList or EmptyList
    count: u32,                    // O(1)
    meta: RwLock<Option<PyObject>>,
}

#[pyclass(module = "clojure._core", name = "EmptyList", frozen)]
pub struct EmptyList {
    meta: RwLock<Option<PyObject>>,
}
```

A module-init-time singleton `EMPTY_LIST` is constructed once and reused — callers hold `Py<EmptyList>` clones. `(seq ())` returns `nil`; `(seq '(1))` returns the list itself.

**Protocols:** `ISeq`, `ISeqable`, `Counted`, `IEquiv`, `IHashEq`, `IMeta`, `IPersistentCollection`, `IPersistentList`, `IPersistentStack`, `Sequential`.

**Python dunders** (all go through `rt::*`): `__iter__`, `__len__`, `__eq__`, `__hash__`, `__bool__` (empty = False).

**Not included:** `IFn` (lists aren't callable as fns in Clojure; they're data). `rseq` (reverse iteration — requires a separate type).

### 4.2 PersistentVector

Hand-rolled HAMT with Clojure's structure: `root` (trie) + `tail` (flat 32-element array) + `cnt`/`shift`/`meta`.

```rust
#[pyclass(module = "clojure._core", name = "PersistentVector", frozen)]
pub struct PersistentVector {
    cnt: u32,
    shift: u32,                    // bits to shift off root hash for top-level index
    root: Arc<VNode>,              // VNode is the internal node enum (pvector_node.rs)
    tail: Arc<[PyObject]>,         // 1..=32 elements; the in-progress appending area
    meta: RwLock<Option<PyObject>>,
}
```

**Append semantics:** `conj(v, x)` either extends tail (if tail.len() < 32) or pushes tail into trie and starts new tail. `assoc_n(v, i, x)` path-copies from root.

**Protocols:** `ISeq` (via chunked seq), `ISeqable`, `Counted`, `IEquiv`, `IHashEq`, `IMeta`, `IPersistentCollection`, `IPersistentVector`, `IPersistentStack`, `Associative`, `Indexed`, `Sequential`, `IEditableCollection` (to go transient), `IFn` (invoke with index — `(v 0)` = nth).

**Python dunders:** `__iter__`, `__len__`, `__eq__`, `__hash__`, `__getitem__`, `__contains__`, `__call__`.

**Deferred:** `subvec`, `rseq`, `reduce` via Transient.

### 4.3 PersistentArrayMap

Flat array storage. Efficient for small maps (≤8 entries). Auto-promotes to `PersistentHashMap` on `assoc` when size would exceed threshold.

```rust
#[pyclass(module = "clojure._core", name = "PersistentArrayMap", frozen)]
pub struct PersistentArrayMap {
    entries: Arc<[(PyObject, PyObject)]>,   // alternating K V... or [(K,V)] pairs
    meta: RwLock<Option<PyObject>>,
}

const HASHMAP_THRESHOLD: usize = 8;
```

`assoc(m, k, v)` scans entries linearly (using `rt::equiv`). If key present, replaces. If absent and size < threshold, extends. If absent and size == threshold, builds a `PersistentHashMap` from the entries and the new pair, returns that.

`val_at(m, k)` scans linearly with `rt::equiv`.

**Protocols:** `ISeqable` (yields MapEntries), `Counted`, `IEquiv`, `IHashEq`, `IMeta`, `IPersistentCollection`, `IPersistentMap`, `Associative`, `ILookup`, `IFn`, `IEditableCollection`.

**Python dunders:** `__iter__` (yields keys, Python-dict-style), `__len__`, `__eq__`, `__hash__`, `__getitem__`, `__contains__`, `__call__`.

### 4.4 PersistentHashMap

32-way HAMT. See §5 for node details.

```rust
#[pyclass(module = "clojure._core", name = "PersistentHashMap", frozen)]
pub struct PersistentHashMap {
    count: u32,
    root: Option<Arc<MNode>>,      // None = empty map
    has_null: bool,                // whether nil-key is present
    null_value: Option<PyObject>,  // value for nil-key
    hash_cache: AtomicI64,         // lazy; 0 = uncomputed, else hash+1 (to distinguish from unset)
    meta: RwLock<Option<PyObject>>,
}
```

`nil`-as-key handled separately (matches Clojure) because `rt::hash_eq(nil)` is 0 and we'd otherwise conflict with hash-0 non-nil keys.

**Protocols:** same set as PersistentArrayMap, including `IPersistentMap`, `Associative`, `ILookup`, `IFn`, `IEditableCollection`.

### 4.5 PersistentHashSet

Thin wrapper. Internally a `PersistentHashMap` where each key maps to itself.

```rust
#[pyclass(module = "clojure._core", name = "PersistentHashSet", frozen)]
pub struct PersistentHashSet {
    impl_map: Py<PersistentHashMap>,
    meta: RwLock<Option<PyObject>>,
}
```

**Protocols:** `ISeqable` (yields values = keys), `Counted`, `IEquiv`, `IHashEq`, `IMeta`, `IPersistentCollection`, `IPersistentSet`, `IFn` (invoke with key → key-if-present-else-nil — Clojure's set-as-fn).

---

## 5. HAMT Internals

Port of `clojure/lang/PersistentHashMap.java` and `PersistentVector.java`. Same algorithms, adapted for Rust idioms (`Arc`, `AtomicUsize`) and our protocol-routed hash/eq.

### 5.1 HashMap nodes

```rust
pub enum MNode {
    Bitmap(BitmapIndexedNode),
    Array(ArrayNode),
    Collision(HashCollisionNode),
}

pub struct BitmapIndexedNode {
    bitmap: u32,                                        // 32 bits for 32 possible slots
    array: Arc<[MEntry]>,                               // densely packed; length = popcount(bitmap)
    edit: Option<Arc<AtomicUsize>>,                     // Some = transient-editable, None = persistent
}

pub struct ArrayNode {
    count: u32,                                         // non-empty slot count
    array: Arc<[Option<Arc<MNode>>; 32]>,               // dense
    edit: Option<Arc<AtomicUsize>>,
}

pub struct HashCollisionNode {
    hash: i32,                                          // the full (folded) hash all entries share
    entries: Arc<[(PyObject, PyObject)]>,
    edit: Option<Arc<AtomicUsize>>,
}

// An entry slot in a BitmapIndexedNode can be either a direct key/value pair
// or a child subtree. We encode: `Some(key)` = key/value leaf, `None` key
// = subtree. This matches Clojure's sentinel encoding.
pub enum MEntry {
    KV { key: PyObject, val: PyObject },
    Child(Arc<MNode>),
}
```

**5 bits of hash per level** (`1 << 5 == 32`). Root uses bits 0-4, level 1 uses bits 5-9, etc. Max depth for 32-bit hashes: 7 levels. If we fold `hash_eq`'s `i64` result to `i32` for indexing, the tree depth bound is the same as Clojure-JVM.

**Node operations:** `get`, `assoc` (returns new Arc), `without`, `find` (returns MapEntry), `iter`, `without_edit`/`assoc_edit`/`assoc_edit_in_place` for transients.

**Promotion:** `BitmapIndexedNode` with 16+ slots → `ArrayNode`. `ArrayNode` with ≤8 slots after a removal → `BitmapIndexedNode` (demotion).

### 5.2 Vector nodes

```rust
pub struct VNode {
    array: Arc<[Option<VSlot>; 32]>,
    edit: Option<Arc<AtomicUsize>>,
}

pub enum VSlot {
    Branch(Arc<VNode>),
    Leaf(PyObject),  // only at the deepest level
}
```

Simpler than map nodes — no collision handling, no bitmap (always 32 slots, possibly partially filled). The `PersistentVector` struct's `shift` field tracks the current root depth.

**Tail optimization:** the last 32 elements live in `PersistentVector.tail` (a flat `Arc<[PyObject]>`) rather than in the trie. `conj` on a vector with `tail.len() < 32` just extends the tail. When `tail` fills (at 32), it's pushed down into the trie as a new leaf, and `tail` resets.

### 5.3 Transient internals

A transient holds an `edit: Arc<AtomicUsize>` token. When a node is cloned during a transient operation, the clone carries `Some(edit.clone())`. On subsequent ops, a node whose `edit == transient.edit` may be mutated in place; otherwise it's cloned fresh.

`persistent_bang` atomically stores 0 in `*transient.edit`. This orphans all nodes that reference that edit — they stop being mutate-in-place candidates for any still-outstanding transient handle. Subsequent ops on the now-dead transient raise `IllegalStateException` (see §6).

### 5.4 Hash folding

`rt::hash_eq` returns `i64`. For trie indexing we need `i32`:

```rust
fn fold_hash(h: i64) -> i32 {
    ((h as u64 ^ ((h as u64) >> 32)) as u32) as i32
}
```

Clojure-JVM uses 32-bit hash codes natively; we fold 64→32 once at the `hash_eq` call site.

---

## 6. Transient Safety

Each transient struct carries:

```rust
pub struct TransientHashMap {
    // ... same fields as PersistentHashMap internally ...
    alive: AtomicBool,
    owner_thread: AtomicUsize,     // OS thread id; 0 = unset
}
```

**On construction (via `as_transient`):** `alive = true`, `owner_thread = current_thread_id()`.

**On every transient op:**

```rust
fn check_alive(&self) -> PyResult<()> {
    if !self.alive.load(Ordering::Acquire) {
        return Err(IllegalStateException::new_err(
            "Transient used after persistent! call"
        ));
    }
    let current = current_thread_id();
    let owner = self.owner_thread.load(Ordering::Acquire);
    if owner != 0 && owner != current {
        return Err(IllegalStateException::new_err(
            "Transient used by non-owner thread"
        ));
    }
    Ok(())
}
```

**On `persistent_bang`:** after producing the final persistent form, flip `alive = false`. The transient may not be used after this.

`current_thread_id()`: read `libc::pthread_self() as usize` on Linux/macOS, equivalent on Windows. Stored as `usize`.

---

## 7. Lazy Sequences & Chunked Seqs

### 7.1 LazySeq

```rust
#[pyclass(module = "clojure._core", name = "LazySeq", frozen)]
pub struct LazySeq {
    state: RwLock<LazySeqState>,
    meta: RwLock<Option<PyObject>>,
}

enum LazySeqState {
    Unrealized(PyObject),          // an IFn (via rt::invoke_n) returning a Cons/seq/nil
    Realized(Option<PyObject>),    // None = empty seq
}
```

Every seq op (`first`, `next`, `seq`) calls `realize()` which:

1. Read-lock; if realized, return cached.
2. Upgrade to write-lock; re-check; if another thread realized, return cached.
3. Call the thunk via `rt::invoke_n(py, thunk, &[])`. If result is a `LazySeq`, unwrap it (iterative tail-unwinding, to prevent stack overflow on deeply-nested lazy chains). Store result. Clear thunk to release any captured references.

Protocols: `ISeq`, `ISeqable`, `IMeta`, `Sequential`. No `Counted` — lazy seqs don't know their length until realized.

### 7.2 ChunkedCons and ChunkedSeq

Used by `PersistentVector.seq()` to let `reduce`/`doseq`/`map` iterate 32 elements at a time without going through one cons cell per element.

```rust
#[pyclass(module = "clojure._core", name = "ChunkedSeq", frozen)]
pub struct ChunkedSeq {
    vec: Py<PersistentVector>,
    node: Arc<[PyObject]>,         // current 32-element leaf
    i: u32,                        // offset in vec where `node` starts
    offset: u32,                   // offset within `node` (always 0 for ChunkedSeq, nonzero for drop-first)
    meta: RwLock<Option<PyObject>>,
}
```

Provides `chunked-first` (returns the current chunk), `chunked-next` (advances 32), and the normal `first`/`next` (one at a time). Internal protocol `IChunkedSeq` in a later spec; for now we expose chunked access via concrete method names on the pyclass and use them from `rt::*` helpers that know the chunked path.

### 7.3 Cons

Plain cons cell — strict, not lazy:

```rust
#[pyclass(module = "clojure._core", name = "Cons", frozen)]
pub struct Cons {
    first: PyObject,
    more: PyObject,              // another seq or nil
    meta: RwLock<Option<PyObject>>,
}
```

### 7.4 IteratorSeq

Wraps a Python iterator into an `ISeq`. Stateful (Python iterators are stateful), so this uses a `RwLock<IteratorSeqState>` to cache the first value and the tail. Effectively a lazy cons that reads one element at a time from the iterator.

```rust
enum IteratorSeqState {
    Fresh(PyObject),                       // iterator not yet called
    Realized { first: PyObject, rest: PyObject },  // got a value; rest is a new IteratorSeq
    Exhausted,                              // iterator returned StopIteration
}
```

---

## 8. Error Types

Reuse `IllegalStateException` (from core-abstractions) for:
- Stale transient (used after `persistent_bang`)
- Non-owner thread accessing transient
- Popping an empty vector or list

Reuse `IllegalArgumentException` for:
- Vector `nth` out-of-bounds when no default provided
- Odd number of forms to map constructor

New: `IndexOutOfBoundsException` — subclass of `PyIndexError`, raised by vector `nth` / `__getitem__` out of bounds when no default is supplied. Added to `exceptions.rs`.

---

## 9. Testing Bar

### 9.1 Unit tests (per collection type)

`tests/test_plist.py`, `test_pvector.py`, `test_parraymap.py`, `test_phashmap.py`, `test_phashset.py`. Each covers:

- Construction (empty, single, multi-element).
- Equality (same contents ≡; different contents ≢).
- Hash stability (same content → same hash, across construction orders where defined).
- Iteration yields expected elements.
- Count correctness.
- Meta preservation through `with_meta`.
- `conj` / `assoc` / `dissoc` / `disj` / `pop` produce correct successors and leave originals unchanged (structural sharing sanity).
- Python dunders (`__len__`, `__iter__`, `__eq__`, `__hash__`, `__getitem__`, `__contains__`, `__call__`).
- Edge: empty vs non-empty, nil keys/values, nested collections.

### 9.2 Protocol dispatch tests

`tests/test_iseq.py`, `test_iequiv.py`, `test_ihasheq.py`, `test_imeta.py`. Verify:

- Every collection satisfies its declared protocols (`satisfies?` returns true).
- `rt::equiv(a, b)` returns correct value; `rt::hash_eq(a) == rt::hash_eq(b)` when `rt::equiv(a, b)`.
- `rt::seq` / `rt::first` / `rt::next` / `rt::count` work on every sequence type.

### 9.3 Transient tests

`tests/test_transients.py`:

- Batch build: start with empty persistent, `as_transient`, apply N `conj_bang` / `assoc_bang`, `persistent_bang`, compare to applying same ops directly on persistent.
- Stale transient raises `IllegalStateException`: `t = as_transient(v); v2 = persistent_bang(t); conj_bang(t, x)` must raise.
- Cross-thread transient raises: `t = as_transient(m)` on thread A; thread B calls `conj_bang(t, x)` — must raise.
- ArrayMap → HashMap promotion during transient build: `transient` a small map, `assoc_bang` >8 keys, `persistent_bang`; result is `PersistentHashMap` and has all keys.

### 9.4 Property-based fuzzing (Python `hypothesis`)

`tests/test_collections_fuzz.py`. Generate random op sequences; compare our collections to Python built-in references.

| Collection | Reference | Ops fuzzed | Invariants asserted |
|------------|-----------|------------|---------------------|
| PersistentList | Python `list` (with ops applied to head/tail per Clojure semantics) | `cons`, `rest`, `first`, `peek`, `pop`, `count` | same sequence, same count, same first |
| PersistentVector | Python `list` | `conj`, `assoc_n`, `pop`, `nth` | same sequence, same count, same nth values |
| PersistentHashMap / ArrayMap | Python `dict` | `assoc`, `dissoc`, `contains_key`, `val_at` | same key set, same value per key, same count |
| PersistentHashSet | Python `set` | `conj`, `disj`, `contains` | same membership, same count |
| TransientVector roundtrip | Python `list` | mixed `conj_bang` / `assoc_bang` / `pop_bang` | after persistent_bang, matches ref |
| TransientHashMap roundtrip | Python `dict` | mixed `assoc_bang` / `dissoc_bang` | after persistent_bang, matches ref |

**Budget:** 500 cases per property on CI (~few seconds), 10k+ cases nightly.

**Structural-sharing integrity:** in each fuzz, hold references to intermediate persistent values; after many downstream ops, re-check each held reference's iteration/count/eq — they must equal their original captured state. Catches path-copy bugs that mutate a parent trie.

### 9.5 Rust-side proptest on HAMT node invariants

`crates/clojure_core/tests/proptest_hamt.rs`:

- `BitmapIndexedNode`: `popcount(bitmap) == array.len()` after arbitrary `assoc` / `dissoc` sequences.
- `ArrayNode`: number of `Some(_)` slots matches `count` field.
- Depth bound: no HAMT deeper than 7 levels for 32-bit hashes.
- `HashCollisionNode`: all entries have the same hash.

### 9.6 Stress test

`tests/test_collections_stress.py`:

- Build a 10k-entry hash map transiently in one thread; `persistent_bang`; do 10k random `val_at` calls; compare to a Python `dict` reference — 100% match rate.
- 32-worker `ThreadPoolExecutor` each doing 500 `conj` on *separate* persistent vectors (no shared transient, since transients are single-thread) — no errors, no GC anomalies, final lengths correct.
- Held reference stress: capture a `PersistentHashMap` with 1000 entries; in another thread, repeatedly `assoc` derivative maps; confirm the held one still iterates the original 1000 entries.

### 9.7 Loom

The HAMT itself isn't amenable to loom model-checking (too many atomics, too many code paths), and Clojure's atomic semantics on individual nodes are narrow: `AtomicUsize` for the transient edit token. We add one new loom test:

- `crates/clojure_core/tests/loom_transient_edit.rs`: two threads attempt operations on a transient, one is owner, the other isn't — owner completes, non-owner receives the equivalent of `IllegalStateException` signal (modeled via a flag since we're testing the atomic semantics). Exhaustive interleaving should never observe both threads mutating.

---

## 10. Non-Goals / Follow-on Specs

Each of these is a separate spec:

1. **Reader + printer** — next. Uses these collections directly.
2. **Sorted collections** — `PersistentTreeMap`, `PersistentTreeSet`, `Sorted` protocol.
3. **Queues + exotic seqs** — `PersistentQueue`, `Cycle`, `Iterate`, `Repeat`.
4. **Reducers** — `IReduce`, `IReduceInit`, `reduce` core.
5. **Clojure-JVM-compatible hash codes** — precise bit-for-bit match of Clojure-JVM's hash values for interop / print-read equivalence.
6. **Subvec** — O(1) slicing view of PersistentVector.
7. **Rseq** — reverse iteration for vectors, sorted maps.
8. **Print-method multimethod** — user-extensible printing (after evaluator + multimethods land).

This spec leaves explicit hooks for (1) — the reader will build vectors/maps/sets directly via transients + `persistent_bang` — and (5) — we've centralized hash-eq in `IHashEq` and `rt::hash_eq`, so upgrading to bit-compatible hashes is a localized edit later.
