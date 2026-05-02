"""Tests for PersistentTreeSet."""
import pytest

from clojure.lang import (
    PersistentTreeSet, PERSISTENT_TREE_SET_EMPTY, PersistentTreeMap,
    IPersistentSet, IPersistentCollection, Counted, IFn, IHashEq,
    IMeta, IObj, Reversible, Sorted,
    Murmur3,
)


class TestConstruction:
    def test_empty(self):
        assert PERSISTENT_TREE_SET_EMPTY.count() == 0

    def test_create(self):
        s = PersistentTreeSet.create(3, 1, 2, 1, 5, 4)
        assert s.count() == 5  # de-duplicated

    def test_create_with_comparator(self):
        rev = lambda a, b: -1 if a > b else (1 if a < b else 0)
        s = PersistentTreeSet.create_with_comparator(rev, 1, 3, 2)
        assert list(s) == [3, 2, 1]


class TestSorted:
    def test_iter_in_order(self):
        s = PersistentTreeSet.create(5, 2, 8, 1, 3)
        assert list(s) == [1, 2, 3, 5, 8]

    def test_rseq(self):
        s = PersistentTreeSet.create(1, 2, 3)
        assert list(s.rseq()) == [3, 2, 1]

    def test_seq_with_comparator(self):
        s = PersistentTreeSet.create(1, 2, 3, 4, 5)
        asc = list(s.seq_with_comparator(True))
        assert asc == [1, 2, 3, 4, 5]
        desc = list(s.seq_with_comparator(False))
        assert desc == [5, 4, 3, 2, 1]

    def test_seq_from(self):
        s = PersistentTreeSet.create(1, 2, 3, 4, 5)
        assert list(s.seq_from(3, True)) == [3, 4, 5]
        assert list(s.seq_from(3, False)) == [3, 2, 1]


class TestSetOps:
    def test_contains(self):
        s = PersistentTreeSet.create(1, 2, 3)
        assert s.contains(2)
        assert not s.contains(99)

    def test_disjoin(self):
        s = PersistentTreeSet.create(1, 2, 3)
        s2 = s.disjoin(2)
        assert list(s2) == [1, 3]

    def test_cons(self):
        s = PersistentTreeSet.create(1, 3)
        s2 = s.cons(2)
        assert list(s2) == [1, 2, 3]   # sorted

    def test_cons_existing_returns_self(self):
        s = PersistentTreeSet.create(1, 2)
        assert s.cons(1) is s


class TestCallable:
    def test_call_returns_key_when_present(self):
        s = PersistentTreeSet.create(1, 2)
        assert s(1) == 1

    def test_call_missing_returns_none(self):
        assert PersistentTreeSet.create(1)(99) is None


class TestEquality:
    def test_equal_to_other_pts(self):
        a = PersistentTreeSet.create(3, 1, 2)
        b = PersistentTreeSet.create(1, 2, 3)
        assert a == b

    def test_equal_to_python_set(self):
        assert PersistentTreeSet.create(1, 2, 3) == {1, 2, 3}


class TestHash:
    def test_hashable(self):
        a = PersistentTreeSet.create(1, 2, 3)
        b = PersistentTreeSet.create(3, 2, 1)
        assert hash(a) == hash(b)

    def test_hasheq_unordered(self):
        s = PersistentTreeSet.create(1, 2, 3)
        assert s.hasheq() == Murmur3.hash_unordered(s)


class TestEmpty:
    def test_empty_propagates_meta(self):
        s = PersistentTreeSet.create(1).with_meta({"k": "v"})
        e = s.empty()
        assert e.count() == 0
        assert e.meta() == {"k": "v"}


class TestInterfaces:
    def test_all_abcs(self):
        s = PersistentTreeSet.create(1)
        assert isinstance(s, IPersistentSet)
        assert isinstance(s, IPersistentCollection)
        assert isinstance(s, Counted)
        assert isinstance(s, IFn)
        assert isinstance(s, Reversible)
        assert isinstance(s, Sorted)
