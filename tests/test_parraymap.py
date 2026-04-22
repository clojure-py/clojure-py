"""PersistentArrayMap — flat-array small map with PHashMap promotion."""

import pytest
from clojure._core import (
    PersistentArrayMap, PersistentHashMap, TransientArrayMap, TransientHashMap,
    array_map, hash_map, keyword,
    transient, persistent_bang, assoc_bang, dissoc_bang,
)


def test_empty():
    m = array_map()
    assert isinstance(m, PersistentArrayMap)
    assert len(m) == 0


def test_assoc_small():
    m = array_map().assoc("a", 1).assoc("b", 2)
    assert isinstance(m, PersistentArrayMap)
    assert len(m) == 2
    assert m.val_at("a") == 1
    assert m.val_at("b") == 2


def test_assoc_replaces_existing():
    m = array_map().assoc("a", 1).assoc("a", 2)
    assert len(m) == 1
    assert m.val_at("a") == 2


def test_without_removes():
    m = array_map().assoc("a", 1).assoc("b", 2)
    m2 = m.without("a")
    assert len(m2) == 1
    assert m2.val_at("a") is None


def test_without_missing_noop():
    m = array_map().assoc("a", 1)
    m2 = m.without("missing")
    assert len(m2) == 1


def test_contains_key():
    m = array_map().assoc("a", 1)
    assert m.contains_key("a") is True
    assert m.contains_key("missing") is False


def test_nil_key():
    m = array_map().assoc(None, "nil-val")
    assert m.val_at(None) == "nil-val"
    assert m.contains_key(None) is True


def test_promotes_to_hashmap_past_threshold():
    m = array_map()
    for i in range(8):
        m = m.assoc(f"k{i}", i)
    assert isinstance(m, PersistentArrayMap)  # still at threshold
    m = m.assoc("k8", 8)
    # Now promoted:
    assert isinstance(m, PersistentHashMap)
    assert len(m) == 9
    for i in range(9):
        assert m.val_at(f"k{i}") == i


def test_val_at_default():
    m = array_map().assoc("a", 1)
    assert m.val_at_default("missing", "default") == "default"
    assert m.val_at_default("a", "default") == 1


def test_iteration_yields_keys():
    m = array_map().assoc("a", 1).assoc("b", 2)
    keys = set(iter(m))
    assert keys == {"a", "b"}


def test_getitem():
    m = array_map().assoc("k", 42)
    assert m["k"] == 42


def test_getitem_missing_raises():
    m = array_map().assoc("k", 1)
    with pytest.raises(KeyError):
        _ = m["missing"]


def test_contains_via_in():
    m = array_map().assoc("k", 1)
    assert "k" in m
    assert "missing" not in m


def test_array_map_callable_ifn():
    m = array_map().assoc("a", 1)
    assert m("a") == 1
    assert m("missing") is None
    assert m("missing", "default") == "default"


def test_repr():
    m = array_map().assoc("a", 1)
    r = repr(m)
    assert r.startswith("{")
    assert r.endswith("}")


# --- Transient array map ---

def test_transient_round_trip():
    m = array_map().assoc("a", 1)
    t = transient(m)
    assert isinstance(t, TransientArrayMap)
    m2 = persistent_bang(t)
    assert isinstance(m2, PersistentArrayMap)
    assert m2.val_at("a") == 1


def test_transient_assoc_bang_promotes_to_hashmap():
    """Building past 8 entries transiently promotes the transient to a
    TransientHashMap; persistent_bang yields a PersistentHashMap."""
    t = transient(array_map())
    for i in range(20):
        t = assoc_bang(t, f"k{i}", i)   # return value may shift type mid-build
    m = persistent_bang(t)
    assert isinstance(m, PersistentHashMap)
    assert len(m) == 20


def test_transient_dissoc_bang():
    m = array_map().assoc("a", 1).assoc("b", 2)
    t = transient(m)
    t = dissoc_bang(t, "a")
    m2 = persistent_bang(t)
    assert len(m2) == 1
    assert m2.val_at("a") is None
