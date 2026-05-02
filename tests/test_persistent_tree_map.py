"""Tests for PersistentTreeMap (red-black tree)."""
import random
import pytest

from clojure.lang import (
    PersistentTreeMap, PERSISTENT_TREE_MAP_EMPTY,
    MapEntry, Reduced,
    IPersistentMap, Associative, ILookup, IPersistentCollection,
    Counted, IFn, IHashEq, IMeta, IObj, IKVReduce, Reversible, Sorted,
    Murmur3,
)


class TestConstruction:
    def test_empty(self):
        assert PERSISTENT_TREE_MAP_EMPTY.count() == 0

    def test_create(self):
        m = PersistentTreeMap.create(2, "two", 1, "one", 3, "three")
        assert m.count() == 3

    def test_create_odd_args_raises(self):
        with pytest.raises(ValueError):
            PersistentTreeMap.create(1, "one", 2)

    def test_create_with_comparator(self):
        # Reverse-order comparator
        rev = lambda a, b: -1 if a > b else (1 if a < b else 0)
        m = PersistentTreeMap.create_with_comparator(rev, 1, "a", 2, "b", 3, "c")
        assert list(m.keys()) == [3, 2, 1]

    def test_from_iterable_of_pairs(self):
        m = PersistentTreeMap.from_iterable([(2, "b"), (1, "a")])
        assert list(m.keys()) == [1, 2]


class TestBasicOps:
    def test_count(self):
        m = PersistentTreeMap.create(1, "a", 2, "b")
        assert m.count() == 2
        assert len(m) == 2

    def test_val_at(self):
        m = PersistentTreeMap.create(1, "a", 2, "b")
        assert m.val_at(1) == "a"
        assert m.val_at(99) is None
        assert m.val_at(99, "default") == "default"

    def test_contains_key(self):
        m = PersistentTreeMap.create(1, "a")
        assert m.contains_key(1)
        assert not m.contains_key(99)

    def test_entry_at(self):
        m = PersistentTreeMap.create(1, "a")
        e = m.entry_at(1)
        assert e.key() == 1 and e.val() == "a"

    def test_entry_at_missing(self):
        assert PersistentTreeMap.create(1, "a").entry_at(99) is None


class TestSorted:
    def test_keys_in_order(self):
        m = PersistentTreeMap.create(5, "e", 2, "b", 8, "h", 1, "a", 3, "c")
        assert list(m.keys()) == [1, 2, 3, 5, 8]

    def test_min_max_key(self):
        m = PersistentTreeMap.create(5, None, 1, None, 3, None)
        assert m.min_key() == 1
        assert m.max_key() == 5

    def test_min_max_empty(self):
        assert PERSISTENT_TREE_MAP_EMPTY.min_key() is None
        assert PERSISTENT_TREE_MAP_EMPTY.max_key() is None

    def test_rseq(self):
        m = PersistentTreeMap.create(1, "a", 2, "b", 3, "c")
        keys = [e.key() for e in m.rseq()]
        assert keys == [3, 2, 1]

    def test_seq_with_comparator_ascending(self):
        m = PersistentTreeMap.create(2, "b", 1, "a", 3, "c")
        keys = [e.key() for e in m.seq_with_comparator(True)]
        assert keys == [1, 2, 3]

    def test_seq_with_comparator_descending(self):
        m = PersistentTreeMap.create(2, "b", 1, "a", 3, "c")
        keys = [e.key() for e in m.seq_with_comparator(False)]
        assert keys == [3, 2, 1]

    def test_seq_from_present_key(self):
        m = PersistentTreeMap.create(1, "a", 2, "b", 3, "c", 4, "d", 5, "e")
        keys = [e.key() for e in m.seq_from(3, True)]
        assert keys == [3, 4, 5]

    def test_seq_from_absent_key(self):
        m = PersistentTreeMap.create(1, "a", 3, "c", 5, "e")
        # 2 isn't present; ascending starts at the next-greater key (3).
        keys = [e.key() for e in m.seq_from(2, True)]
        assert keys == [3, 5]

    def test_seq_from_descending(self):
        m = PersistentTreeMap.create(1, "a", 3, "c", 5, "e")
        keys = [e.key() for e in m.seq_from(4, False)]
        assert keys == [3, 1]

    def test_isinstance_sorted(self):
        m = PersistentTreeMap.create(1, "a")
        assert isinstance(m, Sorted)
        assert isinstance(m, Reversible)


class TestAssoc:
    def test_assoc_new(self):
        m = PersistentTreeMap.create(1, "a")
        m2 = m.assoc(2, "b")
        assert m2.count() == 2
        assert m.count() == 1   # original unchanged

    def test_assoc_replace(self):
        m = PersistentTreeMap.create(1, "a")
        assert m.assoc(1, "X").val_at(1) == "X"

    def test_assoc_same_value_returns_self(self):
        m = PersistentTreeMap.create(1, "a")
        assert m.assoc(1, "a") is m

    def test_assoc_ex_new(self):
        m = PersistentTreeMap.create(1, "a")
        assert m.assoc_ex(2, "b").val_at(2) == "b"

    def test_assoc_ex_existing_raises(self):
        m = PersistentTreeMap.create(1, "a")
        with pytest.raises(ValueError):
            m.assoc_ex(1, "X")


class TestWithout:
    def test_without_present(self):
        m = PersistentTreeMap.create(1, "a", 2, "b", 3, "c")
        m2 = m.without(2)
        assert m2.count() == 2
        assert not m2.contains_key(2)
        # Sortedness preserved.
        assert list(m2.keys()) == [1, 3]

    def test_without_missing_returns_self(self):
        m = PersistentTreeMap.create(1, "a")
        assert m.without(99) is m

    def test_without_to_empty(self):
        m = PersistentTreeMap.create(1, "a")
        assert m.without(1).count() == 0


# =========================================================================
# Stress test: random insert + remove
# =========================================================================

class TestStress:
    def test_random_inserts_yield_sorted_keys(self):
        random.seed(42)
        keys = list(range(500))
        random.shuffle(keys)
        m = PERSISTENT_TREE_MAP_EMPTY
        for k in keys:
            m = m.assoc(k, k * 10)
        assert m.count() == 500
        # Walking should yield keys 0..499 in order.
        assert list(m.keys()) == list(range(500))
        # Lookups all hit.
        for k in keys:
            assert m.val_at(k) == k * 10

    def test_random_inserts_then_removes(self):
        random.seed(42)
        keys = list(range(200))
        random.shuffle(keys)
        m = PERSISTENT_TREE_MAP_EMPTY
        for k in keys:
            m = m.assoc(k, k)
        # Remove half.
        random.shuffle(keys)
        for k in keys[:100]:
            m = m.without(k)
        assert m.count() == 100
        remaining = set(keys[100:])
        assert set(m.keys()) == remaining


# =========================================================================
# Custom comparator
# =========================================================================

class TestCustomComparator:
    def test_string_length_comparator(self):
        # Keys ordered by string length, then lexicographic. Note: PTM.create
        # takes alternating k/v, so each pair below is (key, value).
        def by_len(a, b):
            la, lb = len(a), len(b)
            if la != lb:
                return -1 if la < lb else 1
            if a == b:
                return 0
            return -1 if a < b else 1

        m = PersistentTreeMap.create_with_comparator(
            by_len,
            "ccc", 1,
            "x",   2,
            "aa",  3,
            "bb",  4,
            "zzzz", 5,
            "y",   6)
        # Keys sorted by length then lex: x, y (len 1), aa, bb (len 2),
        # ccc (len 3), zzzz (len 4).
        assert list(m.keys()) == ["x", "y", "aa", "bb", "ccc", "zzzz"]


# =========================================================================
# Equality and hash
# =========================================================================

class TestEquality:
    def test_equal_to_other_ptm(self):
        a = PersistentTreeMap.create(1, "a", 2, "b")
        b = PersistentTreeMap.create(2, "b", 1, "a")
        assert a == b

    def test_equal_to_dict(self):
        m = PersistentTreeMap.create(1, "a", 2, "b")
        assert m == {1: "a", 2: "b"}


class TestHash:
    def test_hashable(self):
        a = PersistentTreeMap.create(1, "a")
        b = PersistentTreeMap.create(1, "a")
        assert hash(a) == hash(b)


# =========================================================================
# kv_reduce
# =========================================================================

class TestKVReduce:
    def test_kv_reduce_visits_in_order(self):
        m = PersistentTreeMap.create(3, "c", 1, "a", 2, "b")
        seen = []
        m.kv_reduce(lambda acc, k, v: seen.append((k, v)) or acc, None)
        assert seen == [(1, "a"), (2, "b"), (3, "c")]

    def test_kv_reduce_short_circuit(self):
        m = PersistentTreeMap.create(1, "a", 2, "b", 3, "c")
        result = m.kv_reduce(
            lambda acc, k, v: Reduced(v) if k == 2 else acc, None)
        assert result == "b"


class TestEmpty:
    def test_empty_propagates_meta(self):
        m = PersistentTreeMap.create(1, "a").with_meta({"k": "v"})
        e = m.empty()
        assert e.count() == 0
        assert e.meta() == {"k": "v"}


class TestPython:
    def test_iter_yields_entries_in_order(self):
        m = PersistentTreeMap.create(2, "b", 1, "a", 3, "c")
        keys = [e.key() for e in m]
        assert keys == [1, 2, 3]

    def test_keys_values(self):
        m = PersistentTreeMap.create(3, "c", 1, "a", 2, "b")
        assert list(m.keys()) == [1, 2, 3]
        assert list(m.values()) == ["a", "b", "c"]

    def test_in_operator(self):
        m = PersistentTreeMap.create(1, "a")
        assert 1 in m
        assert 99 not in m

    def test_getitem(self):
        m = PersistentTreeMap.create(1, "a")
        assert m[1] == "a"
        with pytest.raises(KeyError):
            m[99]

    def test_callable(self):
        m = PersistentTreeMap.create(1, "a")
        assert m(1) == "a"


class TestInterfaces:
    def test_all_abcs(self):
        m = PersistentTreeMap.create(1, "a")
        assert isinstance(m, IPersistentMap)
        assert isinstance(m, Associative)
        assert isinstance(m, ILookup)
        assert isinstance(m, Counted)
        assert isinstance(m, IFn)
        assert isinstance(m, IKVReduce)
        assert isinstance(m, Reversible)
        assert isinstance(m, Sorted)


class TestNullValues:
    def test_null_values_supported(self):
        m = PersistentTreeMap.create(1, None, 2, "b")
        assert m.val_at(1) is None
        assert m.contains_key(1)
        # entry_at distinguishes from missing
        assert m.entry_at(1) is not None
        assert m.entry_at(99) is None
