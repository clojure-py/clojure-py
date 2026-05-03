# Port of clojure.lang.PersistentList.
#
# A singly-linked persistent list. Each node holds first / rest / count.
# count is cached so list.count() is O(1). cons is O(1) (links a new head).
# pop is O(1) (returns the existing tail). The empty case is represented by
# the EmptyList singleton from empty_list.pxi (aliased as PersistentList.EMPTY).


cdef class PersistentList(ASeq):
    """An immutable, singly-linked list."""

    cdef readonly object _first
    cdef readonly object _rest      # PersistentList or None (None at the singleton end)
    cdef readonly int _pl_count

    def __cinit__(self, first, rest=None, count=1):
        self._first = first
        self._rest = rest
        self._pl_count = count

    @staticmethod
    def create(iterable):
        """Build a PersistentList from a Python iterable, in order."""
        items = list(iterable)
        ret = _empty_list
        cdef Py_ssize_t i = len(items) - 1
        while i >= 0:
            ret = ret.cons(items[i])
            i -= 1
        return ret

    @staticmethod
    def creator(*items):
        """Variadic constructor — the Python equivalent of JVM
        PersistentList.creator (a static IFn that builds a list from
        its varargs)."""
        return PersistentList.create(items)

    def first(self):
        return self._first

    def next(self):
        if self._pl_count == 1:
            return None
        return self._rest

    def more(self):
        if self._pl_count == 1:
            return _empty_list
        return self._rest

    def peek(self):
        return self._first

    def pop(self):
        if self._rest is None:
            if self._meta is not None:
                return _empty_list.with_meta(self._meta)
            return _empty_list
        return self._rest

    def count(self):
        return self._pl_count

    def cons(self, o):
        cdef PersistentList pl = PersistentList(o, self, self._pl_count + 1)
        pl._meta = self._meta
        return pl

    def empty(self):
        if self._meta is not None:
            return _empty_list.with_meta(self._meta)
        return _empty_list

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef PersistentList pl = PersistentList(self._first, self._rest, self._pl_count)
        pl._meta = meta
        return pl

    def reduce(self, f, start=NOT_FOUND):
        """IReduce: reduce(f) and reduce(f, init). Honors Reduced for early
        termination."""
        if start is NOT_FOUND:
            ret = self._first
            s = self.next()
        else:
            ret = f(start, self._first)
            if isinstance(ret, Reduced):
                return (<Reduced>ret).deref()
            s = self.next()
        while s is not None:
            ret = f(ret, s.first())
            if isinstance(ret, Reduced):
                return (<Reduced>ret).deref()
            s = s.next()
        return ret


# PERSISTENT_LIST_EMPTY is the same EmptyList singleton used everywhere as
# "end of seq". Cython cdef classes don't allow setting class-level
# attributes after class definition, so the JVM-style PersistentList.EMPTY
# is exposed at module level instead.
PERSISTENT_LIST_EMPTY = _empty_list


IPersistentList.register(PersistentList)
IPersistentStack.register(PersistentList)
Sequential.register(PersistentList)
Counted.register(PersistentList)
IReduce.register(PersistentList)
IReduceInit.register(PersistentList)
