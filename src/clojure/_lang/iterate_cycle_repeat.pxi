# Port of clojure.lang.Iterate, Cycle, Repeat.
#
# All three are infinite (or potentially-infinite) seqs realized lazily. They
# could be expressed as LazySeq composition, but we make them their own ASeq
# subclasses to match Java's structure and to keep memory footprint bounded
# (each step produces a new node referencing the same f or coll).


cdef class Iterate(ASeq):
    """(iterate f x) — the seq (x, f(x), f(f(x)), ...). Infinite."""

    cdef readonly object _f
    cdef object _seed
    cdef object _next_node       # cached on first next() call
    cdef object _lock

    def __cinit__(self, f, seed):
        self._f = f
        self._seed = seed
        self._next_node = None
        self._lock = Lock()

    @staticmethod
    def create(f, seed):
        return Iterate(f, seed)

    def first(self):
        return self._seed

    def next(self):
        with self._lock:
            if self._next_node is None:
                self._next_node = Iterate(self._f, self._f(self._seed))
            return self._next_node

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef Iterate it = Iterate(self._f, self._seed)
        it._meta = meta
        return it


cdef class Cycle(ASeq):
    """(cycle coll) — coll repeated forever. Infinite if coll is non-empty;
    behaves as the empty list when coll is empty."""

    cdef object _all              # original (non-empty) seq we keep cycling through
    cdef object _current          # current position in _all
    cdef object _next_node
    cdef object _lock

    def __cinit__(self, all, current):
        self._all = all
        self._current = current
        self._next_node = None
        self._lock = Lock()

    @staticmethod
    def create(coll):
        s = _coerce_to_seq(coll)
        if s is None:
            return _empty_list
        return Cycle(s, s)

    def first(self):
        return self._current.first()

    def next(self):
        with self._lock:
            if self._next_node is None:
                nxt = self._current.next()
                if nxt is None:
                    self._next_node = Cycle(self._all, self._all)
                else:
                    self._next_node = Cycle(self._all, nxt)
            return self._next_node

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef Cycle c = Cycle(self._all, self._current)
        c._meta = meta
        return c


cdef class Repeat(ASeq):
    """(repeat x) — infinite x's. (repeat n x) — n x's."""

    cdef readonly object _val
    cdef readonly object _count   # None for infinite, else remaining count

    def __cinit__(self, val, count):
        self._val = val
        self._count = count

    @staticmethod
    def create(*args):
        if len(args) == 1:
            return Repeat(args[0], None)        # infinite
        if len(args) == 2:
            n, val = args
            if n <= 0:
                return _empty_list
            return Repeat(val, n)
        raise TypeError(f"Repeat.create takes 1 or 2 args, got {len(args)}")

    def first(self):
        return self._val

    def next(self):
        if self._count is None:
            return self                          # infinite — never advance
        if self._count <= 1:
            return None
        return Repeat(self._val, self._count - 1)

    def count(self):
        if self._count is None:
            raise ArithmeticError("count not supported on infinite Repeat")
        return self._count

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef Repeat r = Repeat(self._val, self._count)
        r._meta = meta
        return r


# Iterate / Cycle are infinite — not Counted.
Counted.register(Repeat)   # only when finite (raises when infinite)
