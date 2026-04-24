"""Tests for clojure.core/delay, delay?, force."""

import threading
from clojure._core import eval_string, Delay


def _ev(src):
    return eval_string(src)


def test_delay_produces_delay_instance():
    d = _ev("(delay 42)")
    assert isinstance(d, Delay)


def test_delay_predicate():
    assert _ev("(delay? (delay :x))") is True
    assert _ev("(delay? 42)") is False


def test_force_returns_value():
    assert _ev("(force (delay 42))") == 42


def test_force_on_non_delay_returns_input():
    assert _ev("(force 7)") == 7
    assert _ev("(force nil)") is None


def test_delay_evaluates_at_most_once():
    # A Python-visible counter — we build a callable the Delay body can invoke.
    import clojure._core as c
    counter = {"n": 0}

    def bump():
        counter["n"] += 1
        return counter["n"]

    # Build a Delay directly so we can control what the thunk does.
    d = c.delay(bump)
    # Multiple concurrent forces from different threads.
    results = []

    def worker():
        results.append(d.deref())

    threads = [threading.Thread(target=worker) for _ in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert counter["n"] == 1
    assert all(r == 1 for r in results)


def test_delay_realized_flag():
    import clojure._core as c
    d = c.delay(lambda: 99)
    assert d.realized() is False
    d.deref()
    assert d.realized() is True
