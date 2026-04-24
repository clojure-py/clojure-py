"""TransientHashMap — mutable-in-place variant of PersistentHashMap."""

import pytest
import threading
from clojure._core import (
    hash_map, PersistentHashMap, TransientHashMap,
    transient, persistent_bang, conj_bang, assoc_bang, dissoc_bang,
    IllegalStateException,
)


def test_transient_round_trip():
    m = hash_map().assoc("a", 1).assoc("b", 2)
    t = transient(m)
    assert isinstance(t, TransientHashMap)
    m2 = persistent_bang(t)
    assert isinstance(m2, PersistentHashMap)
    assert m2.val_at("a") == 1
    assert m2.val_at("b") == 2


def test_assoc_bang_many():
    t = transient(hash_map())
    for i in range(100):
        assoc_bang(t, f"k{i}", i)
    m = persistent_bang(t)
    assert len(m) == 100
    for i in range(100):
        assert m.val_at(f"k{i}") == i


def test_assoc_bang_replaces():
    t = transient(hash_map().assoc("a", 1))
    assoc_bang(t, "a", 99)
    m = persistent_bang(t)
    assert len(m) == 1
    assert m.val_at("a") == 99


def test_dissoc_bang():
    m = hash_map().assoc("a", 1).assoc("b", 2).assoc("c", 3)
    t = transient(m)
    dissoc_bang(t, "b")
    m2 = persistent_bang(t)
    assert len(m2) == 2
    assert m2.val_at("b") is None


def test_dissoc_bang_missing_noop():
    t = transient(hash_map().assoc("a", 1))
    dissoc_bang(t, "nonexistent")
    m = persistent_bang(t)
    assert len(m) == 1
    assert m.val_at("a") == 1


def test_use_after_persistent_bang_raises():
    t = transient(hash_map())
    persistent_bang(t)
    with pytest.raises(IllegalStateException):
        assoc_bang(t, "k", 1)


def test_cross_thread_use_allowed():
    """Matches Clojure JVM post-CLJ-1613: transients do NOT enforce
    thread ownership. Callers are responsible for synchronization (e.g.
    `future`'s @deref happens-before) when handing a transient across
    threads. Our check only fires on use-after-persistent!."""
    t = transient(hash_map())
    err_box = []
    def worker():
        try:
            assoc_bang(t, "k", 1)
        except Exception as e:
            err_box.append(type(e).__name__)
    th = threading.Thread(target=worker)
    th.start()
    th.join()
    assert err_box == []


def test_stress_2000_then_persistent():
    t = transient(hash_map())
    for i in range(2000):
        assoc_bang(t, i, i * 2)
    m = persistent_bang(t)
    assert len(m) == 2000
    for i in range(2000):
        assert m.val_at(i) == i * 2


def test_transient_preserves_original():
    m = hash_map().assoc("a", 1).assoc("b", 2)
    t = transient(m)
    assoc_bang(t, "c", 3)
    # Original unchanged.
    assert len(m) == 2
    assert m.val_at("c") is None


def test_nil_key_in_transient():
    t = transient(hash_map())
    assoc_bang(t, None, "nil-val")
    m = persistent_bang(t)
    assert m.val_at(None) == "nil-val"


def test_dissoc_bang_nil_key():
    m = hash_map().assoc(None, "x").assoc("a", 1)
    t = transient(m)
    dissoc_bang(t, None)
    m2 = persistent_bang(t)
    assert m2.val_at(None) is None
    assert m2.val_at("a") == 1
