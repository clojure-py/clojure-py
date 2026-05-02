# Port of clojure.lang.PersistentQueue.
#
# Okasaki's batched queue using a PersistentVector for the rear (which is
# in-order, so no reverse / suspension needed). Front is an ISeq for fast
# popping; rear is a PersistentVector for fast cons-ing. When the front
# empties on pop, the rear's seq becomes the new front.


cdef class PersistentQueue:
    """FIFO queue. Conses onto rear, peeks/pops from front."""

    cdef readonly int _cnt
    cdef object _f       # ISeq or None (front)
    cdef object _r       # PersistentVector or None (rear)
    cdef object _meta
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self):
        pass

    @staticmethod
    cdef PersistentQueue _make(object meta, int cnt, object f, object r):
        cdef PersistentQueue q = PersistentQueue.__new__(PersistentQueue)
        q._meta = meta
        q._cnt = cnt
        q._f = f
        q._r = r
        return q

    @staticmethod
    def create(*items):
        ret = _PQ_EMPTY
        for x in items:
            ret = ret.cons(x)
        return ret

    @staticmethod
    def from_iterable(iterable):
        ret = _PQ_EMPTY
        for x in iterable:
            ret = ret.cons(x)
        return ret

    def count(self):
        return self._cnt

    def __len__(self):
        return self._cnt

    def peek(self):
        if self._f is None:
            return None
        return self._f.first()

    def pop(self):
        # Java: pop of empty queue → empty queue (NOT an exception).
        if self._f is None:
            return self
        f1 = self._f.next()
        r1 = self._r
        if f1 is None:
            # Front exhausted; promote rear into front.
            f1 = self._r.seq() if self._r is not None else None
            r1 = None
        return PersistentQueue._make(self._meta, self._cnt - 1, f1, r1)

    def cons(self, o):
        if self._f is None:
            # First element — start with a length-1 PersistentList as front.
            return PersistentQueue._make(self._meta, self._cnt + 1,
                                         PersistentList.create([o]), None)
        new_r = self._r if self._r is not None else _PV_EMPTY
        return PersistentQueue._make(self._meta, self._cnt + 1,
                                     self._f, new_r.cons(o))

    def seq(self):
        if self._f is None:
            return None
        rseq = self._r.seq() if self._r is not None else None
        return _PQSeq(self._f, rseq)

    def empty(self):
        if self._meta is None:
            return _PQ_EMPTY
        return PersistentQueue._make(self._meta, 0, None, None)

    def equiv(self, other):
        if other is self:
            return True
        if not (isinstance(other, Sequential) or isinstance(other, (list, tuple))):
            return False
        return _walk_equal(self, other, Util.equiv)

    def __eq__(self, other):
        if other is self:
            return True
        if not (isinstance(other, Sequential) or isinstance(other, (list, tuple))):
            return False
        return _walk_equal(self, other, Util.equals)

    def __ne__(self, other):
        return not self.__eq__(other)

    def __hash__(self):
        if self._hash_cache != 0:
            return self._hash_cache
        cdef uint32_t h = 1u
        for x in self:
            h = (31u * h + (0u if x is None else <uint32_t>(<int32_t>hash(x)))) & 0xFFFFFFFFu
        cdef int32_t result = <int32_t>h
        self._hash_cache = result
        return result

    def hasheq(self):
        if self._hasheq_cache != 0:
            return self._hasheq_cache
        result = Murmur3.hash_ordered(self)
        self._hasheq_cache = result
        return result

    def meta(self):
        return self._meta

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        return PersistentQueue._make(meta, self._cnt, self._f, self._r)

    def __iter__(self):
        s = self._f
        while s is not None:
            yield s.first()
            s = s.next()
        if self._r is not None:
            for x in self._r:
                yield x

    def __contains__(self, o):
        for x in self:
            if Util.equiv(x, o):
                return True
        return False

    def __bool__(self):
        return self._cnt > 0

    def __str__(self):
        # Clojure's queue printer uses the <-( ... )-< form.
        parts = []
        for x in self:
            parts.append(_print_str(x))
        return "<-(" + " ".join(parts) + ")-<"

    def __repr__(self):
        return self.__str__()


cdef class _PQSeq(ASeq):
    """Combined seq view: walks front, then rear."""

    cdef object _f      # ISeq
    cdef object _rseq   # ISeq from the rear vector, or None

    def __cinit__(self, f=None, rseq=None):
        if f is None:
            return
        self._f = f
        self._rseq = rseq

    def first(self):
        return self._f.first()

    def next(self):
        f1 = self._f.next()
        r1 = self._rseq
        if f1 is None:
            if self._rseq is None:
                return None
            f1 = self._rseq
            r1 = None
        return _PQSeq(f1, r1)

    def count(self):
        cdef int c = 0
        s = self._f
        while s is not None:
            c += 1
            s = s.next()
        s = self._rseq
        while s is not None:
            c += 1
            s = s.next()
        return c

    def with_meta(self, meta):
        cdef _PQSeq s = _PQSeq(self._f, self._rseq)
        s._meta = meta
        return s


cdef PersistentQueue _PQ_EMPTY = PersistentQueue._make(None, 0, None, None)
PERSISTENT_QUEUE_EMPTY = _PQ_EMPTY


IPersistentList.register(PersistentQueue)
IPersistentStack.register(PersistentQueue)
IPersistentCollection.register(PersistentQueue)
Sequential.register(PersistentQueue)
Counted.register(PersistentQueue)
IHashEq.register(PersistentQueue)
IMeta.register(PersistentQueue)
IObj.register(PersistentQueue)
