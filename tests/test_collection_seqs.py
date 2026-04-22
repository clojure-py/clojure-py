"""ISeqable on maps and sets — (seq m) / (seq s)."""

import pytest
from clojure._core import (
    hash_map, array_map, hash_set, seq, first, count, MapEntry,
)
from clojure._core import next as next_seq


def test_seq_empty_hashmap_is_nil():
    assert seq(hash_map()) is None


def test_seq_non_empty_hashmap_walks_entries():
    m = hash_map().assoc("a", 1).assoc("b", 2)
    s = seq(m)
    assert s is not None
    entries = []
    cur = s
    while cur is not None:
        e = first(cur)
        assert isinstance(e, MapEntry)
        entries.append((e.key, e.val))
        cur = next_seq(cur)
    assert set(entries) == {("a", 1), ("b", 2)}


def test_seq_array_map_walks_entries():
    m = array_map().assoc("x", 10).assoc("y", 20)
    s = seq(m)
    entries = []
    cur = s
    while cur is not None:
        e = first(cur)
        entries.append((e.key, e.val))
        cur = next_seq(cur)
    assert set(entries) == {("x", 10), ("y", 20)}


def test_seq_empty_hashset_is_nil():
    assert seq(hash_set()) is None


def test_seq_non_empty_hashset_walks_values():
    s_coll = hash_set("a", "b", "c")
    s = seq(s_coll)
    values = []
    cur = s
    while cur is not None:
        values.append(first(cur))
        cur = next_seq(cur)
    assert set(values) == {"a", "b", "c"}


def test_seq_hashmap_count_matches():
    m = hash_map()
    for i in range(50):
        m = m.assoc(i, i * 2)
    s = seq(m)
    assert count(s) == 50


def test_seq_over_large_map():
    """Stress: 500 entries."""
    m = hash_map()
    for i in range(500):
        m = m.assoc(i, i)
    s = seq(m)
    collected = {}
    cur = s
    while cur is not None:
        e = first(cur)
        collected[e.key] = e.val
        cur = next_seq(cur)
    assert len(collected) == 500
    for i in range(500):
        assert collected[i] == i
