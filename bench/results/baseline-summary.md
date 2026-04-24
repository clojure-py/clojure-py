# Baseline — b733884 (2026-04-23)

Full-matrix tests: 1714 passing on linux-x64, linux-arm64, macos-arm64.
Measurements taken on linux-x64 (local, no CPU pinning) @ iters=25,
warmup=5. Raw numbers in `baseline-b733884.json`.

## Inner loop / VM dispatch

| bench                         | median  | notes                                  |
|-------------------------------|---------|----------------------------------------|
| loop-recur-sum-100k           | 200 ms  | ~2 µs / iteration — baseline for VM    |
| fn-call-tight-100k            | 209 ms  | tight `(f x)` — call overhead dominant |
| reduce-sum-100k               | 738 ms  | 3.7× slower than loop-recur            |
| transduce-sum-100k (filter+map)| 1.15 s | **5.7× loop-recur**; stack of xforms   |
| nested-dotimes-200×1k         | 587 ms  | `(swap! atom inc)` in the hot loop     |
| doseq-vec-100k                | 585 ms  | chunked-seq traversal over vector      |

## Collections

### Vector (PVector)

| bench                           | median  | notes                                         |
|---------------------------------|---------|-----------------------------------------------|
| vector-conj-10k (persistent)    | 24 ms   |                                               |
| vector-conj-transient-10k       | 25 ms   | **transient path does not speed this up** — bug? |
| vector-nth-10k                  | 34 ms   |                                               |
| vector-assoc-10k (in-place rewrite) | 37 ms |                                             |

### Hash-map

| bench                                | median  | notes                          |
|--------------------------------------|---------|--------------------------------|
| map-assoc-persistent-kw-8 (array-map)| 37 µs   | size stays < 8, small data     |
| map-assoc-persistent-kw-100          | 441 µs  | crosses into HAMT              |
| map-build-int-10k-transient          | 29 ms   | fast — ints hash trivially     |
| map-build-str-10k-transient          | 107 ms  | 3.6× int — string hash cost    |
| map-build-kw-10k-transient           | 136 ms  | 4.6× int — **keyword hash overhead**, unexpected |
| map-get-int-10k                      | 99 ms   |                                |
| map-get-kw-10k                       | 47 ms   | kw beats int on lookup (cached hash)  |
| map-get-str-10k                      | 238 ms  | **5× kw** — str hash per lookup |
| map-dissoc-all-10k-kw                | 55 ms   |                                |

### Set

| bench                  | median  |
|------------------------|---------|
| set-conj-10k           | 32 ms   |
| set-contains-10k       | 38 ms   |

### Seq walk

| bench                  | median  |
|------------------------|---------|
| list-walk-10k          | 22 ms   |

## Call / dispatch overhead (all N=100k)

| bench                      | median  | vs direct-call   |
|----------------------------|---------|------------------|
| direct-call                | 189 ms  | 1.0×             |
| var-call                   | 213 ms  | 1.13× (Var deref) |
| dyn-var-call (binding)     | 225 ms  | 1.19×            |
| kw-as-fn                   | 246 ms  | 1.30×            |
| proto-mono                 | 260 ms  | 1.37× (cache hit) |
| proto-poly (5 types ring)  | 431 ms  | **2.28×** — cache thrash |
| mm-mono                    | 322 ms  | 1.70×            |
| mm-poly (5 keys ring)      | 466 ms  | 2.47×            |

## Macrobenchmarks

| bench                       | median  | notes                          |
|-----------------------------|---------|--------------------------------|
| ukanren/appendo-splits-8-100 | 3.1 ms  |                                |
| ukanren/appendo-splits-16-100| 5.8 ms  | scales sublinear with k        |
| ukanren/unify-chain-10k     | 157 ms  | pure unify + `assoc` stress    |
| wc/reduce-5k                | 22 ms   |                                |
| wc/frequencies-5k           | 21 ms   | ~= reduce                      |
| wc/reduce-20k               | 84 ms   |                                |
| wc/frequencies-20k          | 86 ms   |                                |
| wc/transient-20k            | 94 ms   | **slower than persistent** — transient HAMT likely broken |

## Hot spots to investigate

Ranked by likely ROI × surprise-factor:

1. **Transducer stack overhead.** `transduce` is 5.7× slower than
   `loop-recur`. Every step-fn wraps a closure, and our VM pays dispatch
   cost per call. Even a minor per-invocation saving here multiplies.
2. **Transient hash-map / vector not faster than persistent.** If
   confirmed, this is a bug: both `assoc!`/`conj!` should short-circuit
   the structural-sharing path. Worth a look before micro-tuning anything.
3. **VM dispatch per-op cost ~2 µs.** This is the floor for everything
   else. `py-spy --native` on `loop-recur-sum` will show whether it's
   bytecode decode, `invoke_n`, or Var deref dominating.
4. **Keyword hashing on assoc.** Keywords *should* hash by cached slot;
   4.6× slower than ints during map build suggests something is
   recomputing per call (protocol dispatch on `__hash__`?).
5. **String hashing in ILookup.** `map-get-str` is the slowest lookup
   bench. Possible wins from caching a string hash on the key or a
   fast-path in the Rust HAMT for Python `str`.
6. **Multimethod dispatch (mm-mono 1.70×).** Compared to protocol-mono
   (1.37×), the multimethod cache path is heavier. Worth checking
   whether the `isa?` walk runs on every call even with a cache hit.
