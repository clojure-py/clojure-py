"""Compare two benchmark result JSON files.

    python bench/diff.py bench/results/before.json bench/results/after.json

Prints a per-bench table with the median delta (%), highlighting
regressions (>5% slower) and improvements (>5% faster).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def fmt_ns(ns: int) -> str:
    if ns < 1_000:
        return f"{ns:d}ns"
    if ns < 1_000_000:
        return f"{ns / 1_000:.2f}µs"
    if ns < 1_000_000_000:
        return f"{ns / 1_000_000:.2f}ms"
    return f"{ns / 1_000_000_000:.3f}s"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("before", type=Path)
    ap.add_argument("after", type=Path)
    ap.add_argument("--threshold", type=float, default=5.0,
                    help="Percent change considered significant (default 5.0)")
    args = ap.parse_args()

    a = json.loads(args.before.read_text())
    b = json.loads(args.after.read_text())
    ar = a.get("results", a)  # tolerate both raw-results and full-run JSON
    br = b.get("results", b)

    all_names = sorted(set(ar) | set(br))
    name_w = max((len(n) for n in all_names), default=0) + 2
    print(f"# {args.before}  →  {args.after}")
    print(f"#   {a.get('git_sha','?')}{'*' if a.get('git_dirty') else ''}  →  "
          f"{b.get('git_sha','?')}{'*' if b.get('git_dirty') else ''}")
    print(f"\n  {'name':<{name_w}}  {'before':>10}  {'after':>10}  {'Δ':>8}  status")

    worse = better = same = 0
    for name in all_names:
        ae = ar.get(name, {})
        be = br.get(name, {})
        am = ae.get("median_ns")
        bm = be.get("median_ns")
        if am is None or bm is None:
            note = (f"{'MISSING in before':>10}" if am is None
                    else f"{'MISSING in after':>10}")
            print(f"  {name:<{name_w}}  {note}")
            continue
        pct = (bm - am) / am * 100.0 if am else 0.0
        if pct > args.threshold:
            status = "SLOWER"; worse += 1
        elif pct < -args.threshold:
            status = "faster"; better += 1
        else:
            status = "--"; same += 1
        print(f"  {name:<{name_w}}  {fmt_ns(am):>10}  {fmt_ns(bm):>10}  "
              f"{pct:+7.1f}%  {status}")

    print(f"\nSummary: {better} faster, {worse} slower, {same} unchanged "
          f"(threshold ±{args.threshold:.1f}%)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
