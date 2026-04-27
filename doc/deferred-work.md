# Deferred Work

What is **explicitly out of scope** for the initial-runtime substrate.
The endgame target is the full design described in
`dispatch-architecture-summary.md` and `rc-research-findings.md`; this
document tracks what we are *not* building yet so future rounds know
what's left.

The v1 surface is just three subsystems: type system, RC, polymorphic
dispatch. Nothing Clojure-specific is built yet.

## Heap allocator

- **In v1+:** `RCImmixAllocator` — thread-local-bump on 32 KB blocks
  with 128 B lines and intrusive cross-thread free lists. Owner-thread
  alloc fast path is bump + line-counter increment, no atomics. Cross-
  thread `dealloc` CAS-prepends onto a per-block remote-free list;
  owner drains during slow path. Large objects (>8 KB) fall through to
  `std::alloc`. See `doc/rcimmix-allocator.md`.
- **Deferred:**
  - **Compaction / copying evacuation** — sparse-block defragmentation.
    Full RCImmix paper feature.
  - **Sticky mark bits / age-oriented collection** — generational RC.
  - **TLAB stealing across thread boundaries** — beyond
    `partial_pool`/`empty_pool` cooperation.
  - **Orphan reaping on thread exit** — biased-RC objects owned by an
    exited thread leak in v1+. Auto-share-on-exit hooks deferred.
  - **Slab-level OS release** — blocks alloc'd 8 at a time can't be
    individually returned to OS; we leak past the empty_pool cap until
    process exit. Track slab origin and ref-count blocks within their
    slab to enable per-slab release.

## Reference counting

- **In v1+:** biased RC with a single signed counter (`rc < 0` biased,
  `rc > 0` shared); manual `gc::dup` / `gc::drop` at call sites.
  Substrate-level optimizations landed: dyn-dispatch bypass on the
  alloc fast path (`RCIMMIX.alloc_inline`), single-line specialization
  in `inc/dec_line_counts`. Current `lazy_cons biased step` is 14 ns;
  the remaining gap to the original 10-12 ns target is dominated by
  the RC accounting work itself (two `inc_line_counts` per step), not
  by allocator overhead.

- **Deferred:**

  ### Perceus (the next major lever — explicitly held)

  The full Perceus pipeline (Reinking et al. 2021) — automatic
  dup/drop insertion, borrow inference, and reuse pairing
  (FBIP) — is the largest remaining performance lever. With reuse
  pairing, the typical functional pattern `Cons(f(h), recur(t))` over
  a unique-RC list compiles to in-place writes: the alloc and drop
  cancel, and a list traversal becomes a single rewriting pass. We
  estimate this would put `lazy_cons biased step` at ~5-8 ns.

  **Why we're holding off.** Perceus is a *compiler* pass, not a
  runtime feature. It runs over an IR with explicit ownership ops
  and uses dataflow analysis (last-use, reuse-credit threading,
  shape matching) to insert and pair RC ops. We don't have a
  Clojure-level compiler yet — that's a distinct sub-project, and
  building a compiler primarily to enable Perceus would be putting
  the cart before the horse. The natural sequence is:
    1. Persistent collections (HAMT vector/map, lists, sets).
    2. Reader (text → forms).
    3. Bytecode evaluator (forms → ops).
    4. Compiler proper (forms → optimized IR), at which point
       Perceus is the natural shape of the RC-insertion pass.
  Until step 4, every alloc/drop is hand-written or macro-generated,
  so there's no IR for a Perceus pass to operate on. The runtime
  side of Perceus (the `if rc==1 { reuse } else { alloc+drop }`
  branch and the line-count adjust-in-place) is buildable today as
  a hand-written demo in benches but doesn't compose without the
  compiler.

  Components, ordered by their compiler dependency:
  - **Reuse pairing / runtime in-place mutation** — needs the
    compiler to identify alloc/drop pairs.
  - **Automatic dup/drop insertion** — needs the IR to insert into.
  - **Borrow inference** — needs lifetime analysis on the IR.
  - **Drop-guided / frame-limited reuse** (Lorenzen & Leijen 2023) —
    extension to the above.

  ### Other RC items

  - **Bacon trial-deletion cycle collector.** No cycle handling at
    all in v1. Cycles are leaks until the collector lands. Independent
    of Perceus; can land any time.
  - **Weak references.** No first-class weak ref type yet.
  - **STM batching of RC ops across transaction commits.** Requires
    atom/ref/channel to exist first.

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
