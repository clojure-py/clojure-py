"""Tests for PersistentQueue."""
import pytest

from clojure.lang import (
    PersistentQueue, PERSISTENT_QUEUE_EMPTY,
    IPersistentList, IPersistentStack, IPersistentCollection,
    Sequential, Counted, IHashEq, IMeta, IObj,
    Murmur3,
)


class TestConstruction:
    def test_empty(self):
        assert PERSISTENT_QUEUE_EMPTY.count() == 0

    def test_create(self):
        q = PersistentQueue.create("a", "b", "c")
        assert q.count() == 3

    def test_from_iterable(self):
        q = PersistentQueue.from_iterable(range(5))
        assert list(q) == [0, 1, 2, 3, 4]


class TestFifoBehavior:
    def test_peek_returns_first_added(self):
        q = PersistentQueue.create("a", "b", "c")
        assert q.peek() == "a"

    def test_pop_removes_first(self):
        q = PersistentQueue.create("a", "b", "c")
        assert list(q.pop()) == ["b", "c"]

    def test_pop_empty_returns_self(self):
        # Java semantics: pop of empty is empty (NOT an exception).
        empty = PERSISTENT_QUEUE_EMPTY
        assert empty.pop() is empty

    def test_peek_empty_returns_none(self):
        assert PERSISTENT_QUEUE_EMPTY.peek() is None

    def test_full_drain(self):
        q = PersistentQueue.create(1, 2, 3, 4, 5)
        drained = []
        while q.count() > 0:
            drained.append(q.peek())
            q = q.pop()
        assert drained == [1, 2, 3, 4, 5]


class TestCons:
    def test_cons_first_element(self):
        q = PERSISTENT_QUEUE_EMPTY.cons("a")
        assert q.count() == 1
        assert q.peek() == "a"

    def test_cons_subsequent(self):
        q = PERSISTENT_QUEUE_EMPTY.cons("a").cons("b").cons("c")
        assert list(q) == ["a", "b", "c"]


class TestLargeQueue:
    def test_thousand_enqueue_then_drain(self):
        q = PERSISTENT_QUEUE_EMPTY
        for i in range(1000):
            q = q.cons(i)
        drained = list(q)
        assert drained == list(range(1000))

    def test_alternating_cons_pop(self):
        # Java's main test pattern: cons twice, pop once → end up with 1000 items.
        q = PERSISTENT_QUEUE_EMPTY
        for i in range(1000):
            q = q.cons(i).cons(i)
            q = q.pop()
        # We've added 2000 and removed 1000 → 1000 left.
        assert q.count() == 1000

    def test_persistence(self):
        q1 = PersistentQueue.create(1, 2, 3)
        q2 = q1.cons(4)
        # Original unchanged.
        assert list(q1) == [1, 2, 3]
        assert list(q2) == [1, 2, 3, 4]


class TestEquality:
    def test_equal_to_other_queue(self):
        a = PersistentQueue.create(1, 2, 3)
        b = PersistentQueue.create(1, 2, 3)
        assert a == b

    def test_equal_to_list(self):
        # Sequential equality with a Python list.
        assert PersistentQueue.create(1, 2, 3) == [1, 2, 3]

    def test_different_order_not_equal(self):
        assert PersistentQueue.create(1, 2) != PersistentQueue.create(2, 1)


class TestHash:
    def test_hashable(self):
        a = PersistentQueue.create(1, 2, 3)
        b = PersistentQueue.create(1, 2, 3)
        assert hash(a) == hash(b)

    def test_hasheq_ordered(self):
        q = PersistentQueue.create(1, 2, 3)
        assert q.hasheq() == Murmur3.hash_ordered([1, 2, 3])


class TestSeq:
    def test_seq_walks_in_order(self):
        q = PersistentQueue.create(1, 2, 3)
        assert list(q.seq()) == [1, 2, 3]

    def test_empty_seq_none(self):
        assert PERSISTENT_QUEUE_EMPTY.seq() is None


class TestEmpty:
    def test_empty_no_meta_returns_singleton(self):
        q = PersistentQueue.create(1, 2)
        assert q.empty() is PERSISTENT_QUEUE_EMPTY

    def test_empty_propagates_meta(self):
        q = PersistentQueue.create(1).with_meta({"k": "v"})
        e = q.empty()
        assert e.meta() == {"k": "v"}


class TestInterfaces:
    def test_isinstance(self):
        q = PersistentQueue.create(1)
        assert isinstance(q, IPersistentList)
        assert isinstance(q, IPersistentStack)
        assert isinstance(q, IPersistentCollection)
        assert isinstance(q, Sequential)
        assert isinstance(q, Counted)
        assert isinstance(q, IHashEq)
