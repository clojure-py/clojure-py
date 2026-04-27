# Single-Digit-Nanosecond Protocol Dispatch in Rust: Architecture Summary

## Core Design

A **three-tier dispatch system** combining proven techniques from Clojure, V8, Objective-C, and CPython 3.11+, with no runtime code generation. Hot dispatch costs **4-6 cycles (~1.3-2 ns)** on modern x86_64 and ARM64 — equivalent to a C++ virtual call.

## The Three Tiers

**Tier 1 — Per-call-site inline cache (the hot path).** A 16-byte slot beside each call holding `(cached_type_id: u32, version: u32, fn_ptr: *const ())`. Dispatch is one `type_id` load from the object header, one compare against the cached value, one direct call. When predicted by the CPU's branch predictor (ITTAGE on Zen 4/5 and Apple Silicon, three-level BTB on Golden Cove), this is ~4-6 cycles. This is the PEP 659 pattern — pure data, no codegen.

**Tier 2 — Per-type perfect-hash protocol table.** When the IC misses, consult the type's own table indexed by `(proto_id) & mask`, using Ducournau's PN-and perfect hashing (TOPLAS 2008). Each type owns its table; adding a protocol implementation rebuilds only that type's table. Lookup is ~6-9 cycles.

**Tier 3 — Global stub cache.** A 4096-entry hash keyed by `(type_id ^ proto_id) & mask` for truly megamorphic sites. V8's design.

## Open-World Extensibility

Both new types and new protocols are addable at runtime via three mechanisms:

- **`u32` type IDs** interned in a global registry (not `std::any::TypeId`, which is too wide).
- **Per-protocol-fn version counters** (`AtomicU64`). Bumped on `extend`; callsites detect staleness lazily on next call. No global walks, no on-stack replacement.
- **`ArcSwap` for per-type tables.** Single-writer extends; readers get lock-free access. Old tables reclaimed via epoch-based GC (`crossbeam-epoch`).

## Key Implementation Choices

| Decision | Choice | Rationale |
|---|---|---|
| Type identity | `u32` in object header | One 4-byte cmp; fits with hidden-class pattern |
| Cache layout | 16-byte slot, `repr(C, align(16))` | 4 ICs per cache line |
| Concurrency | `AtomicU64` packed `(type, ver)` + `AtomicPtr` for fn | Lock-free, Release/Relaxed ordering |
| Registration | `linkme`/`inventory` for static, macro for runtime | Compile-time impls bypass cache (Clojure asymmetry) |
| Fallback width | 4-way polymorphic before global stub cache | Matches V8, Pharo Cog measurements |

## Why This Hits the Target

Three hardware facts make it work: **(1)** Post-2022 mainstream CPUs run indirect branches at full speed under eIBRS/Auto-IBRS — no retpoline tax in userspace. **(2)** Modern BTBs (Zen 5: 16K entries, Golden Cove: 12K, Apple M1: 1024) easily hold every dispatch site. **(3)** L1d hit on the IC slot is 4-5 cycles, fast enough to be the bottleneck, not memory.

The historic intuition that "fast dispatch needs a JIT" is wrong: every required trick — inline caches, hidden classes, perfect hashing, version-based invalidation — is a data-structure technique. CPython 3.11 proved this in production with 25% speedups across the board, all without codegen.

## What You Don't Build

Skip selector coloring and row displacement (Driesen 1999) — beautiful but require global recoloring on extend. Skip C++-style fixed vtable slots — incompatible with adding protocols at runtime. Skip world-age (Julia) — more powerful than needed; per-protocol versioning is sufficient for Clojure semantics and considerably simpler.

## Memory Cost

For 1000 types × 100 protocols × 10 protocols-per-type × 5 methods-per-protocol: ~2 MB total dispatch metadata + ~16 bytes per call site. Comfortably L2/L3 resident.
