"""Tests for Atom (CAS-backed) and Volatile.

Atom correctness under concurrency is the main thing to exercise — the whole
point of the ArcSwap-based refactor is that swap! should compose atomically
across threads."""

import threading
import pytest
from clojure._core import eval_string, Atom, Volatile, IllegalArgumentException


def _ev(src):
    return eval_string(src)


# --- Atom basics ---

def test_atom_deref():
    a = _ev("(atom 42)")
    assert isinstance(a, Atom)
    assert _ev("(deref (atom 42))") == 42


def test_atom_reader_deref():
    assert _ev("(let* [a (atom :x)] @a)") == _ev(":x")


def test_reset_bang():
    assert _ev("(let* [a (atom 0)] (reset! a 99))") == 99


def test_reset_vals_bang():
    r = _ev("(let* [a (atom 5)] (reset-vals! a 10))")
    assert list(r) == [5, 10]


def test_swap_bang_no_extra_args():
    assert _ev("(let* [a (atom 5)] (swap! a inc))") == 6


def test_swap_bang_with_extra_args():
    assert _ev("(let* [a (atom 10)] (swap! a + 1 2 3))") == 16


def test_swap_vals_bang():
    r = _ev("(let* [a (atom 0)] (swap-vals! a inc))")
    assert list(r) == [0, 1]


def test_compare_and_set_hit():
    assert _ev("(let* [a (atom 10)] (compare-and-set! a 10 20))") is True


def test_compare_and_set_miss():
    assert _ev("(let* [a (atom 10)] (compare-and-set! a 99 20))") is False


def test_compare_and_set_uses_value_equality():
    # Different PersistentVector instances that compare = by Clojure equality
    # should satisfy compare-and-set.
    r = _ev(
        "(let* [a (atom [1 2 3])]"
        "  (compare-and-set! a [1 2 3] [4 5 6]))"
    )
    assert r is True


# --- Validator ---

def test_reset_rejects_via_validator():
    import clojure._core as c
    a = c.atom(5)
    a.set_validator(lambda v: v > 0)
    with pytest.raises(IllegalArgumentException):
        c.find_ns(c.symbol("clojure.lang.RT")).__getattribute__("reset-bang")(a, -1)


# --- Watch ---

def test_add_and_fire_watch():
    import clojure._core as c
    a = c.atom(0)
    fired = []

    def w(key, ref, old, new):
        fired.append((key, old, new))

    a.add_watch("w1", w)
    getattr(c.find_ns(c.symbol("clojure.lang.RT")), "reset-bang")(a, 10)
    getattr(c.find_ns(c.symbol("clojure.lang.RT")), "reset-bang")(a, 20)
    assert fired == [("w1", 0, 10), ("w1", 10, 20)]


@pytest.mark.timeout(20)
def test_watches_concurrent_add_and_fire():
    """add_watch / remove_watch must not race with fire_watches' iteration.
    Under free-threaded 3.14t, running many add/remove/fire calls in parallel
    must not raise `RuntimeError: dictionary changed size during iteration`
    or any other concurrent-modification error."""
    import time
    import clojure._core as c
    from concurrent.futures import ThreadPoolExecutor

    a = c.atom(0)
    deadline = time.monotonic() + 5.0
    errors: list[str] = []

    def noop(_k, _r, _o, _n):
        pass

    def mutate_worker(tid):
        n = 0
        while time.monotonic() < deadline:
            try:
                key = f"w{tid}-{n % 16}"
                a.add_watch(key, noop)
                if n % 3 == 0:
                    a.remove_watch(key)
            except Exception as e:
                errors.append(f"mutate: {e!r}")
            n += 1

    reset_bang = getattr(c.find_ns(c.symbol("clojure.lang.RT")), "reset-bang")

    def fire_worker():
        n = 0
        while time.monotonic() < deadline:
            try:
                reset_bang(a, n)
            except Exception as e:
                errors.append(f"fire: {e!r}")
            n += 1

    with ThreadPoolExecutor(max_workers=16) as ex:
        mut_futs = [ex.submit(mutate_worker, i) for i in range(8)]
        fire_futs = [ex.submit(fire_worker) for _ in range(8)]
        for f in mut_futs + fire_futs:
            f.result()

    assert errors == [], f"concurrent watch errors: {errors[:5]}"


# --- Concurrent swap! stress ---

def test_swap_bang_concurrent_increment():
    """N threads each running swap! inc K times. Final value must be N*K.
    This is the canonical CAS-loop stress test."""
    import clojure._core as c
    a = c.atom(0)
    N_THREADS = 8
    K_ITERS = 200
    swap_bang = getattr(c.find_ns(c.symbol("clojure.lang.RT")), "swap-bang")

    def inc_py(x):
        return x + 1

    def worker():
        for _ in range(K_ITERS):
            swap_bang(a, inc_py)

    threads = [threading.Thread(target=worker) for _ in range(N_THREADS)]
    for t in threads: t.start()
    for t in threads: t.join()
    assert a.__class__.__name__ == "Atom"
    # Read via RT/deref.
    deref = getattr(c.find_ns(c.symbol("clojure.lang.RT")), "deref")
    assert deref(a) == N_THREADS * K_ITERS


# --- Volatile ---

def test_volatile_roundtrip():
    v = _ev("(volatile! 0)")
    assert isinstance(v, Volatile)


def test_volatile_deref():
    assert _ev("(let* [v (volatile! 42)] @v)") == 42


def test_vreset_bang():
    assert _ev("(let* [v (volatile! 0)] (vreset! v 99))") == 99


def test_vswap_bang():
    assert _ev("(let* [v (volatile! 5)] (vswap! v inc))") == 6
    assert _ev("(let* [v (volatile! 10)] (vswap! v + 1 2 3))") == 16


def test_volatile_pred():
    assert _ev("(volatile? (volatile! 1))") is True
    assert _ev("(volatile? (atom 1))") is False
    assert _ev("(volatile? 5)") is False


# --- reduced IDeref interop ---

def test_deref_on_reduced():
    # Reduced implements IDeref too — @(reduced x) returns x.
    assert _ev("@(reduced 42)") == 42


def test_deref_on_delay():
    # Delay already had force; IDeref dispatch should also work.
    assert _ev("(deref (delay 99))") == 99
