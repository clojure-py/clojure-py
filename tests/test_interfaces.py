"""Tests for the I* ABC layer in clojure.lang.

Verifies:
  - every interface is importable from clojure.lang
  - inheritance hierarchy matches the Java `extends` graph
  - abstract methods prevent instantiation
  - implementing all abstract methods allows instantiation
  - register() makes external types pass isinstance
"""
import pytest

from clojure.lang import (
    NOT_FOUND,
    # markers
    Sequential, IRecord, IType, MapEquivalence,
    # single-method
    Counted, Seqable, IHashEq, IMeta, Named, Reversible, Settable,
    IDeref, IBlockingDeref, IPending, IDrop,
    IReduceInit, IKVReduce,
    # multi-method
    Indexed, IObj, IFn, ILookup, Sorted, IMapEntry,
    IPersistentCollection, ISeq, IReduce, IPersistentStack,
    IChunk, IChunkedSeq,
    Associative, IPersistentList, IPersistentVector,
    IPersistentMap, IPersistentSet,
    # transients
    ITransientCollection, ITransientAssociative, ITransientAssociative2,
    ITransientMap, ITransientSet, ITransientVector,
    IEditableCollection,
)


# --- inheritance hierarchy ---

class TestHierarchy:
    """Mirror of the Java extends graph. Each pair (sub, super) asserts
    issubclass(sub, super)."""

    PAIRS = [
        # core single-link chains
        (Indexed, Counted),
        (IObj, IMeta),
        (IPersistentCollection, Seqable),
        (ISeq, IPersistentCollection),
        (IReduce, IReduceInit),
        (IPersistentStack, IPersistentCollection),
        # Associative is the diamond pivot
        (Associative, IPersistentCollection),
        (Associative, ILookup),
        # IPersistentList: Sequential + IPersistentStack
        (IPersistentList, Sequential),
        (IPersistentList, IPersistentStack),
        # IPersistentVector: 6-way (Associative, Sequential, Stack, Reversible, Indexed, IFn)
        (IPersistentVector, Associative),
        (IPersistentVector, Sequential),
        (IPersistentVector, IPersistentStack),
        (IPersistentVector, Reversible),
        (IPersistentVector, Indexed),
        (IPersistentVector, IFn),
        (IPersistentVector, Counted),  # via Indexed
        (IPersistentVector, ILookup),  # via Associative
        (IPersistentVector, IPersistentCollection),  # via several
        (IPersistentVector, Seqable),  # via IPersistentCollection
        # IPersistentMap: Associative + Counted
        (IPersistentMap, Associative),
        (IPersistentMap, Counted),
        # IPersistentSet
        (IPersistentSet, IPersistentCollection),
        (IPersistentSet, Counted),
        # transients
        (ITransientAssociative, ITransientCollection),
        (ITransientAssociative, ILookup),
        (ITransientAssociative2, ITransientAssociative),
        (ITransientMap, ITransientAssociative),
        (ITransientMap, Counted),
        (ITransientSet, ITransientCollection),
        (ITransientSet, Counted),
        (ITransientVector, ITransientAssociative),
        (ITransientVector, Indexed),
    ]

    def test_pairs(self):
        for sub, sup in self.PAIRS:
            assert issubclass(sub, sup), f"expected {sub.__name__} to subclass {sup.__name__}"


# --- abstract enforcement ---

class TestAbstractEnforcement:
    def test_cannot_instantiate_with_missing_methods(self):
        class HalfBakedSeq(ISeq):
            def first(self): return None
            # missing: next, more, count, cons, empty, equiv, seq

        with pytest.raises(TypeError):
            HalfBakedSeq()

    def test_can_instantiate_when_all_abstracts_filled(self):
        class TrivialSeq(ISeq):
            def first(self): return None
            def next(self): return None
            def more(self): return self
            def count(self): return 0
            def cons(self, o): return self
            def empty(self): return self
            def equiv(self, o): return o is self
            def seq(self): return None

        TrivialSeq()  # must not raise

    def test_marker_can_be_subclassed_with_no_methods(self):
        class Marked(Sequential):
            pass
        Marked()  # Sequential has no abstracts


# --- register() integration ---

class TestRegister:
    def test_register_makes_isinstance_true(self):
        class External:
            pass

        IFn.register(External)
        assert isinstance(External(), IFn)


# --- count / __len__ wiring on Counted ---

class TestCountedLen:
    def test_len_delegates_to_count(self):
        class Three(Counted):
            def count(self): return 3
        assert len(Three()) == 3


# --- IFn.apply_to default ---

class TestIFnApplyTo:
    def test_apply_to_walks_seq(self):
        # Minimal hand-rolled cons-cell ISeq for testing.
        class Cell(ISeq):
            def __init__(self, head, tail): self.head, self.tail = head, tail
            def first(self): return self.head
            def next(self): return self.tail
            def more(self): return self.tail if self.tail is not None else self
            def count(self):
                n, s = 0, self
                while s is not None:
                    n += 1
                    s = s.next()
                return n
            def cons(self, o): return Cell(o, self)
            def empty(self): return None
            def equiv(self, o): return o is self
            def seq(self): return self

        class Adder(IFn):
            def __call__(self, *args): return sum(args)

        seq = Cell(1, Cell(2, Cell(3, None)))
        assert Adder().apply_to(seq) == 6


# --- NOT_FOUND sentinel ---

class TestNotFoundSentinel:
    def test_distinct_from_none(self):
        assert NOT_FOUND is not None

    def test_identity_stable(self):
        from clojure.lang import NOT_FOUND as NF2
        assert NOT_FOUND is NF2
