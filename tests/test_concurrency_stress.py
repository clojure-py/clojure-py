"""Integration stress test — concurrent extend_type + dispatch on 3.14t.

Smoke-only; catches gross races (deadlocks, torn reads, exceptions). Loom
tests (crates/clojure_core/tests/loom_*.rs) cover exhaustive interleavings
of the Rust-only primitives; this test exercises the full stack from
Python under real threading on the free-threaded build.
"""

import time
import pytest
from concurrent.futures import ThreadPoolExecutor
from clojure._core import IFn, invoke1


class T1: pass
class T2(T1): pass
class T3(T2): pass


@pytest.mark.timeout(30)
def test_concurrent_extend_and_dispatch():
    deadline = time.monotonic() + 10.0
    errors: list[str] = []

    def extend_worker():
        n = 0
        while time.monotonic() < deadline:
            try:
                IFn.extend_type(T1, {"invoke1": lambda s, a: ("T1", a)})
                IFn.extend_type(T2, {"invoke1": lambda s, a: ("T2", a)})
                IFn.extend_type(T3, {"invoke1": lambda s, a: ("T3", a)})
            except Exception as e:
                errors.append(f"extend: {e!r}")
            n += 1
        return n

    def dispatch_worker():
        n = 0
        while time.monotonic() < deadline:
            for obj in (T1(), T2(), T3()):
                try:
                    result = invoke1(obj, n)
                    # During re-extend the impl may return any tagged variant
                    # for the object's type — all are OK as long as we got
                    # (str, int) back.
                    assert isinstance(result, tuple)
                    assert len(result) == 2
                    assert isinstance(result[0], str)
                    assert result[1] == n
                except Exception as e:
                    errors.append(f"dispatch: {e!r}")
            n += 1
        return n

    with ThreadPoolExecutor(max_workers=32) as ex:
        extend_futs = [ex.submit(extend_worker) for _ in range(4)]
        dispatch_futs = [ex.submit(dispatch_worker) for _ in range(28)]
        for f in extend_futs + dispatch_futs:
            f.result()

    assert not errors, f"{len(errors)} error(s); first 5: {errors[:5]}"


@pytest.mark.timeout(30)
def test_concurrent_keyword_intern_large():
    """Under concurrent intern of many distinct keys across threads, every
    thread that interns the same name must receive the same Py<Keyword>
    instance (pointer identity)."""
    from clojure._core import keyword

    def worker(seed: int):
        out = []
        for i in range(2000):
            out.append(keyword(f"k{i % 50}"))
        return out

    with ThreadPoolExecutor(max_workers=16) as ex:
        results = list(ex.map(worker, range(16)))

    # For each of the 50 key names, gather all returned instances across threads
    # and confirm they're all the same Python object.
    by_name: dict[str, object] = {}
    for r in results:
        for kw in r:
            existing = by_name.setdefault(kw.name, kw)
            assert existing is kw, f"intern returned two distinct objects for :{kw.name}"


@pytest.mark.timeout(30)
def test_concurrent_var_alter_root():
    """Two threads racing alter-var-root increments must converge to the
    correct total (CAS retry loop under contention)."""
    import sys
    import types
    from clojure._core import Var, symbol

    m = types.ModuleType("stress.var")
    sys.modules["stress.var"] = m
    v = Var(m, symbol("counter"))
    v.bind_root(0)

    ITERATIONS = 500

    def worker():
        for _ in range(ITERATIONS):
            v.alter_root(lambda x: x + 1)

    with ThreadPoolExecutor(max_workers=8) as ex:
        futs = [ex.submit(worker) for _ in range(8)]
        for f in futs:
            f.result()

    assert v.deref() == ITERATIONS * 8
