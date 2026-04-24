# Benchmarks

Microbenchmarks + two macrobenchmarks for the Clojure-on-Python runtime.
The harness loads each `.clj` file under `bench/clj/`, looks up its
`benchmarks` var (a `{name fn}` map), and times each 0-arg fn via
`perf_counter_ns`.

## Running

```bash
# All benches, 25 iters + 5 warmup, default:
python bench/run.py

# Filter by name substring or file stem:
python bench/run.py inner ukanren
python bench/run.py coll/vector

# Tweak measurement knobs:
python bench/run.py --iters 50 --warmup 10

# Skip writing a JSON result file (for quick local runs):
python bench/run.py --no-save
```

Results are dumped to `bench/results/bench-<short-sha>[-dirty]-<ts>.json`.

## Comparing runs

```bash
python bench/diff.py bench/results/bench-<before>.json \
                    bench/results/bench-<after>.json
```

Per-bench median delta with `faster` / `SLOWER` markers (default
±5% threshold).

## Layout

```
bench/
  run.py              # runner — loads .clj files, times, emits JSON
  diff.py             # compares two result JSONs
  clj/
    inner_loop.clj    # VM hot path: loop/recur, reduce, transduce, dotimes
    collections.clj   # PVector / PHashMap / PHashSet / lists
    dispatch.clj      # fn call, protocol, multimethod, keyword, Var deref
    ukanren.clj       # classic minikanren port + appendo queries
    wordcount.clj     # regex tokenize + hash-map accumulation
  results/            # JSON snapshots (git-ignored? see below)
```

## Bench file shape

```clojure
(ns bench.something)

(defn some-op [n] ...)

(def V100k (vec (range 100000)))

(def benchmarks
  {"something/op-10k" (fn [] (some-op 10000))
   "something/op-vec" (fn [] (something/op-over V100k))})
```

Each value in `benchmarks` is a 0-arg function — the runner times one
invocation. Pick N so one call takes ~1–100 ms; under ~100 µs the timer
noise dominates.

## Profiling a slow bench

`cProfile` can't see the Rust side of the extension. Use `py-spy`:

```bash
# Produces flame.svg combining Python + Rust frames
py-spy record -o flame.svg --native -- python bench/run.py <filter> \
  --iters 200 --warmup 20 --no-save
```

For Rust-only hot-spot detail (inlining preserved) use `samply` or
`cargo flamegraph` on Linux.
