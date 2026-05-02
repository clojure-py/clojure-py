"""Tests for the seq layer: EmptyList, Cons, LazySeq, IteratorSeq, Range,
Iterate, Cycle, Repeat."""
import pytest

from clojure.lang import (
    Cons, LazySeq, IteratorSeq, Range, Iterate, Cycle, Repeat,
    ISeq, IPersistentCollection, Sequential, IHashEq, IPending,
    Counted, IMeta, IObj,
    Util, Murmur3,
)


# ---------- Cons ----------

class TestCons:
    def test_basic(self):
        c = Cons(1, None)
        assert c.first() == 1
        assert c.next() is None

    def test_chain(self):
        c = Cons(1, Cons(2, Cons(3, None)))
        assert list(c) == [1, 2, 3]

    def test_count(self):
        c = Cons(1, Cons(2, Cons(3, None)))
        assert c.count() == 3
        assert len(c) == 3

    def test_more_returns_empty_seq_not_none(self):
        # Java contract: more() never returns null. End-of-seq returns the
        # empty list singleton.
        c = Cons(1, None)
        m = c.more()
        # Empty — seq() is None — but it's still a Seqable object.
        assert m.seq() is None

    def test_seq_returns_self(self):
        c = Cons(1, None)
        assert c.seq() is c

    def test_cons_method_prepends(self):
        c = Cons(2, Cons(3, None))
        c2 = c.cons(1)
        assert list(c2) == [1, 2, 3]

    def test_isinstance_iseq(self):
        assert isinstance(Cons(1, None), ISeq)
        assert isinstance(Cons(1, None), Sequential)
        assert isinstance(Cons(1, None), IHashEq)
        assert isinstance(Cons(1, None), IMeta)
        assert isinstance(Cons(1, None), IObj)

    def test_with_meta(self):
        c = Cons(1, None)
        c2 = c.with_meta({"key": "value"})
        assert c2 is not c
        assert c.meta() is None
        assert c2.meta() == {"key": "value"}
        assert c == c2  # equality ignores meta


# ---------- equality / hash ----------

class TestSeqEquality:
    def test_cons_equal_to_list(self):
        # ASeq.__eq__ accepts Sequential / list / tuple.
        assert Cons(1, Cons(2, Cons(3, None))) == [1, 2, 3]

    def test_cons_equal_to_tuple(self):
        assert Cons(1, Cons(2, None)) == (1, 2)

    def test_cons_not_equal_to_dict(self):
        assert Cons(1, None) != {"a": 1}

    def test_different_length_not_equal(self):
        assert Cons(1, Cons(2, None)) != [1, 2, 3]
        assert Cons(1, Cons(2, Cons(3, None))) != [1, 2]

    def test_two_cons_chains_equal(self):
        a = Cons(1, Cons(2, None))
        b = Cons(1, Cons(2, None))
        assert a == b

    def test_equiv_uses_util_equiv(self):
        # Util.equiv handles cross-numeric where == doesn't, but for same-category
        # numbers they agree.
        assert Cons(1, None).equiv([1])
        assert not Cons(1, None).equiv([1.0])  # cross-category — Util.equiv false


class TestSeqHashing:
    def test_hashable(self):
        c1 = Cons(1, Cons(2, None))
        c2 = Cons(1, Cons(2, None))
        # Plain Python __hash__ matches because content is identical.
        assert hash(c1) == hash(c2)

    def test_hasheq_uses_murmur3_hash_ordered(self):
        c = Cons(1, Cons(2, Cons(3, None)))
        # Should match the explicit Murmur3.hash_ordered call.
        assert c.hasheq() == Murmur3.hash_ordered([1, 2, 3])

    def test_hasheq_caches(self):
        c = Cons(1, Cons(2, None))
        assert c.hasheq() == c.hasheq()  # Stable


# ---------- LazySeq ----------

class TestLazySeq:
    def test_basic_realizes_on_seq(self):
        side_effects = []

        def thunk():
            side_effects.append("forced")
            return Cons(1, Cons(2, None))

        ls = LazySeq(thunk)
        assert side_effects == []
        assert not ls.is_realized()

        ls.seq()  # force
        assert side_effects == ["forced"]
        assert ls.is_realized()

        # Second seq() doesn't re-run.
        ls.seq()
        assert side_effects == ["forced"]

    def test_walk(self):
        ls = LazySeq(lambda: Cons(1, Cons(2, Cons(3, None))))
        assert list(ls) == [1, 2, 3]

    def test_returns_none_for_empty(self):
        ls = LazySeq(lambda: None)
        assert ls.seq() is None
        assert ls.first() is None
        assert ls.next() is None
        assert list(ls) == []

    def test_nested_lazy_seq_unwraps(self):
        # LazySeq returning another LazySeq → both forced; Java-style
        # iterative unwrap.
        inner = LazySeq(lambda: Cons(42, None))
        outer = LazySeq(lambda: inner)
        assert outer.first() == 42

    def test_lazy_seq_from_list(self):
        # Thunk returning a Python list — coerce-to-seq path.
        ls = LazySeq(lambda: [1, 2, 3])
        assert list(ls) == [1, 2, 3]

    def test_isinstance_ipending(self):
        assert isinstance(LazySeq(lambda: None), IPending)


# ---------- IteratorSeq ----------

class TestIteratorSeq:
    def test_from_list(self):
        s = IteratorSeq.from_iterable([1, 2, 3])
        assert list(s) == [1, 2, 3]

    def test_empty_iterable_returns_none(self):
        assert IteratorSeq.from_iterable([]) is None

    def test_first_repeatable(self):
        s = IteratorSeq.from_iterable([1, 2, 3])
        assert s.first() == 1
        assert s.first() == 1   # Stable; doesn't re-pull from iter.

    def test_next_advances(self):
        s = IteratorSeq.from_iterable([1, 2, 3])
        assert s.first() == 1
        s2 = s.next()
        assert s2.first() == 2
        s3 = s2.next()
        assert s3.first() == 3
        assert s3.next() is None

    def test_works_with_generator(self):
        def gen():
            yield "a"
            yield "b"
        s = IteratorSeq.from_iterable(gen())
        assert list(s) == ["a", "b"]


# ---------- Range ----------

class TestRange:
    def test_create_one_arg(self):
        assert list(Range.create(5)) == [0, 1, 2, 3, 4]

    def test_create_two_args(self):
        assert list(Range.create(2, 6)) == [2, 3, 4, 5]

    def test_create_three_args(self):
        assert list(Range.create(0, 10, 2)) == [0, 2, 4, 6, 8]

    def test_create_descending(self):
        assert list(Range.create(5, 0, -1)) == [5, 4, 3, 2, 1]

    def test_empty_range(self):
        assert Range.create(5, 5) is not None  # empty list, not None
        assert list(Range.create(5, 5)) == []

    def test_step_zero_raises(self):
        with pytest.raises(ValueError):
            Range(0, 10, 0)

    def test_count_o1(self):
        # Range is Counted — count is O(1), not O(n).
        r = Range(0, 1000, 1)
        assert r.count() == 1000
        # And ascending step 3:
        assert Range(0, 10, 3).count() == 4    # 0, 3, 6, 9

    def test_count_descending(self):
        assert Range(10, 0, -1).count() == 10

    def test_membership_optimized(self):
        r = Range(0, 100, 5)
        assert 25 in r
        assert 26 not in r
        assert -5 not in r
        assert 100 not in r   # exclusive end

    def test_isinstance_counted(self):
        assert isinstance(Range(0, 5, 1), Counted)


# ---------- Iterate ----------

class TestIterate:
    def test_basic(self):
        # (iterate inc 0) → 0 1 2 3 4 ...
        it = Iterate(lambda x: x + 1, 0)
        first_five = []
        s = it
        for _ in range(5):
            first_five.append(s.first())
            s = s.next()
        assert first_five == [0, 1, 2, 3, 4]

    def test_each_step_lazy(self):
        # f only invoked when next() advances.
        calls = []

        def f(x):
            calls.append(x)
            return x * 2

        it = Iterate(f, 1)
        assert calls == []
        it.first()
        assert calls == []      # first is the seed; no f call yet
        it.next()
        assert calls == [1]     # one step taken
        it.next()
        assert calls == [1]     # second next() is cached, no new call


# ---------- Cycle ----------

class TestCycle:
    def test_cycle_finite_input(self):
        c = Cycle.create([1, 2, 3])
        first_seven = []
        s = c
        for _ in range(7):
            first_seven.append(s.first())
            s = s.next()
        assert first_seven == [1, 2, 3, 1, 2, 3, 1]

    def test_empty_input(self):
        # (cycle ()) → ()
        c = Cycle.create([])
        # Empty list singleton: seq() is None
        assert c.seq() is None


# ---------- Repeat ----------

class TestRepeat:
    def test_finite(self):
        r = Repeat.create(3, "x")
        assert list(r) == ["x", "x", "x"]

    def test_finite_count(self):
        assert Repeat.create(5, 0).count() == 5

    def test_infinite_count_raises(self):
        with pytest.raises(ArithmeticError):
            Repeat.create("x").count()

    def test_zero_count_is_empty(self):
        assert Repeat.create(0, "x").seq() is None

    def test_infinite_first_two(self):
        r = Repeat.create("x")
        assert r.first() == "x"
        assert r.next() is r   # Infinite Repeat returns itself as next


# ---------- EmptyList ----------

class TestEmptyList:
    def test_empty_list_via_emit(self):
        # We don't export EmptyList directly, but Cons(_, None).more() returns
        # the singleton.
        c = Cons(1, None)
        empty = c.more()
        assert empty.seq() is None
        assert empty.count() == 0
        assert len(empty) == 0
        assert list(empty) == []

    def test_empty_equals_empty_list(self):
        empty = Cons(1, None).more()
        assert empty == []
        assert empty == ()

    def test_empty_cons_creates_one_element(self):
        empty = Cons(1, None).more()
        c = empty.cons("a")
        assert list(c) == ["a"]

    def test_pop_empty_raises(self):
        empty = Cons(1, None).more()
        with pytest.raises(IndexError):
            empty.pop()


# ---------- Mixed: Range hashing ----------

class TestRangeHashing:
    def test_range_hasheq_matches_walked_seq(self):
        # Range.hasheq should equal Murmur3.hash_ordered over its values.
        r = Range(0, 5, 1)
        assert r.hasheq() == Murmur3.hash_ordered([0, 1, 2, 3, 4])
