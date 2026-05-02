# Port of clojure.lang.ASeq — abstract base class for sequences.
#
# Subclasses must implement first() and next(). ASeq provides defaults for
# count(), more(), seq(), cons(), empty(), equiv(), equals, hashCode, hasheq,
# __iter__, __str__, etc., matching Java semantics.


_SEQ_END = object()  # sentinel used by ASeq.equiv / __eq__ when iterators exhaust at different times


cdef class ASeq:
    """Abstract base for sequences. Cython subclasses (Cons, LazySeq, Range, …)
    override first() and next()."""

    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def first(self):
        raise NotImplementedError

    def next(self):
        raise NotImplementedError

    def more(self):
        s = self.next()
        if s is None:
            return _empty_list
        return s

    def seq(self):
        return self

    def empty(self):
        return _empty_list

    def cons(self, o):
        return Cons(o, self)

    def count(self):
        # Walk the seq. Counted nodes short-circuit (e.g. a Range tail).
        cdef int i = 1
        s = self.next()
        while s is not None:
            if isinstance(s, Counted):
                return i + s.count()
            i += 1
            s = s.next()
        return i

    # --- equality (Java ASeq.equiv / equals) ---

    def equiv(self, other):
        # Sequential or list/tuple comparison, element-wise via Util.equiv.
        if not (isinstance(other, Sequential) or isinstance(other, (list, tuple))):
            return False
        if isinstance(self, Counted) and isinstance(other, Counted):
            if self.count() != other.count():
                return False
        return _walk_equal(self, other, Util.equiv)

    def __eq__(self, other):
        if self is other:
            return True
        if not (isinstance(other, Sequential) or isinstance(other, (list, tuple))):
            return False
        return _walk_equal(self, other, Util.equals)

    def __ne__(self, other):
        return not self.__eq__(other)

    # --- hash (Java AbstractList.hashCode + Murmur3 hasheq) ---

    def __hash__(self):
        if self._hash_cache != 0:
            return self._hash_cache
        cdef uint32_t h = 1u
        s = self
        while s is not None:
            fst = s.first()
            h = (31u * h + (0u if fst is None else <uint32_t>(<int32_t>hash(fst)))) & 0xFFFFFFFFu
            s = s.next()
        cdef int32_t result = <int32_t>h
        self._hash_cache = result
        return result

    def hasheq(self):
        if self._hasheq_cache != 0:
            return self._hasheq_cache
        result = Murmur3.hash_ordered(self)
        self._hasheq_cache = result
        return result

    # --- meta / with_meta — subclasses should override with_meta to return correct subtype ---

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        raise NotImplementedError("subclass must override with_meta")

    # --- Python protocol ---

    def __iter__(self):
        s = self
        while s is not None:
            yield s.first()
            s = s.next()

    def __len__(self):
        return self.count()

    def __contains__(self, o):
        s = self
        while s is not None:
            if Util.equiv(s.first(), o):
                return True
            s = s.next()
        return False

    def __bool__(self):
        # ASeq is non-empty by definition (seq() returns self, never None).
        return True

    def __str__(self):
        # Approximation of RT.printString. Concrete pretty-printing comes later.
        parts = []
        s = self
        while s is not None:
            parts.append(_print_str(s.first()))
            s = s.next()
        return "(" + " ".join(parts) + ")"

    def __repr__(self):
        return self.__str__()


def _print_str(o):
    if o is None:
        return "nil"
    if isinstance(o, str):
        return '"' + o + '"'
    return str(o)


def _walk_equal(seq, other, eq_fn):
    # Walk both seq and other in parallel using Python iteration. Returns True
    # iff same length and every pair (a, b) satisfies eq_fn(a, b).
    si = iter(seq)
    oi = iter(other)
    while True:
        try:
            a = next(si)
        except StopIteration:
            a = _SEQ_END
        try:
            b = next(oi)
        except StopIteration:
            b = _SEQ_END
        if a is _SEQ_END and b is _SEQ_END:
            return True
        if a is _SEQ_END or b is _SEQ_END:
            return False
        if not eq_fn(a, b):
            return False


ISeq.register(ASeq)
IPersistentCollection.register(ASeq)
Sequential.register(ASeq)
IHashEq.register(ASeq)
IMeta.register(ASeq)
IObj.register(ASeq)
