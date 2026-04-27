# Deferred Work

What is **explicitly out of scope** for the initial-runtime substrate.
The endgame target is the full design described in
`dispatch-architecture-summary.md` and `rc-research-findings.md`; this
document tracks what we are *not* building yet so future rounds know
what's left.

The v1 surface is just three subsystems: type system, RC, polymorphic
dispatch. Nothing Clojure-specific is built yet.

## Heap allocator

- **Naive `std::alloc`-backed allocator only.** Each heap object is a
  separate `alloc`/`dealloc`. Behind a `GcAllocator` trait so future
  allocators slot in without touching client code.
- **Deferred:** RCImmix line-and-block heap, copying evacuation,
  sticky mark bits, lazy mod-buffer, age-oriented collection.

## Reference counting

- **In v1:** biased RC with a single signed counter (`rc < 0` biased,
  `rc > 0` shared); manual `gc::dup` / `gc::drop` at call sites.
- **Deferred:**
  - **Perceus-style automatic dup/drop insertion.** Macros do not yet
    emit RC ops around argument flow. Callers manage RC manually.
  - **Reuse pairing / runtime in-place mutation.** When `refcount == 1`
    a `dup` + `drop` pair becomes a no-op and the storage is reused.
    This is the FBIP / "automatic transients" win and the largest
    remaining performance lever.
  - **Borrow inference.** Compile-time elimination of dup/drop pairs
    based on parameter usage analysis.
  - **Drop-guided / frame-limited reuse** (Lorenzen & Leijen 2023).
  - **Bacon trial-deletion cycle collector.** No cycle handling at
    all in v1. Cycles are leaks until the collector lands.
  - **Weak references.** No first-class weak ref type yet.
  - **STM batching of RC ops across transaction commits.**

## Sharing primitives

- **In v1:** the `gc::share` escape op is defined and tested in
  isolation, but **no caller invokes it** because there are no
  sharing primitives yet.
- **Deferred:** atom, ref, agent, channel — the constructs that
  trigger escape in real workloads.

## Dispatch

- **In v1:** all three tiers shipped (per-callsite IC, per-type
  perfect-hash table, global stub cache). Megamorphic transitions are
  deferred — every IC miss in v1 walks tiers 2 and 3.
- **Deferred:**
  - **Megamorphic IC mode.** After N distinct types at one site, the
    IC enters a sentinel state and goes directly to tier 3.
  - **Crossbeam-epoch reclamation of old per-type tables.** v1 lets
    `Arc` reclaim them; old tables may be retained briefly under
    contention. Acceptable because extend events are rare.
  - **Method-fn ABI hardening.** v1 uses `unsafe extern "C" fn` with
    a single `(*const Value, usize)` shape. Cranelift codegen later
    may need a slightly different ABI; pluggable behind the IC.

## Python binding

- **In v1:** pure-Rust kernel only. `cargo test` runs without Python
  installed. The `TYPE_PYOBJECT` tag is reserved but the kernel never
  dereferences a pyobject payload.
- **Deferred:**
  - The PyO3 cdylib binding crate (`crates/clojure_py/`).
  - `tp_traverse` on the Python wrapper for cross-heap cycle
    collection.
  - Boundary conversion policy (eager vs lazy conversion of Python
    builtins to Clojure equivalents).
  - Protocol extension to Python types (so `(get py_dict :foo)` works
    without conversion).

## Clojure semantics

Nothing Clojure-specific exists yet. Out of scope for v1:

- IFn (the protocol that makes values callable).
- Keyword and Symbol types.
- Cons, PersistentList, EmptyList.
- PersistentVector, PersistentArrayMap, PersistentHashMap,
  PersistentHashSet (and their transients).
- LazySeq, MapEntry, all seq types.
- Var, Namespace, intern / refer / alias.
- Reader, evaluator, REPL.
- Macros (Clojure-level `defmacro`, `defn`, `cond`, ...).
- Multimethods, hierarchies (`derive`, `isa?`).
- Records and types (`defrecord`, `deftype`, `reify`).
- `clojure.core` port.

## Tooling

- **Deferred:** Cranelift JIT, `loom`-gated CI workflow, fuzz harness
  beyond proptest, full criterion baseline / regression-tracking
  pipeline.

## Notes

- All deferred items have a stable seam in the v1 design behind which
  they slot. The point of building v1 with the full type-id / header /
  IC layout — even with the simplest implementations — is that none
  of these deferred upgrades require a layout change.
