"""Tests for Atom and Volatile."""
import threading
import pytest

from clojure.lang import (
    Atom, Volatile, PersistentVector,
    IAtom, IAtom2, IRef, IDeref, IReference, IMeta,
)


# =========================================================================
# Atom — basic operations
# =========================================================================

class TestAtomBasics:
    def test_initial_deref(self):
        assert Atom(42).deref() == 42

    def test_swap_no_extra_args(self):
        a = Atom(0)
        result = a.swap(lambda x: x + 1)
        assert result == 1
        assert a.deref() == 1

    def test_swap_with_args(self):
        a = Atom(10)
        assert a.swap(lambda x, n: x + n, 5) == 15
        assert a.swap(lambda x, n, m: x * n + m, 2, 1) == 31

    def test_reset(self):
        a = Atom(0)
        result = a.reset(99)
        assert result == 99
        assert a.deref() == 99

    def test_compare_and_set_success(self):
        a = Atom(1)
        assert a.compare_and_set(1, 2) is True
        assert a.deref() == 2

    def test_compare_and_set_failure(self):
        a = Atom(1)
        assert a.compare_and_set(99, 2) is False
        assert a.deref() == 1   # unchanged


# =========================================================================
# *vals variants — return [old, new] vector
# =========================================================================

class TestSwapVals:
    def test_swap_vals_returns_old_and_new(self):
        a = Atom(5)
        result = a.swap_vals(lambda x: x * 2)
        assert isinstance(result, PersistentVector)
        assert result.nth(0) == 5
        assert result.nth(1) == 10
        assert a.deref() == 10

    def test_reset_vals_returns_old_and_new(self):
        a = Atom("orig")
        result = a.reset_vals("new")
        assert isinstance(result, PersistentVector)
        assert result.nth(0) == "orig"
        assert result.nth(1) == "new"


# =========================================================================
# Validators
# =========================================================================

class TestValidator:
    def test_validator_rejects_invalid_swap(self):
        a = Atom(0)
        a.set_validator(lambda v: v >= 0)
        # Valid: 0 → 5
        assert a.swap(lambda x: x + 5) == 5
        # Invalid: 5 → -1, validator rejects
        with pytest.raises(RuntimeError):
            a.swap(lambda x: x - 100)
        # Atom unchanged after rejection.
        assert a.deref() == 5

    def test_validator_rejects_invalid_reset(self):
        a = Atom(0)
        a.set_validator(lambda v: isinstance(v, int))
        with pytest.raises(RuntimeError):
            a.reset("not an int")

    def test_set_validator_validates_current_value(self):
        # Setting a validator that the current value fails should raise.
        a = Atom(-5)
        with pytest.raises(RuntimeError):
            a.set_validator(lambda v: v > 0)


# =========================================================================
# Watches
# =========================================================================

class TestWatches:
    def test_watch_called_on_swap(self):
        a = Atom(0)
        seen = []
        def watcher(key, ref, old, new):
            seen.append((key, old, new))
        a.add_watch("w1", watcher)
        a.swap(lambda x: x + 1)
        assert seen == [("w1", 0, 1)]

    def test_watch_called_on_reset(self):
        a = Atom(0)
        events = []
        a.add_watch("w", lambda k, r, o, n: events.append((o, n)))
        a.reset(99)
        assert events == [(0, 99)]

    def test_watch_not_called_on_failed_cas(self):
        a = Atom(1)
        events = []
        a.add_watch("w", lambda k, r, o, n: events.append((o, n)))
        a.compare_and_set(99, 2)   # fails
        assert events == []

    def test_remove_watch(self):
        a = Atom(0)
        events = []
        a.add_watch("w", lambda k, r, o, n: events.append((o, n)))
        a.remove_watch("w")
        a.reset(1)
        assert events == []

    def test_multiple_watches(self):
        a = Atom(0)
        seen_a = []
        seen_b = []
        a.add_watch("a", lambda k, r, o, n: seen_a.append(n))
        a.add_watch("b", lambda k, r, o, n: seen_b.append(n))
        a.reset(42)
        assert seen_a == [42]
        assert seen_b == [42]


# =========================================================================
# Meta (inherited from ARef → AReference)
# =========================================================================

class TestMeta:
    def test_meta_initially_none(self):
        assert Atom(0).meta() is None

    def test_meta_via_ctor(self):
        a = Atom(0, {"k": "v"})
        assert a.meta() == {"k": "v"}

    def test_reset_meta(self):
        a = Atom(0)
        a.reset_meta({"new": True})
        assert a.meta() == {"new": True}


# =========================================================================
# Concurrent CAS — verify spin-loop correctness under contention
# =========================================================================

class TestConcurrent:
    def test_many_threads_increment(self):
        # 10 threads × 1000 increments each → final value should be 10_000.
        # If CAS races weren't handled, we'd see lost updates.
        a = Atom(0)
        n_threads = 10
        per_thread = 1000

        def worker():
            for _ in range(per_thread):
                a.swap(lambda x: x + 1)

        threads = [threading.Thread(target=worker) for _ in range(n_threads)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        assert a.deref() == n_threads * per_thread

    def test_swap_function_called_multiple_times_under_contention(self):
        # If concurrent swaps race, the swap function MAY be invoked more
        # than `n_threads × per_thread` times — that's the spin-loop's
        # cost. We don't measure exact retry count but confirm correctness.
        a = Atom(0)
        call_count = [0]
        lock = threading.Lock()

        def f(x):
            with lock:
                call_count[0] += 1
            return x + 1

        threads = [threading.Thread(target=lambda: a.swap(f)) for _ in range(20)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # Under contention, call_count >= 20 (some retries possible).
        assert a.deref() == 20
        assert call_count[0] >= 20


# =========================================================================
# ABC registration
# =========================================================================

class TestAtomInterfaces:
    def test_isinstance(self):
        a = Atom(0)
        assert isinstance(a, IAtom)
        assert isinstance(a, IAtom2)
        assert isinstance(a, IRef)
        assert isinstance(a, IDeref)
        assert isinstance(a, IReference)
        assert isinstance(a, IMeta)


# =========================================================================
# Volatile
# =========================================================================

class TestVolatile:
    def test_initial_deref(self):
        assert Volatile("x").deref() == "x"

    def test_reset(self):
        v = Volatile(0)
        assert v.reset(99) == 99
        assert v.deref() == 99

    def test_no_validator_no_watches(self):
        # Volatile is intentionally bare — has no add_watch / set_validator.
        v = Volatile(0)
        assert not hasattr(v, "set_validator")
        assert not hasattr(v, "add_watch")

    def test_isinstance_ideref(self):
        assert isinstance(Volatile(0), IDeref)

    def test_str(self):
        assert "Volatile" in str(Volatile("x"))


class TestVolatileConcurrent:
    def test_writes_dont_corrupt(self):
        # Volatile doesn't guarantee CAS — concurrent reset may race so the
        # final value is one of the written ones, but reads/writes don't
        # corrupt the cell or segfault under 3.14t.
        v = Volatile(None)

        def writer(value):
            for _ in range(1000):
                v.reset(value)

        threads = [
            threading.Thread(target=writer, args=(i,))
            for i in range(8)
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # Final value must be one of the writers' values.
        assert v.deref() in range(8)
