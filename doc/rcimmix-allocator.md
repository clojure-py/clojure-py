# RCImmix Allocator — Foundation Reference

The clojure-py heap allocator. Replaces the v1 `NaiveAllocator` (std::alloc-backed) with a thread-local-bump-allocator on 32 KB / 128 B line-and-block heap. Foundation only — compaction, evacuation, and gen-RC are deferred (see `deferred-work.md`).

## Targets

Validated against `NaiveAllocator` baselines from the substrate round:

| Bench | Naive | RCImmix target |
|---|---|---|
| lazy_cons biased | 30 ns/cell | 10–12 ns/cell |
| lazy_cons escaped | 44 ns/cell | 22–26 ns/cell |
| drop_to_zero | 14 ns | 2–4 ns |
| rc_share op | 21 ns | 8–11 ns |
| dispatch tier-1 hit | 1.16 ns | 1.16 ns (unchanged) |

## Heap shape

- **Block** — 32 KB, 32 KB-aligned. From any pointer interior to a block, `block_addr = ptr & !(BLOCK_SIZE - 1)`.
- **Line** — 128 B. 256 lines per block. First 3 lines reserved for `BlockHeader`; remaining 253 lines hold objects.
- **Three populations**: per-thread TLAB blocks; global `partial_pool` (partially-used, unowned); global `empty_pool` (fully empty, capped at 16 — excess returns to OS).
- **Object tiers**: small ≤ 128 B (single line); medium 128 B – 8 KB (multi-line); large > 8 KB (falls through to `std::alloc`, tracked in `LARGE_OBJECTS` sidetable).

## `BlockHeader` (in-band, at block start)

| Field | Type | Mutator |
|---|---|---|
| `owner_tid` | `AtomicU64` | thread acquiring/releasing under pool mutex |
| `line_counts` | `[Cell<u8>; 256]` | owner thread only (no atomics) |
| `bump_ptr` / `bump_end` | `Cell<u32>` | owner thread only |
| `remote_free_head` | `AtomicPtr<Header>` | any thread CAS-prepends; owner atomic-swaps to drain |
| `next_in_pool` | `Cell<*mut Block>` | under pool mutex only |

Owner identity is monotonic-and-unique within any TLAB-residence interval: a block transitions `pool → owned by exactly one thread → pool` and pool transitions are serialized. So owner-only fields need no synchronization.

## Concurrency invariants

- **Owner alloc fast path**: bump increment + 1–2 `Cell<u8>` ops + Header write. No atomics. Sub-nanosecond on modern hardware (only the TLS lookup is non-trivial).
- **Owner dealloc**: line-count decrement. No atomics.
- **Remote dealloc**: CAS-prepend onto `remote_free_head` (Release on success, Acquire on retry). Repurposes the dead object's body bytes 0..8 as the `next` pointer (the destructor has already run; the body is garbage).
- **Drain** (owner, on slow path): atomic-swap `remote_free_head → null`, walk the chain, decrement line counts. Synchronization point: any prepend that observes the swap-result-as-null lands on the fresh head, not the chain being drained. No double-decrement, no lost frees.

## Cross-thread `dealloc` race summary

| Step | Owner thread | Remote thread |
|---|---|---|
| 1 | allocs in block B; rc = -1 (biased) | — |
| 2 | calls `share_heap`; rc → +1 (atomic) | — |
| 3 | publishes pointer | — |
| 4 | drops; rc fetch_sub | — |
| 5 | — | reads pointer; rc fetch_sub → 0 |
| 6 | — | `dealloc`: not owner of B, CAS-prepends onto B's `remote_free_head` |
| 7 | next slow-path: drain | — |
| 8 | swap remote_free_head; walk chain; decrement line counts | — |

No race on owner-only fields.

## Thread-exit contract (v1)

When a thread exits, its TLAB block returns to `partial_pool`. Live (rc != 0) biased-RC objects in that block become **orphans** — owner is gone, rc never reaches 0, lines never recycle. **They leak.** Future owner skips occupied lines; no UB. Long-lived threads must explicitly drop or `share_heap` their biased objects before exit. Auto-orphan-reaping is deferred work.

## Deferred (vs. full RCImmix paper)

- Compaction / copying evacuation (sparse-block defragmentation)
- Sticky mark bits / age-oriented collection (gen-RC)
- TLAB stealing across thread boundaries
- Cycle collection (substrate-level deferral, unchanged)
- Orphan reaping on thread exit
- Auto-share-on-exit hooks
- Cranelift hardening of the `*const ()` → `MethodFn` transmute (substrate concern)

The `GcAllocator` trait is unchanged; all deferred items slot in behind it.

## Tests & benches

Multi-threaded `remote_free` correctness has dedicated coverage:
- `tests/rcimmix_remote_free.rs` — eventual-drain, no-double-decrement, 8-thread contention, thread-exit orphan handling
- `tests/loom_rcimmix_remote_free.rs` — CAS protocol; owner drain vs. concurrent prepend
- `benches/rcimmix_remote_free.rs` — worst case (alloc on A, dealloc on B), N-thread contention curve, drain cost vs. chain length

Existing substrate benches replicate against RCImmix to verify the targets above.

## Companion docs

- `doc/type-system.md` — fat-Value layout, Header invariants
- `doc/dispatch-architecture-summary.md` — three-tier dispatch (allocator-independent)
- `doc/rc-research-findings.md` — biased RC background
- `doc/deferred-work.md` — what RCImmix doesn't cover yet
