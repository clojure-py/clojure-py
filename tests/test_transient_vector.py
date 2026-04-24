"""TransientVector — mutable-in-place variant of PersistentVector."""

import pytest
import threading
from clojure._core import (
    vector, PersistentVector, TransientVector,
    transient, persistent_bang, conj_bang, assoc_bang, pop_bang,
    IllegalStateException,
)


def test_transient_round_trip():
    v = vector(1, 2, 3)
    t = transient(v)
    assert isinstance(t, TransientVector)
    v2 = persistent_bang(t)
    assert isinstance(v2, PersistentVector)
    assert list(v2) == [1, 2, 3]


def test_conj_bang_many():
    t = transient(vector())
    for i in range(100):
        conj_bang(t, i)
    v = persistent_bang(t)
    assert len(v) == 100
    for i in range(100):
        assert v.nth(i) == i


def test_assoc_bang():
    t = transient(vector(1, 2, 3))
    assoc_bang(t, 1, 99)
    v = persistent_bang(t)
    assert v.nth(1) == 99


def test_pop_bang():
    t = transient(vector(1, 2, 3))
    pop_bang(t)
    v = persistent_bang(t)
    assert len(v) == 2
    assert list(v) == [1, 2]


def test_use_after_persistent_bang_raises():
    t = transient(vector(1, 2, 3))
    persistent_bang(t)
    with pytest.raises(IllegalStateException):
        conj_bang(t, 99)


def test_transient_cross_thread_use_allowed():
    """Matches Clojure JVM post-CLJ-1613: transients do NOT enforce thread
    ownership. Users handing a transient across threads must provide their
    own synchronization (typically via `future`'s @deref happens-before).
    Our check only fires on use-after-persistent!."""
    t = transient(vector(1, 2, 3))
    err_box = []
    def worker():
        try:
            conj_bang(t, 99)
        except Exception as e:
            err_box.append(type(e).__name__)
    th = threading.Thread(target=worker)
    th.start()
    th.join()
    assert err_box == []


def test_persistent_after_many_ops():
    """Stress: 2000 conj! then persistent! produces correct vector."""
    t = transient(vector())
    for i in range(2000):
        conj_bang(t, i)
    v = persistent_bang(t)
    assert len(v) == 2000
    for i in range(2000):
        assert v.nth(i) == i


def test_transient_preserves_original():
    v = vector(1, 2, 3)
    t = transient(v)
    conj_bang(t, 4)
    # Original unchanged
    assert len(v) == 3


def test_assoc_bang_out_of_bounds():
    t = transient(vector(1, 2, 3))
    with pytest.raises(IndexError):
        assoc_bang(t, 10, 99)


def test_pop_bang_empty_raises():
    t = transient(vector())
    with pytest.raises(IllegalStateException):
        pop_bang(t)
