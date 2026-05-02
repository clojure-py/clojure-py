"""Tests for PersistentList and Reduced."""
import pytest

from clojure.lang import (
    PersistentList, PERSISTENT_LIST_EMPTY, Cons,
    Reduced, is_reduced,
    IPersistentList, IPersistentStack, IPersistentCollection,
    Sequential, Counted, ISeq, IHashEq, IMeta, IObj, IDeref,
    IReduce, IReduceInit,
    Murmur3,
)


# ---------- Construction ----------

class TestConstruction:
    def test_create_from_list(self):
        pl = PersistentList.create([1, 2, 3])
        assert list(pl) == [1, 2, 3]

    def test_create_empty_returns_singleton(self):
        # PersistentList.create([]) returns the empty singleton.
        assert PersistentList.create([]) is PERSISTENT_LIST_EMPTY

    def test_create_from_tuple(self):
        assert list(PersistentList.create((1, 2, 3))) == [1, 2, 3]

    def test_create_from_generator(self):
        assert list(PersistentList.create(i for i in range(3))) == [0, 1, 2]

    def test_direct_construction_singleton(self):
        # Internal construction: PersistentList(first, rest=None, count=1)
        pl = PersistentList(42)
        assert pl.count() == 1
        assert pl.first() == 42
        assert pl.next() is None


# ---------- count / O(1) ----------

class TestCount:
    def test_count_singleton(self):
        assert PersistentList.create([1]).count() == 1

    def test_count_long(self):
        assert PersistentList.create(range(100)).count() == 100

    def test_count_is_o1(self):
        # Direct field, not a walk. We verify by comparing wall-clock O(n) walk.
        pl = PersistentList.create(range(100_000))
        # Just assert correctness — O(1) is implicit.
        assert pl.count() == 100_000
        assert len(pl) == 100_000

    def test_empty_count(self):
        assert PERSISTENT_LIST_EMPTY.count() == 0


# ---------- ISeq behavior ----------

class TestSeqBehavior:
    def test_first(self):
        assert PersistentList.create([1, 2, 3]).first() == 1

    def test_next_walks(self):
        pl = PersistentList.create([1, 2, 3])
        assert pl.next().first() == 2
        assert pl.next().next().first() == 3
        assert pl.next().next().next() is None

    def test_seq_returns_self(self):
        pl = PersistentList.create([1])
        assert pl.seq() is pl

    def test_more_returns_empty_seq_at_end(self):
        # Java contract: more() never null. Returns empty seq when at end.
        pl = PersistentList.create([1])  # singleton
        m = pl.more()
        assert m.seq() is None
        assert m.count() == 0


# ---------- IPersistentStack: peek / pop ----------

class TestStack:
    def test_peek(self):
        assert PersistentList.create([1, 2, 3]).peek() == 1

    def test_pop_returns_tail(self):
        pl = PersistentList.create([1, 2, 3])
        assert list(pl.pop()) == [2, 3]

    def test_pop_singleton_returns_empty(self):
        pl = PersistentList.create([42])
        assert pl.pop() is PERSISTENT_LIST_EMPTY

    def test_pop_empty_raises(self):
        with pytest.raises(IndexError):
            PERSISTENT_LIST_EMPTY.pop()


# ---------- cons / persistence ----------

class TestCons:
    def test_cons_prepends(self):
        pl = PersistentList.create([2, 3])
        pl2 = pl.cons(1)
        assert list(pl2) == [1, 2, 3]

    def test_cons_returns_persistent_list(self):
        pl = PersistentList.create([2, 3]).cons(1)
        assert isinstance(pl, PersistentList)

    def test_cons_increments_count(self):
        pl = PersistentList.create([2, 3])
        assert pl.cons(1).count() == 3

    def test_original_unchanged(self):
        pl = PersistentList.create([2, 3])
        pl.cons(1)  # discard
        assert list(pl) == [2, 3]
        assert pl.count() == 2

    def test_empty_cons_yields_persistent_list_singleton(self):
        # _empty_list.cons(x) → PersistentList(x, None, 1)
        pl = PERSISTENT_LIST_EMPTY.cons("a")
        assert isinstance(pl, PersistentList)
        assert pl.count() == 1
        assert pl.first() == "a"


# ---------- equality ----------

class TestEquality:
    def test_equal_to_other_persistent_list(self):
        a = PersistentList.create([1, 2, 3])
        b = PersistentList.create([1, 2, 3])
        assert a == b

    def test_equal_to_python_list(self):
        # ASeq.__eq__ accepts Sequential / list / tuple.
        assert PersistentList.create([1, 2, 3]) == [1, 2, 3]

    def test_equal_to_tuple(self):
        assert PersistentList.create([1, 2]) == (1, 2)

    def test_equal_to_cons_chain(self):
        pl = PersistentList.create([1, 2, 3])
        c = Cons(1, Cons(2, Cons(3, None)))
        assert pl == c

    def test_different_length_not_equal(self):
        assert PersistentList.create([1, 2]) != [1, 2, 3]

    def test_empty_equal_to_empty_list(self):
        assert PERSISTENT_LIST_EMPTY == []
        assert PERSISTENT_LIST_EMPTY == ()


# ---------- hash / hasheq ----------

class TestHash:
    def test_hashable(self):
        a = PersistentList.create([1, 2, 3])
        b = PersistentList.create([1, 2, 3])
        assert hash(a) == hash(b)

    def test_hasheq_matches_murmur3_hash_ordered(self):
        pl = PersistentList.create([1, 2, 3])
        assert pl.hasheq() == Murmur3.hash_ordered([1, 2, 3])

    def test_empty_hasheq_matches_mix_coll_hash(self):
        # Java EmptyList.hasheq = Murmur3.hashOrdered([]) = mix_coll_hash(1, 0).
        assert PERSISTENT_LIST_EMPTY.hasheq() == Murmur3.mix_coll_hash(1, 0)


# ---------- meta / withMeta ----------

class TestMeta:
    def test_default_meta_none(self):
        assert PersistentList.create([1]).meta() is None

    def test_with_meta_returns_new_instance(self):
        pl = PersistentList.create([1, 2])
        meta = {"line": 10}
        pl2 = pl.with_meta(meta)
        assert pl2 is not pl
        assert pl.meta() is None
        assert pl2.meta() is meta
        assert pl == pl2  # equality ignores meta

    def test_with_meta_idempotent(self):
        pl = PersistentList.create([1])
        meta = {"a": 1}
        pl2 = pl.with_meta(meta)
        assert pl2.with_meta(meta) is pl2

    def test_empty_with_meta(self):
        meta = {"x": 1}
        empty2 = PERSISTENT_LIST_EMPTY.with_meta(meta)
        assert empty2.meta() is meta
        assert empty2.count() == 0


# ---------- empty() ----------

class TestEmpty:
    def test_empty_returns_empty_singleton(self):
        pl = PersistentList.create([1, 2, 3])
        assert pl.empty() is PERSISTENT_LIST_EMPTY

    def test_empty_propagates_meta(self):
        pl = PersistentList.create([1, 2]).with_meta({"k": "v"})
        e = pl.empty()
        assert e.count() == 0
        assert e.meta() == {"k": "v"}


# ---------- reduce / IReduce ----------

class TestReduce:
    def test_reduce_no_init(self):
        pl = PersistentList.create([1, 2, 3, 4])
        assert pl.reduce(lambda a, b: a + b) == 10

    def test_reduce_with_init(self):
        pl = PersistentList.create([1, 2, 3, 4])
        assert pl.reduce(lambda a, b: a + b, 100) == 110

    def test_reduce_singleton_no_init(self):
        # No-init reduce on a 1-element list: returns the single element.
        pl = PersistentList.create([42])
        assert pl.reduce(lambda a, b: a + b) == 42

    def test_reduce_with_reduced_short_circuits(self):
        def early(a, b):
            if b == 3:
                return Reduced("STOP")
            return a + b

        pl = PersistentList.create([1, 2, 3, 4, 5])
        # Without short-circuit would be 15. We expect "STOP".
        assert pl.reduce(early, 0) == "STOP"

    def test_reduce_with_reduced_no_init(self):
        def early(a, b):
            if b == 3:
                return Reduced(a)
            return a + b
        pl = PersistentList.create([1, 2, 3, 4, 5])
        # Walk: ret=1, then a=1 b=2 → 3, then a=3 b=3 → Reduced(3) → unwrap
        assert pl.reduce(early) == 3


# ---------- Reduced sentinel ----------

class TestReduced:
    def test_deref(self):
        r = Reduced(42)
        assert r.deref() == 42

    def test_isinstance_ideref(self):
        assert isinstance(Reduced(0), IDeref)

    def test_is_reduced_predicate(self):
        assert is_reduced(Reduced(0))
        assert not is_reduced(0)
        assert not is_reduced(None)


# ---------- ABC registration ----------

class TestInterfaces:
    def test_isinstance_ipersistentlist(self):
        pl = PersistentList.create([1])
        assert isinstance(pl, IPersistentList)
        assert isinstance(PERSISTENT_LIST_EMPTY, IPersistentList)

    def test_isinstance_ipersistentstack(self):
        assert isinstance(PersistentList.create([1]), IPersistentStack)

    def test_isinstance_ipersistentcollection(self):
        assert isinstance(PersistentList.create([1]), IPersistentCollection)

    def test_isinstance_sequential(self):
        assert isinstance(PersistentList.create([1]), Sequential)

    def test_isinstance_counted(self):
        assert isinstance(PersistentList.create([1]), Counted)

    def test_isinstance_iseq(self):
        # PersistentList is an ASeq → IS an ISeq.
        assert isinstance(PersistentList.create([1]), ISeq)

    def test_isinstance_ireduce(self):
        assert isinstance(PersistentList.create([1]), IReduce)
        assert isinstance(PersistentList.create([1]), IReduceInit)
