"""Software Transactional Memory (Ref) tests.

Covers the full surface: ref/deref inside and outside txns, ref-set / alter /
commute / ensure, `sync`/`dosync`/`io!`, history growth, validators, and
concurrent retry under conflict.

Many tests use `(def --name (ref ...))` to create a named var that subsequent
`eval_string` calls (and worker threads) can reference by name.
"""

import threading
import pytest

from clojure._core import (
    eval_string,
    Ref,
    IllegalStateException,
    IllegalArgumentException,
)


def _ev(s):
    return eval_string(s)


# --- Basic txn ops ---


def test_dosync_returns_body_value():
    assert _ev("(dosync 42)") == 42


def test_sync_nil_returns_body_value():
    assert _ev("(sync nil 42)") == 42


def test_ref_set_requires_txn():
    with pytest.raises(IllegalStateException):
        _ev("(ref-set (ref 1) 2)")


def test_alter_requires_txn():
    with pytest.raises(IllegalStateException):
        _ev("(alter (ref 1) inc)")


def test_commute_requires_txn():
    with pytest.raises(IllegalStateException):
        _ev("(commute (ref 1) inc)")


def test_ensure_requires_txn():
    with pytest.raises(IllegalStateException):
        _ev("(ensure (ref 1))")


def test_ref_set_persists_after_commit():
    assert _ev("(let* [r (ref 0)] (dosync (ref-set r 99)) @r)") == 99


def test_alter_applies_fn():
    assert _ev("(let* [r (ref 0)] (dosync (alter r inc)) @r)") == 1


def test_alter_multiple_in_same_dosync():
    assert _ev("(let* [r (ref 0)] (dosync (alter r inc) (alter r inc) (alter r inc)) @r)") == 3


def test_alter_with_args():
    assert _ev("(let* [r (ref 10)] (dosync (alter r + 5 6)) @r)") == 21


def test_commute_applies_and_commits():
    assert _ev("(let* [r (ref 0)] (dosync (commute r inc) (commute r inc)) @r)") == 2


def test_commute_with_args():
    assert _ev("(let* [r (ref 10)] (dosync (commute r + 5)) @r)") == 15


def test_ensure_returns_in_txn_value():
    assert _ev("(let* [r (ref 42)] (dosync (ensure r)))") == 42


def test_ensure_then_alter_same_txn():
    assert _ev("(let* [r (ref 1)] (dosync (ensure r) (alter r inc)) @r)") == 2


def test_cannot_set_after_commute_raises():
    with pytest.raises(IllegalStateException):
        _ev("(let* [r (ref 0)] (dosync (commute r inc) (ref-set r 99)))")


def test_in_txn_read_sees_own_writes():
    assert _ev("(let* [r (ref 0)] (dosync (ref-set r 5) @r))") == 5


def test_alter_chain_sees_previous_in_txn_value():
    assert _ev("(let* [r (ref 0)] (dosync (alter r inc) (alter r inc) @r))") == 2


def test_nested_sync_reuses_txn():
    # Inner dosync must run under the outer txn; both alters stick.
    result = _ev(
        "(let* [r (ref 0)] (dosync (alter r inc) (dosync (alter r inc))) @r)"
    )
    assert result == 2


def test_txn_aborts_on_exception_no_side_effects():
    _ev("(def --stm-abort-r (ref 0))")
    with pytest.raises(Exception):
        _ev("(dosync (alter --stm-abort-r inc) (throw (ex-info \"x\" {})))")
    assert _ev("@--stm-abort-r") == 0


# --- io! ---


def test_io_bang_outside_txn():
    assert _ev("(io! :ok)") == _ev(":ok")


def test_io_bang_inside_dosync_throws():
    with pytest.raises(IllegalStateException):
        _ev("(dosync (io! :bad))")


def test_io_bang_inside_nested_dosync_throws():
    with pytest.raises(IllegalStateException):
        _ev("(dosync (dosync (io! :bad)))")


# --- History / min-max ---


def test_ref_history_count_starts_at_one():
    r = _ev("(ref 42)")
    assert r.history_count() == 1


def test_history_grows_on_commit():
    result = _ev(
        "(let* [r (ref 0)] (dosync (alter r inc)) (dosync (alter r inc)) "
        "  [(ref-history-count r) @r])"
    )
    assert list(result) == [3, 2]


def test_max_history_caps_growth():
    final = _ev(
        "(let* [r (ref 0)] "
        "  (ref-max-history r 3) "
        "  (dotimes [_ 10] (dosync (alter r inc))) "
        "  [(ref-history-count r) @r])"
    )
    # History capped at max-history = 3; value is still 10 after 10 increments.
    assert list(final) == [3, 10]


def test_ref_min_history_get_and_set():
    r = _ev("(let* [r (ref 0)] (ref-min-history r 5) r)")
    assert r.min_history == 5


def test_ref_max_history_get_and_set():
    r = _ev("(let* [r (ref 0)] (ref-max-history r 20) r)")
    assert r.max_history == 20


# --- Validator ---


def test_validator_rejects_at_commit():
    with pytest.raises(IllegalArgumentException):
        _ev("(let* [r (ref 1 :validator pos?)] (dosync (ref-set r -5)))")


def test_validator_accepts_valid():
    assert _ev("(let* [r (ref 1 :validator pos?)] (dosync (ref-set r 5)) @r)") == 5


def test_ref_meta_option():
    r = _ev("(ref 1 :meta {:k 7})")
    assert r.meta is not None


# --- Concurrency: retry under conflict ---


def test_retry_under_conflict_produces_correct_result():
    # Four threads each alter the same ref N times. Final value must be
    # starting_value + total_alters — no lost updates.
    _ev("(def --stm-counter (ref 0))")

    def worker(n):
        for _ in range(n):
            eval_string("(dosync (alter --stm-counter inc))")

    threads = [threading.Thread(target=worker, args=(200,)) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert _ev("@--stm-counter") == 800


def test_commute_concurrent_no_lost_updates():
    _ev("(def --stm-ctr2 (ref 0))")

    def worker(n):
        for _ in range(n):
            eval_string("(dosync (commute --stm-ctr2 inc))")

    threads = [threading.Thread(target=worker, args=(100,)) for _ in range(3)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert _ev("@--stm-ctr2") == 300


def test_commute_commutative_ordering_consistent():
    # Commute with +; final value is correct regardless of interleaving.
    _ev("(def --stm-ctr3 (ref 0))")

    def worker(ns):
        for n in ns:
            eval_string("(dosync (commute --stm-ctr3 + %d))" % n)

    expected = sum(range(1, 31)) * 3  # three threads each add 1..30
    threads = [threading.Thread(target=worker, args=(list(range(1, 31)),)) for _ in range(3)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert _ev("@--stm-ctr3") == expected
