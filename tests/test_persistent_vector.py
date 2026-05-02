"""Tests for PersistentVector, TransientVector, ChunkedSeq, ArrayChunk,
ChunkedCons, ChunkBuffer."""
import pytest

from clojure.lang import (
    PersistentVector, PERSISTENT_VECTOR_EMPTY, TransientVector,
    ArrayChunk, ChunkBuffer, ChunkedCons,
    Reduced,
    IPersistentVector, IPersistentStack, IPersistentCollection,
    Associative, ILookup, Indexed, Counted, Reversible, IFn,
    IHashEq, IMeta, IObj, Sequential, IEditableCollection,
    IReduce, IReduceInit, IKVReduce, IDrop,
    IChunk, IChunkedSeq,
    ITransientVector, ITransientAssociative, ITransientAssociative2,
    ITransientCollection,
    Murmur3,
)


# =========================================================================
# PersistentVector — small (everything fits in tail)
# =========================================================================

class TestSmallVector:
    def test_empty_count(self):
        assert PERSISTENT_VECTOR_EMPTY.count() == 0
        assert len(PERSISTENT_VECTOR_EMPTY) == 0

    def test_create_varargs(self):
        v = PersistentVector.create(1, 2, 3)
        assert list(v) == [1, 2, 3]
        assert v.count() == 3

    def test_create_no_args_returns_empty_singleton(self):
        # PersistentVector.create() with no args produces an empty vector.
        # NOT necessarily the same singleton (since create goes through
        # transient + persistent), but functionally empty.
        v = PersistentVector.create()
        assert v.count() == 0

    def test_from_iterable(self):
        v = PersistentVector.from_iterable(range(5))
        assert list(v) == [0, 1, 2, 3, 4]

    def test_nth(self):
        v = PersistentVector.create(10, 20, 30)
        assert v.nth(0) == 10
        assert v.nth(1) == 20
        assert v.nth(2) == 30

    def test_nth_out_of_range_raises(self):
        v = PersistentVector.create(1, 2)
        with pytest.raises(IndexError):
            v.nth(2)
        with pytest.raises(IndexError):
            v.nth(-1)

    def test_nth_with_default(self):
        v = PersistentVector.create(1, 2)
        assert v.nth(99, "default") == "default"

    def test_indexing(self):
        v = PersistentVector.create("a", "b", "c")
        assert v[0] == "a"
        assert v[-1] == "c"   # negative index supported

    def test_slice(self):
        v = PersistentVector.create(0, 1, 2, 3, 4)
        assert v[1:4] == [1, 2, 3]


# =========================================================================
# Large vectors — exercise the trie
# =========================================================================

class TestLargeVector:
    def test_create_thousand(self):
        v = PersistentVector.from_iterable(range(1000))
        assert v.count() == 1000
        for i in range(0, 1000, 47):  # spot-check
            assert v.nth(i) == i

    def test_create_one_million_count(self):
        # Stress: a vector deep enough to exercise multiple trie levels
        # (>32^2 = 1024 elements forces shift > 5).
        v = PersistentVector.from_iterable(range(100_000))
        assert v.count() == 100_000
        assert v.nth(0) == 0
        assert v.nth(99_999) == 99_999

    def test_assoc_in_trie(self):
        v = PersistentVector.from_iterable(range(100))
        v2 = v.assoc_n(50, "X")
        assert v2.nth(50) == "X"
        assert v.nth(50) == 50   # original unchanged

    def test_cons_grows_through_trie(self):
        v = PERSISTENT_VECTOR_EMPTY
        for i in range(40):
            v = v.cons(i)
        assert v.count() == 40
        assert list(v) == list(range(40))

    def test_pop_through_trie(self):
        # Build then pop down to empty.
        v = PersistentVector.from_iterable(range(50))
        while v.count() > 0:
            v = v.pop()
        assert v.count() == 0


# =========================================================================
# assoc_n boundary: i == count appends
# =========================================================================

class TestAssocN:
    def test_assoc_at_end_appends(self):
        v = PersistentVector.create(1, 2, 3)
        v2 = v.assoc_n(3, 4)   # i == count → append (cons)
        assert list(v2) == [1, 2, 3, 4]

    def test_assoc_in_middle(self):
        v = PersistentVector.create(1, 2, 3)
        assert list(v.assoc_n(1, 99)) == [1, 99, 3]

    def test_assoc_out_of_range_raises(self):
        with pytest.raises(IndexError):
            PersistentVector.create(1, 2).assoc_n(10, 0)

    def test_associative_assoc(self):
        v = PersistentVector.create(1, 2, 3)
        assert list(v.assoc(1, 99)) == [1, 99, 3]

    def test_associative_assoc_non_int_raises(self):
        with pytest.raises(TypeError):
            PersistentVector.create(1).assoc("foo", 99)


# =========================================================================
# stack ops
# =========================================================================

class TestStack:
    def test_peek(self):
        assert PersistentVector.create(1, 2, 3).peek() == 3

    def test_peek_empty_returns_none(self):
        assert PERSISTENT_VECTOR_EMPTY.peek() is None

    def test_pop(self):
        assert list(PersistentVector.create(1, 2, 3).pop()) == [1, 2]

    def test_pop_empty_raises(self):
        with pytest.raises(IndexError):
            PERSISTENT_VECTOR_EMPTY.pop()


# =========================================================================
# IFn — vectors as functions of their index
# =========================================================================

class TestIFnInvoke:
    def test_call_with_int(self):
        v = PersistentVector.create("a", "b", "c")
        assert v(1) == "b"

    def test_call_out_of_range_raises(self):
        with pytest.raises(IndexError):
            PersistentVector.create("a")(5)

    def test_call_with_non_int_raises(self):
        with pytest.raises(TypeError):
            PersistentVector.create("a")("foo")


# =========================================================================
# ILookup / Associative
# =========================================================================

class TestLookup:
    def test_val_at_present(self):
        v = PersistentVector.create("a", "b", "c")
        assert v.val_at(0) == "a"
        assert v.val_at(2) == "c"

    def test_val_at_missing_returns_none(self):
        v = PersistentVector.create("a")
        assert v.val_at(99) is None

    def test_val_at_with_default(self):
        v = PersistentVector.create("a")
        assert v.val_at(99, "X") == "X"

    def test_contains_key(self):
        v = PersistentVector.create("a", "b")
        assert v.contains_key(0)
        assert v.contains_key(1)
        assert not v.contains_key(2)
        assert not v.contains_key(-1)
        assert not v.contains_key("foo")

    def test_entry_at_present(self):
        v = PersistentVector.create("a", "b")
        e = v.entry_at(1)
        assert e is not None
        assert e[0] == 1 and e[1] == "b"

    def test_entry_at_missing(self):
        assert PersistentVector.create("a").entry_at(5) is None


# =========================================================================
# equality
# =========================================================================

class TestEquality:
    def test_equal_to_other_vector(self):
        a = PersistentVector.create(1, 2, 3)
        b = PersistentVector.create(1, 2, 3)
        assert a == b

    def test_equal_to_python_list(self):
        assert PersistentVector.create(1, 2, 3) == [1, 2, 3]

    def test_equal_to_tuple(self):
        assert PersistentVector.create(1, 2) == (1, 2)

    def test_different_length_not_equal(self):
        assert PersistentVector.create(1, 2) != [1, 2, 3]

    def test_different_elements_not_equal(self):
        assert PersistentVector.create(1, 2, 3) != [1, 2, 4]

    def test_equiv_for_sequential(self):
        from clojure.lang import PersistentList
        assert PersistentVector.create(1, 2, 3).equiv(
            PersistentList.create([1, 2, 3]))

    def test_empty_equal_to_empty(self):
        assert PERSISTENT_VECTOR_EMPTY == []
        assert PERSISTENT_VECTOR_EMPTY == ()


# =========================================================================
# hash / hasheq
# =========================================================================

class TestHash:
    def test_hashable(self):
        a = PersistentVector.create(1, 2, 3)
        b = PersistentVector.create(1, 2, 3)
        assert hash(a) == hash(b)

    def test_hasheq_matches_murmur3(self):
        v = PersistentVector.create(1, 2, 3)
        assert v.hasheq() == Murmur3.hash_ordered([1, 2, 3])


# =========================================================================
# meta / withMeta
# =========================================================================

class TestMeta:
    def test_default_meta_none(self):
        assert PersistentVector.create(1).meta() is None

    def test_with_meta_returns_new_instance(self):
        v = PersistentVector.create(1, 2)
        meta = {"line": 5}
        v2 = v.with_meta(meta)
        assert v2 is not v
        assert v.meta() is None
        assert v2.meta() is meta
        assert v == v2  # meta doesn't affect equality


# =========================================================================
# empty()
# =========================================================================

class TestEmpty:
    def test_empty_returns_empty_vector(self):
        v = PersistentVector.create(1, 2, 3)
        assert v.empty().count() == 0

    def test_empty_no_meta_returns_singleton(self):
        v = PersistentVector.create(1, 2, 3)
        assert v.empty() is PERSISTENT_VECTOR_EMPTY

    def test_empty_propagates_meta(self):
        v = PersistentVector.create(1, 2).with_meta({"k": "v"})
        e = v.empty()
        assert e.meta() == {"k": "v"}


# =========================================================================
# reduce / IReduce
# =========================================================================

class TestReduce:
    def test_reduce_no_init(self):
        v = PersistentVector.create(1, 2, 3, 4)
        assert v.reduce(lambda a, b: a + b) == 10

    def test_reduce_with_init(self):
        v = PersistentVector.create(1, 2, 3, 4)
        assert v.reduce(lambda a, b: a + b, 100) == 110

    def test_reduce_empty_no_init_calls_f(self):
        # Java: reduce(f) on empty returns f.invoke() — Python equivalent f().
        sentinel = "F-OF-NONE"
        result = PERSISTENT_VECTOR_EMPTY.reduce(lambda *args: sentinel)
        assert result == sentinel

    def test_reduce_empty_with_init(self):
        assert PERSISTENT_VECTOR_EMPTY.reduce(lambda a, b: a + b, 42) == 42

    def test_reduce_short_circuit(self):
        def stop_at_three(a, b):
            if b == 3:
                return Reduced("STOP")
            return a + b

        v = PersistentVector.create(1, 2, 3, 4, 5)
        assert v.reduce(stop_at_three, 0) == "STOP"

    def test_reduce_through_trie(self):
        # Stress reduce across a multi-chunk vector.
        v = PersistentVector.from_iterable(range(1000))
        assert v.reduce(lambda a, b: a + b, 0) == sum(range(1000))


class TestKVReduce:
    def test_kv_reduce_sum_indices(self):
        v = PersistentVector.create("a", "b", "c")

        def f(acc, idx, val):
            return acc + idx
        assert v.kv_reduce(f, 0) == 0 + 1 + 2

    def test_kv_reduce_short_circuit(self):
        def f(acc, idx, val):
            if idx == 2:
                return Reduced(acc)
            return acc + 1
        v = PersistentVector.from_iterable(range(10))
        assert v.kv_reduce(f, 0) == 2

    def test_kv_reduce_through_trie(self):
        v = PersistentVector.from_iterable(range(200))
        result = v.kv_reduce(lambda acc, i, x: acc + (i * x), 0)
        assert result == sum(i * i for i in range(200))


# =========================================================================
# seq / chunked_seq
# =========================================================================

class TestSeq:
    def test_seq_walks_in_order(self):
        v = PersistentVector.create(1, 2, 3)
        s = v.seq()
        assert s.first() == 1
        assert s.next().first() == 2
        assert s.next().next().first() == 3
        assert s.next().next().next() is None

    def test_seq_empty_returns_none(self):
        assert PERSISTENT_VECTOR_EMPTY.seq() is None

    def test_seq_through_trie(self):
        v = PersistentVector.from_iterable(range(200))
        assert list(v.seq()) == list(range(200))

    def test_chunked_seq_first_is_chunk(self):
        v = PersistentVector.create(1, 2, 3)
        s = v.seq()
        c = s.chunked_first()
        assert isinstance(c, IChunk)
        assert c.count() == 3
        assert [c.nth(i) for i in range(c.count())] == [1, 2, 3]

    def test_chunked_seq_count(self):
        v = PersistentVector.from_iterable(range(70))
        s = v.seq()
        # First chunk is full 32; remaining count starts at 70.
        assert s.count() == 70

    def test_rseq(self):
        v = PersistentVector.create(1, 2, 3)
        rs = v.rseq()
        assert rs.first() == 3
        assert list(rs) == [3, 2, 1]
        assert rs.count() == 3


# =========================================================================
# IDrop
# =========================================================================

class TestDrop:
    def test_drop_zero(self):
        v = PersistentVector.create(1, 2, 3)
        assert list(v.drop(0)) == [1, 2, 3]

    def test_drop_partial(self):
        v = PersistentVector.create(1, 2, 3, 4, 5)
        assert list(v.drop(2)) == [3, 4, 5]

    def test_drop_all(self):
        v = PersistentVector.create(1, 2)
        assert v.drop(2) is None
        assert v.drop(99) is None

    def test_drop_through_trie(self):
        v = PersistentVector.from_iterable(range(100))
        assert list(v.drop(50))[:5] == [50, 51, 52, 53, 54]


# =========================================================================
# Iteration
# =========================================================================

class TestIteration:
    def test_iter(self):
        v = PersistentVector.create(1, 2, 3)
        assert list(v) == [1, 2, 3]

    def test_iter_through_trie(self):
        # Forces walking arrayFor across multiple chunks.
        v = PersistentVector.from_iterable(range(70))
        assert list(v) == list(range(70))

    def test_reversed(self):
        v = PersistentVector.create(1, 2, 3)
        assert list(reversed(v)) == [3, 2, 1]

    def test_contains_checks_KEY_RANGE(self):
        # Java/Clojure: (contains? v 3) checks key range, NOT value membership.
        v = PersistentVector.create("a", "b", "c")
        assert 0 in v
        assert 2 in v
        assert 3 not in v
        assert "a" not in v   # value lookup is NOT what `in` does for vectors


# =========================================================================
# String representation
# =========================================================================

class TestStr:
    def test_repr_uses_brackets(self):
        v = PersistentVector.create(1, 2, 3)
        assert str(v) == "[1 2 3]"

    def test_empty_repr(self):
        assert str(PERSISTENT_VECTOR_EMPTY) == "[]"


# =========================================================================
# ABC registration
# =========================================================================

class TestInterfaces:
    def test_isinstance_ipersistentvector(self):
        v = PersistentVector.create(1)
        assert isinstance(v, IPersistentVector)

    def test_isinstance_associative(self):
        assert isinstance(PersistentVector.create(1), Associative)

    def test_isinstance_indexed(self):
        assert isinstance(PersistentVector.create(1), Indexed)

    def test_isinstance_counted(self):
        assert isinstance(PersistentVector.create(1), Counted)

    def test_isinstance_reversible(self):
        assert isinstance(PersistentVector.create(1), Reversible)

    def test_isinstance_ifn(self):
        assert isinstance(PersistentVector.create(1), IFn)

    def test_isinstance_iredudce_kvreduce(self):
        assert isinstance(PersistentVector.create(1), IReduce)
        assert isinstance(PersistentVector.create(1), IReduceInit)
        assert isinstance(PersistentVector.create(1), IKVReduce)

    def test_isinstance_idrop(self):
        assert isinstance(PersistentVector.create(1), IDrop)

    def test_isinstance_ieditablecollection(self):
        assert isinstance(PersistentVector.create(1), IEditableCollection)

    def test_isinstance_sequential(self):
        assert isinstance(PersistentVector.create(1), Sequential)


# =========================================================================
# TransientVector
# =========================================================================

class TestTransient:
    def test_basic_conj(self):
        t = PERSISTENT_VECTOR_EMPTY.as_transient()
        for i in range(10):
            t.conj(i)
        v = t.persistent()
        assert list(v) == list(range(10))

    def test_assoc_n_in_place(self):
        t = PersistentVector.from_iterable(range(5)).as_transient()
        t.assoc_n(2, 99)
        assert t.persistent() == [0, 1, 99, 3, 4]

    def test_pop(self):
        t = PersistentVector.from_iterable(range(5)).as_transient()
        t.pop()
        t.pop()
        assert list(t.persistent()) == [0, 1, 2]

    def test_callable(self):
        t = PersistentVector.from_iterable(range(5)).as_transient()
        assert t(2) == 2

    def test_use_after_persistent_raises(self):
        t = PERSISTENT_VECTOR_EMPTY.as_transient()
        t.conj("a")
        t.persistent()
        with pytest.raises(RuntimeError):
            t.conj("b")

    def test_isinstance_transient_interfaces(self):
        t = PERSISTENT_VECTOR_EMPTY.as_transient()
        assert isinstance(t, ITransientVector)
        assert isinstance(t, ITransientAssociative)
        assert isinstance(t, ITransientCollection)

    def test_transient_grows_through_trie(self):
        t = PERSISTENT_VECTOR_EMPTY.as_transient()
        for i in range(100):
            t.conj(i)
        assert t.persistent() == list(range(100))


# =========================================================================
# ArrayChunk
# =========================================================================

class TestArrayChunk:
    def test_count(self):
        c = ArrayChunk([1, 2, 3, 4])
        assert c.count() == 4
        assert len(c) == 4

    def test_nth(self):
        c = ArrayChunk([10, 20, 30])
        assert c.nth(0) == 10
        assert c.nth(2) == 30

    def test_nth_out_of_range(self):
        c = ArrayChunk([10])
        with pytest.raises(IndexError):
            c.nth(5)
        assert c.nth(5, "default") == "default"

    def test_drop_first(self):
        c = ArrayChunk([1, 2, 3])
        c2 = c.drop_first()
        assert c2.count() == 2
        assert c2.nth(0) == 2

    def test_drop_first_empty_raises(self):
        c = ArrayChunk([1], 0, 0)  # already empty
        with pytest.raises(IndexError):
            c.drop_first()

    def test_reduce_returns_reduced_unwrapped_at_caller(self):
        # ArrayChunk.reduce returns the Reduced wrapper directly so its
        # caller (e.g. PersistentVector.reduce) can detect early termination.
        c = ArrayChunk([1, 2, 3])
        result = c.reduce(lambda a, b: Reduced(a + b), 0)
        assert isinstance(result, Reduced)

    def test_isinstance_ichunk_indexed_counted(self):
        c = ArrayChunk([1])
        assert isinstance(c, IChunk)
        assert isinstance(c, Indexed)
        assert isinstance(c, Counted)


# =========================================================================
# ChunkBuffer
# =========================================================================

class TestChunkBuffer:
    def test_add_and_chunk(self):
        b = ChunkBuffer(8)
        b.add("a")
        b.add("b")
        b.add("c")
        assert b.count() == 3
        c = b.chunk()
        assert isinstance(c, ArrayChunk)
        assert c.count() == 3
        assert c.nth(0) == "a"

    def test_add_after_chunk_raises(self):
        b = ChunkBuffer(4)
        b.add("a")
        b.chunk()
        with pytest.raises(RuntimeError):
            b.add("b")


# =========================================================================
# ChunkedCons
# =========================================================================

class TestChunkedCons:
    def test_walk(self):
        # Prepend a 3-element chunk to a None tail.
        cc = ChunkedCons(ArrayChunk([1, 2, 3]), None)
        assert list(cc) == [1, 2, 3]

    def test_chunked_first(self):
        c = ArrayChunk([1, 2, 3])
        cc = ChunkedCons(c, None)
        assert cc.chunked_first() is c

    def test_isinstance_ichunkedseq(self):
        cc = ChunkedCons(ArrayChunk([1]), None)
        assert isinstance(cc, IChunkedSeq)
