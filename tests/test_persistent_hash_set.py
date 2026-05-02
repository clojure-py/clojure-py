"""Tests for PersistentHashSet and TransientHashSet."""
import pytest

from clojure.lang import (
    PersistentHashSet, PERSISTENT_HASH_SET_EMPTY, TransientHashSet,
    Keyword,
    IPersistentSet, IPersistentCollection, Counted, IFn, IHashEq,
    IMeta, IObj, IEditableCollection,
    ITransientSet, ITransientCollection,
    Murmur3,
)


class TestConstruction:
    def test_create(self):
        s = PersistentHashSet.create("a", "b", "c")
        assert s.count() == 3

    def test_create_dedupes(self):
        s = PersistentHashSet.create("a", "b", "a")
        assert s.count() == 2

    def test_create_with_check_rejects_duplicates(self):
        with pytest.raises(ValueError):
            PersistentHashSet.create_with_check("a", "b", "a")

    def test_from_iterable(self):
        s = PersistentHashSet.from_iterable(range(10))
        assert s.count() == 10


class TestBasics:
    def test_contains(self):
        s = PersistentHashSet.create(1, 2, 3)
        assert s.contains(1)
        assert not s.contains(99)

    def test_get_returns_key_when_present(self):
        s = PersistentHashSet.create("a", "b")
        assert s.get("a") == "a"

    def test_get_missing(self):
        assert PersistentHashSet.create().get("z", "default") == "default"

    def test_disjoin(self):
        s = PersistentHashSet.create(1, 2, 3)
        s2 = s.disjoin(2)
        assert s2.count() == 2
        assert not s2.contains(2)

    def test_disjoin_missing_returns_self(self):
        s = PersistentHashSet.create(1)
        assert s.disjoin(99) is s

    def test_cons(self):
        s = PersistentHashSet.create(1, 2)
        assert s.cons(3).count() == 3

    def test_cons_existing_returns_self(self):
        s = PersistentHashSet.create(1, 2)
        assert s.cons(1) is s


class TestCallable:
    def test_call_returns_key_when_present(self):
        s = PersistentHashSet.create("a", "b")
        assert s("a") == "a"

    def test_call_missing_returns_none(self):
        assert PersistentHashSet.create("a")("z") is None

    def test_call_with_default(self):
        assert PersistentHashSet.create("a")("z", "X") == "X"


class TestEquality:
    def test_equal_to_other_phs(self):
        a = PersistentHashSet.create(1, 2, 3)
        b = PersistentHashSet.create(3, 2, 1)
        assert a == b

    def test_equal_to_python_set(self):
        assert PersistentHashSet.create(1, 2, 3) == {1, 2, 3}

    def test_equal_to_frozenset(self):
        assert PersistentHashSet.create(1, 2) == frozenset({1, 2})

    def test_different_size_not_equal(self):
        assert PersistentHashSet.create(1) != PersistentHashSet.create(1, 2)


class TestHash:
    def test_hashable(self):
        a = PersistentHashSet.create(1, 2, 3)
        b = PersistentHashSet.create(3, 2, 1)
        assert hash(a) == hash(b)

    def test_hasheq_uses_unordered(self):
        s = PersistentHashSet.create(1, 2, 3)
        assert s.hasheq() == Murmur3.hash_unordered(s)


class TestSeq:
    def test_seq_yields_keys(self):
        s = PersistentHashSet.create("a", "b")
        keys = list(s.seq())
        assert set(keys) == {"a", "b"}

    def test_empty_seq_is_none(self):
        assert PERSISTENT_HASH_SET_EMPTY.seq() is None


class TestPython:
    def test_in_operator(self):
        s = PersistentHashSet.create(1, 2)
        assert 1 in s
        assert 99 not in s

    def test_iter_yields_keys(self):
        s = PersistentHashSet.create("a", "b")
        assert set(s) == {"a", "b"}

    def test_str_uses_hash_set_syntax(self):
        # Order may vary; just check the framing.
        s = PersistentHashSet.create("a")
        assert str(s) == '#{"a"}'


class TestEmpty:
    def test_empty_no_meta_returns_singleton(self):
        s = PersistentHashSet.create("a")
        assert s.empty() is PERSISTENT_HASH_SET_EMPTY

    def test_empty_propagates_meta(self):
        s = PersistentHashSet.create("a").with_meta({"k": "v"})
        e = s.empty()
        assert e.meta() == {"k": "v"}


class TestMeta:
    def test_with_meta(self):
        s = PersistentHashSet.create("a")
        s2 = s.with_meta({"line": 1})
        assert s2.meta() == {"line": 1}
        assert s == s2


class TestInterfaces:
    def test_isinstance_ipersistentset(self):
        s = PersistentHashSet.create(1)
        assert isinstance(s, IPersistentSet)
        assert isinstance(s, IPersistentCollection)
        assert isinstance(s, Counted)
        assert isinstance(s, IFn)
        assert isinstance(s, IHashEq)
        assert isinstance(s, IEditableCollection)


class TestTransient:
    def test_basic(self):
        t = PERSISTENT_HASH_SET_EMPTY.as_transient()
        for i in range(50):
            t.conj(i)
        result = t.persistent()
        assert result.count() == 50
        for i in range(50):
            assert result.contains(i)

    def test_disjoin(self):
        t = PersistentHashSet.create(1, 2, 3).as_transient()
        t.disjoin(2)
        assert not t.persistent().contains(2)

    def test_contains(self):
        t = PersistentHashSet.create(1, 2).as_transient()
        assert t.contains(1)
        assert not t.contains(99)

    def test_isinstance_transient(self):
        t = PERSISTENT_HASH_SET_EMPTY.as_transient()
        assert isinstance(t, ITransientSet)
        assert isinstance(t, ITransientCollection)
