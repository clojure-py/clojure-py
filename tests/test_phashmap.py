"""PersistentHashMap core — HAMT-backed immutable map."""

import pytest
from clojure._core import PersistentHashMap, hash_map, keyword


def test_empty():
    m = hash_map()
    assert isinstance(m, PersistentHashMap)
    assert len(m) == 0


def test_assoc_single():
    m = hash_map().assoc("k", 1)
    assert len(m) == 1
    assert m.val_at("k") == 1


def test_assoc_many():
    m = hash_map()
    for i in range(100):
        m = m.assoc(f"k{i}", i)
    assert len(m) == 100
    for i in range(100):
        assert m.val_at(f"k{i}") == i


def test_assoc_replaces_existing():
    m = hash_map().assoc("k", 1).assoc("k", 2)
    assert len(m) == 1
    assert m.val_at("k") == 2


def test_val_at_missing_returns_nil():
    m = hash_map().assoc("k", 1)
    assert m.val_at("missing") is None


def test_val_at_missing_with_default():
    m = hash_map().assoc("k", 1)
    assert m.val_at_default("missing", "nope") == "nope"


def test_without_removes():
    m = hash_map().assoc("k", 1).assoc("j", 2)
    m2 = m.without("k")
    assert len(m2) == 1
    assert m2.val_at("k") is None
    assert m2.val_at("j") == 2


def test_without_missing_noop():
    m = hash_map().assoc("k", 1)
    m2 = m.without("missing")
    assert len(m2) == 1
    assert m2.val_at("k") == 1


def test_contains_key():
    m = hash_map().assoc("k", 1)
    assert m.contains_key("k") is True
    assert m.contains_key("missing") is False


def test_nil_key():
    m = hash_map().assoc(None, "nil-val")
    assert m.val_at(None) == "nil-val"
    assert m.contains_key(None) is True
    m2 = m.without(None)
    assert m2.contains_key(None) is False


def test_keyword_keys():
    m = hash_map().assoc(keyword("a"), 1).assoc(keyword("b"), 2)
    assert m.val_at(keyword("a")) == 1
    assert m.val_at(keyword("b")) == 2


def test_iteration_yields_keys():
    m = hash_map().assoc("a", 1).assoc("b", 2).assoc("c", 3)
    keys = set(iter(m))
    assert keys == {"a", "b", "c"}


def test_getitem():
    m = hash_map().assoc("k", 42)
    assert m["k"] == 42


def test_getitem_missing_raises():
    m = hash_map().assoc("k", 1)
    with pytest.raises(KeyError):
        _ = m["missing"]


def test_contains_via_in():
    m = hash_map().assoc("k", 1)
    assert "k" in m
    assert "missing" not in m


def test_structural_sharing():
    """Deriving v2 from v1 must leave v1 unchanged."""
    m1 = hash_map()
    for i in range(100):
        m1 = m1.assoc(i, i)
    m2 = m1.without(50)
    assert len(m1) == 100
    assert m1.val_at(50) == 50
    assert len(m2) == 99
    assert m2.val_at(50) is None


def test_hash_collision_handled():
    """Keys with the same hash_eq but different identity must coexist."""
    class SameHash:
        def __init__(self, name): self.name = name
        def __hash__(self): return 42
        def __eq__(self, other): return isinstance(other, SameHash) and self.name == other.name
    a = SameHash("a")
    b = SameHash("b")
    m = hash_map().assoc(a, 1).assoc(b, 2)
    assert len(m) == 2
    assert m.val_at(a) == 1
    assert m.val_at(b) == 2


def test_deep_insertion():
    """Insert 2000 entries — exercises multi-level HAMT."""
    m = hash_map()
    for i in range(2000):
        m = m.assoc(i, i * 2)
    assert len(m) == 2000
    for i in range(2000):
        assert m.val_at(i) == i * 2


def test_repr_contains_entries():
    m = hash_map().assoc("a", 1)
    r = repr(m)
    assert r.startswith("{")
    assert r.endswith("}")
    assert "a" in r
    assert "1" in r
