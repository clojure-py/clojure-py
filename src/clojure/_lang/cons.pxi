# Port of clojure.lang.Cons. The minimal seq node — first + tail.


cdef class Cons(ASeq):
    """A cons cell: (first . rest)."""

    cdef readonly object _first
    cdef readonly object _more   # ISeq or None (tail)

    def __cinit__(self, first, more):
        self._first = first
        self._more = more

    def first(self):
        return self._first

    def next(self):
        # Java: more().seq()  — for empty more, returns null.
        if self._more is None:
            return None
        return self._more.seq()

    def more(self):
        if self._more is None:
            return _empty_list
        return self._more

    def count(self):
        # 1 + count of tail. Tail may be ISeq (walk) or Counted (O(1)).
        if self._more is None:
            return 1
        if isinstance(self._more, Counted):
            return 1 + self._more.count()
        # Defer to ASeq.count() walking semantics.
        cdef int i = 1
        s = self._more.seq() if isinstance(self._more, Seqable) else self._more
        while s is not None:
            if isinstance(s, Counted):
                return i + s.count()
            i += 1
            s = s.next()
        return i

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef Cons c = Cons(self._first, self._more)
        c._meta = meta
        return c
