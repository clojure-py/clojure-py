# Port of clojure.lang.LazySeq.
#
# A LazySeq wraps a thunk that, on first seq(), is forced to produce the
# realized sequence (which may itself be another LazySeq, recursively). Once
# forced, the thunk and intermediate slot are cleared so the realized chain
# can be GC'd as a normal seq.
#
# Java's LazySeq is `synchronized` on every realization step. We use a
# per-instance threading.Lock (3.14t free-threaded → real concurrency).
#
# Note: NOT a subclass of ASeq (Java keeps it separate so realization can be
# transparent — first()/next()/more() force seq(), but seq() itself doesn't
# return self until forced).


cdef class LazySeq:
    """Lazily-realized sequence wrapping a thunk."""

    cdef object _fn          # thunk (callable returning seq); None after forced
    cdef object _sv          # intermediate value pending coercion to ISeq
    cdef object _seq         # realized ISeq (or None for empty)
    cdef object _meta
    cdef object _lock
    cdef int32_t _hash_cache
    cdef int32_t _hasheq_cache
    cdef object __weakref__

    def __cinit__(self, fn):
        self._fn = fn
        self._lock = Lock()

    cdef object _sval(self):
        # Force the thunk once; further calls return cached intermediate.
        # Caller must hold self._lock.
        if self._fn is not None:
            self._sv = self._fn()
            self._fn = None
        if self._sv is not None:
            return self._sv
        return self._seq

    def seq(self):
        with self._lock:
            self._sval()
            if self._sv is not None:
                ls = self._sv
                self._sv = None
                # Iteratively force nested LazySeqs to avoid stack growth.
                while isinstance(ls, LazySeq):
                    ls = (<LazySeq>ls)._sval_external()
                self._seq = _coerce_to_seq(ls)
            return self._seq

    cdef object _sval_external(self):
        # Same as _sval but acquires this LazySeq's own lock.
        with self._lock:
            return self._sval()

    def first(self):
        s = self.seq()
        if s is None:
            return None
        return s.first()

    def next(self):
        s = self.seq()
        if s is None:
            return None
        return s.next()

    def more(self):
        s = self.seq()
        if s is None:
            return _empty_list
        return s.more()

    def cons(self, o):
        return Cons(o, self.seq())

    def empty(self):
        return _empty_list

    def count(self):
        cdef int n = 0
        s = self.seq()
        while s is not None:
            n += 1
            s = s.next()
        return n

    def is_realized(self):
        with self._lock:
            return self._fn is None and self._sv is None

    def equiv(self, other):
        if not (isinstance(other, Sequential) or isinstance(other, (list, tuple))):
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
        # Capture the realized seq (forcing if needed) into a new LazySeq with
        # the new meta. Java does the same — withMeta after realization.
        s = self.seq()
        cdef LazySeq ls = LazySeq(lambda: s)
        ls._meta = meta
        return ls

    def __iter__(self):
        s = self.seq()
        while s is not None:
            yield s.first()
            s = s.next()

    def __len__(self):
        return self.count()

    def __bool__(self):
        return self.seq() is not None

    def __contains__(self, o):
        for x in self:
            if Util.equiv(x, o):
                return True
        return False

    def __str__(self):
        parts = []
        for x in self:
            parts.append(_print_str(x))
        return "(" + " ".join(parts) + ")"

    def __repr__(self):
        return self.__str__()


def _coerce_to_seq(o):
    # Minimal RT.seq replacement: ISeq → seq(), Seqable → seq(), iterable →
    # IteratorSeq, None → None.
    if o is None:
        return None
    if isinstance(o, Seqable):
        return o.seq()
    if isinstance(o, (list, tuple, str)):
        if len(o) == 0:
            return None
        return IteratorSeq.from_iterable(o)
    try:
        it = iter(o)
    except TypeError:
        raise TypeError(f"Don't know how to create ISeq from: {type(o).__name__}")
    return IteratorSeq.from_iterable(o)


ISeq.register(LazySeq)
IPersistentCollection.register(LazySeq)
Sequential.register(LazySeq)
IHashEq.register(LazySeq)
IMeta.register(LazySeq)
IObj.register(LazySeq)
IPending.register(LazySeq)
