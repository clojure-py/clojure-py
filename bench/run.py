"""Benchmark runner.

Each file under bench/clj/ defines (def benchmarks {"name" (fn [] ...) ...}).
The runner loads each file into a fresh ns, times each 0-arg fn with
perf_counter_ns, reports min/median/p95/p99/stddev over N iterations after
a warmup, and dumps everything to JSON for later diffing.

Usage:
    python bench/run.py                         # all benches
    python bench/run.py inner-loop collections  # filter by file prefix or name
    python bench/run.py --iters 50 --warmup 5   # override measurement knobs

Output:
    bench/results/bench-<sha>-<timestamp>.json
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable

# Must come before clojure._core so the Python importer machinery is set up.
import clojure  # noqa: F401
from clojure._core import create_ns, load_file_into_ns, symbol, val_at


# ---------------------------------------------------------------------------
# Loading bench files
# ---------------------------------------------------------------------------

BENCH_ROOT = Path(__file__).resolve().parent
CLJ_ROOT = BENCH_ROOT / "clj"
RESULTS_ROOT = BENCH_ROOT / "results"


def _parse_ns(path: Path) -> str:
    """Extract the declared namespace from a `.clj` file's `(ns ...)` form.

    Works for simple `(ns foo.bar ...)` forms without requiring the reader.
    """
    text = path.read_text(encoding="utf-8")
    # Skip whitespace and `;` line comments until we find `(ns`.
    i = 0
    while i < len(text):
        if text[i] == ";":
            while i < len(text) and text[i] != "\n":
                i += 1
        elif text[i].isspace():
            i += 1
        elif text[i : i + 3] == "(ns":
            j = i + 3
            while j < len(text) and text[j].isspace():
                j += 1
            k = j
            while k < len(text) and (text[k].isalnum() or text[k] in "-_.*?!/"):
                k += 1
            return text[j:k]
        else:
            break
    raise ValueError(f"No (ns ...) form found in {path}")


def load_bench_file(path: Path) -> list[tuple[str, Callable]]:
    """Return an ordered [(bench_name, zero_arg_fn), ...] list from one file."""
    ns_name = _parse_ns(path)
    ns = create_ns(symbol(ns_name))
    load_file_into_ns(str(path), ns)
    if "benchmarks" not in ns.__dict__:
        raise RuntimeError(f"{path}: no `benchmarks` var defined")
    m = ns.__dict__["benchmarks"].deref()
    out = []
    for k in m:
        fn = val_at(m, k, None)
        if not callable(fn):
            raise RuntimeError(f"{path}: benchmark {k!r} is not callable")
        out.append((str(k), fn))
    out.sort(key=lambda kv: kv[0])
    return out


def discover_benches() -> list[tuple[str, Path, Callable]]:
    """[(bench_name, file_path, fn), ...] across all .clj files under bench/clj/."""
    benches = []
    for path in sorted(CLJ_ROOT.glob("*.clj")):
        for name, fn in load_bench_file(path):
            benches.append((name, path, fn))
    return benches


# ---------------------------------------------------------------------------
# Measurement
# ---------------------------------------------------------------------------


def time_one(fn: Callable, *, iters: int, warmup: int) -> dict:
    """Run `fn()` `warmup + iters` times, return timing stats (ns)."""
    for _ in range(warmup):
        fn()
    samples: list[int] = []
    for _ in range(iters):
        t0 = time.perf_counter_ns()
        fn()
        t1 = time.perf_counter_ns()
        samples.append(t1 - t0)
    samples.sort()
    def pct(p: float) -> int:
        # Inclusive percentile on a sorted list.
        if not samples:
            return 0
        k = int(round((len(samples) - 1) * p / 100))
        return samples[k]
    return {
        "iters": iters,
        "warmup": warmup,
        "min_ns": samples[0],
        "median_ns": samples[len(samples) // 2],
        "mean_ns": int(statistics.fmean(samples)),
        "p95_ns": pct(95),
        "p99_ns": pct(99),
        "max_ns": samples[-1],
        "stddev_ns": int(statistics.pstdev(samples)) if len(samples) > 1 else 0,
    }


# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------


def fmt_ns(ns: int) -> str:
    if ns < 1_000:
        return f"{ns:d} ns"
    if ns < 1_000_000:
        return f"{ns / 1_000:.2f} µs"
    if ns < 1_000_000_000:
        return f"{ns / 1_000_000:.2f} ms"
    return f"{ns / 1_000_000_000:.3f} s"


def git_sha() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=BENCH_ROOT.parent,
            stderr=subprocess.DEVNULL,
        ).decode().strip()
    except Exception:
        return "unknown"


def git_dirty() -> bool:
    try:
        return bool(subprocess.check_output(
            ["git", "status", "--porcelain"],
            cwd=BENCH_ROOT.parent,
            stderr=subprocess.DEVNULL,
        ).decode().strip())
    except Exception:
        return False


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("filters", nargs="*",
                    help="substring match on bench name (file or fn); ANY match keeps the bench")
    ap.add_argument("--iters", type=int, default=25)
    ap.add_argument("--warmup", type=int, default=5)
    ap.add_argument("--no-save", action="store_true",
                    help="don't write a results JSON file")
    args = ap.parse_args()

    benches = discover_benches()
    if args.filters:
        benches = [b for b in benches if any(f in b[0] or f in b[1].stem for f in args.filters)]
    if not benches:
        print("No benchmarks match the given filters.", file=sys.stderr)
        return 1

    sha = git_sha()
    dirty = git_dirty()
    print(f"# clojure-py bench — git {sha}{' (dirty)' if dirty else ''}  "
          f"iters={args.iters} warmup={args.warmup}")
    print(f"# {len(benches)} bench(es)\n")

    results: dict[str, dict] = {}
    name_w = max(len(n) for n, _, _ in benches) + 2
    header = f"  {'name':<{name_w}}  {'min':>10}  {'median':>10}  {'p95':>10}  {'max':>10}"
    print(header)
    print("  " + "-" * (len(header) - 2))

    for name, path, fn in benches:
        sys.stdout.write(f"  {name:<{name_w}}  ")
        sys.stdout.flush()
        try:
            r = time_one(fn, iters=args.iters, warmup=args.warmup)
        except KeyboardInterrupt:
            print("\n(interrupted)")
            return 130
        except Exception as e:
            print(f"ERROR: {type(e).__name__}: {e}")
            results[name] = {"error": f"{type(e).__name__}: {e}", "file": path.stem}
            continue
        r["file"] = path.stem
        results[name] = r
        print(f"{fmt_ns(r['min_ns']):>10}  {fmt_ns(r['median_ns']):>10}  "
              f"{fmt_ns(r['p95_ns']):>10}  {fmt_ns(r['max_ns']):>10}")

    if not args.no_save:
        RESULTS_ROOT.mkdir(exist_ok=True, parents=True)
        ts = time.strftime("%Y%m%d-%H%M%S")
        out = RESULTS_ROOT / f"bench-{sha}{'-dirty' if dirty else ''}-{ts}.json"
        out.write_text(json.dumps({
            "git_sha": sha,
            "git_dirty": dirty,
            "iters": args.iters,
            "warmup": args.warmup,
            "ts": ts,
            "python": sys.version,
            "results": results,
        }, indent=2))
        print(f"\nWrote {out.relative_to(BENCH_ROOT.parent)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
