# clojure-py

## Status

Initial **runtime substrate** in place: fat-`Value` type system, biased
reference counting (Lean-4 / Perceus style), three-tier polymorphic
dispatch (per-callsite IC + per-type Ducournau perfect-hash + global
stub cache), and a proc-macro DSL (`register_type!`, `protocol!`,
`implements!`, `dispatch!`).

Pure-Rust kernel, no Python integration yet — `cargo test` runs without
Python installed. PyO3 binding crate, RCImmix heap, Perceus auto
dup/drop, cycle collector, and all Clojure-specific code are deferred
to later rounds (see `doc/deferred-work.md`).

## Build

Requires Rust 1.85+, edition 2024.

```bash
cargo build --workspace
```

## Test

```bash
cargo test --workspace                              # full suite
cargo test -p clojure_rt --test lazy_cons --release # 1M-cell walk, leak-free
```

### Loom model checks

```bash
RUSTFLAGS="--cfg loom" cargo test -p clojure_rt --test loom_rc      --release
RUSTFLAGS="--cfg loom" cargo test -p clojure_rt --test loom_ic      --release
RUSTFLAGS="--cfg loom" cargo test -p clojure_rt --test loom_extend  --release
```

## Bench

```bash
cargo bench -p clojure_rt
```

Three bench harnesses: `lazy_cons` (biased/escaped/drop-to-zero),
`dispatch` (tier-1 hot + megamorphic), and `rc_micro` (escape op).

## Documentation

- `doc/dispatch-architecture-summary.md` — the dispatch-design reference
- `doc/rc-research-findings.md` — RC research summary
- `doc/type-system.md` — type-system decisions
- `doc/deferred-work.md` — what's not yet built

## Repository layout

```
Cargo.toml                          # Rust workspace
crates/
  clojure_rt/                       # the substrate (no PyO3)
    src/                            # value, header, gc, rc, type_registry,
                                    # protocol, dispatch/*, registry, error
    tests/                          # unit + integration + loom + proptest
    benches/                        # criterion benches
  clojure_rt_macros/                # proc-macros
doc/                                # design / research / decision docs
```
