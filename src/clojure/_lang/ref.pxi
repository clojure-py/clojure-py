# Port of clojure.lang.Ref.
#
# A Ref carries a circular doubly-linked list of TVal nodes (each value
# tagged with the commit point that produced it). LockingTransaction walks
# the chain to find the version visible at its read-point.
#
# Outside a transaction: deref returns the most recent val (read under the
# Ref's read lock); set/alter/commute/touch all raise (must be in a tx).
#
# Implements IFn (delegates to deref()'s value) and IRef (validator/watches
# inherited from ARef).


cdef object _REF_ID_COUNTER = _itertools.count()


cdef class _TVal:
    """One historical version of a Ref's value."""
    cdef public object val
    cdef public long point
    cdef public _TVal prior
    cdef public _TVal next_

    def __cinit__(self, val, long point, _TVal prior):
        self.val = val
        self.point = point
        if prior is None:
            # Solo node — circular onto itself.
            self.prior = self
            self.next_ = self
        else:
            # Splice in just-before prior in a circular list.
            self.prior = prior
            self.next_ = prior.next_
            prior.next_ = self
            self.next_.prior = self


cdef class Ref(ARef):
    """A coordinated, sync, optionally-versioned reference cell. Updates
    happen inside dosync transactions."""

    cdef public _TVal _tvals
    cdef public int _faults
    cdef public object _faults_lock
    cdef public object _lock           # _RWLock
    cdef public object _tinfo          # _LTInfo or None — current writer
    cdef public long _id
    cdef public int _min_history
    cdef public int _max_history

    def __init__(self, init_val=None, meta=None):
        ARef.__init__(self, meta)
        self._faults = 0
        self._faults_lock = Lock()
        self._lock = _RWLock()
        self._tinfo = None
        self._id = next(_REF_ID_COUNTER)
        self._min_history = 0
        self._max_history = 10
        self._tvals = _TVal(init_val, 0, None)

    # --- history bounds ---

    def get_min_history(self):
        return self._min_history

    def set_min_history(self, n):
        self._min_history = n
        return self

    def get_max_history(self):
        return self._max_history

    def set_max_history(self, n):
        self._max_history = n
        return self

    cdef int _hist_count(self):
        if self._tvals is None:
            return 0
        cdef _TVal tv = (<_TVal>self._tvals).next_
        cdef int count = 0
        while tv is not self._tvals:
            count += 1
            tv = tv.next_
        return count

    def get_history_count(self):
        # Walks under the write lock for consistency, like Java does.
        self._lock.acquire_write()
        try:
            return self._hist_count()
        finally:
            self._lock.release_write()

    def trim_history(self):
        self._lock.acquire_write()
        try:
            if self._tvals is not None:
                (<_TVal>self._tvals).next_ = <_TVal>self._tvals
                (<_TVal>self._tvals).prior = <_TVal>self._tvals
        finally:
            self._lock.release_write()

    # --- tx-scoped or solo reads/writes ---

    def deref(self):
        # In a transaction: read the version at our readPoint.
        # Outside: read the latest.
        t = LockingTransaction.get_running()
        if t is not None:
            return (<LockingTransaction>t)._do_get(self)
        return self.current_val()

    def current_val(self):
        self._lock.acquire_read()
        try:
            if self._tvals is None:
                raise RuntimeError(f"{self} is unbound.")
            return (<_TVal>self._tvals).val
        finally:
            self._lock.release_read()

    def set(self, val):
        return (<LockingTransaction>LockingTransaction.get_ex())._do_set(self, val)

    def alter(self, fn, *args):
        cdef LockingTransaction t = <LockingTransaction>LockingTransaction.get_ex()
        cur = t._do_get(self)
        new_val = fn(cur, *args)
        return t._do_set(self, new_val)

    def commute(self, fn, *args):
        # Args are passed as a Python list/tuple here — wrap as ISeq for
        # consistency with the rest of the LT internals (which walk via
        # .first()/.next()).
        cdef LockingTransaction t = <LockingTransaction>LockingTransaction.get_ex()
        if args:
            arg_seq = IteratorSeq.from_iterable(args)
        else:
            arg_seq = None
        return t._do_commute(self, fn, arg_seq)

    def touch(self):
        """`ensure` in Clojure parlance — pin this Ref's value within the tx
        without writing to it."""
        (<LockingTransaction>LockingTransaction.get_ex())._do_ensure(self)

    def is_bound(self):
        self._lock.acquire_read()
        try:
            return self._tvals is not None
        finally:
            self._lock.release_read()

    # --- IFn: deref to the underlying value and call ---

    def __call__(self, *args):
        return self.deref()(*args)

    def apply_to(self, arglist):
        target = self.deref()
        if hasattr(target, "apply_to"):
            return target.apply_to(arglist)
        return AFn.apply_to(self, arglist) if False else _seq_to_args_call(target, arglist)

    # --- ordering (Java has Comparable<Ref> via id; we expose __lt__ etc.) ---

    def __lt__(self, other):
        if not isinstance(other, Ref):
            return NotImplemented
        return self._id < (<Ref>other)._id

    def __le__(self, other):
        if not isinstance(other, Ref):
            return NotImplemented
        return self._id <= (<Ref>other)._id

    def __gt__(self, other):
        if not isinstance(other, Ref):
            return NotImplemented
        return self._id > (<Ref>other)._id

    def __ge__(self, other):
        if not isinstance(other, Ref):
            return NotImplemented
        return self._id >= (<Ref>other)._id

    def __hash__(self):
        # Identity hash — match default cdef class behavior, but explicit
        # because we defined comparison operators.
        return id(self) // 16

    def __eq__(self, other):
        return self is other

    def __ne__(self, other):
        return self is not other

    def __str__(self):
        return f"#<Ref @ {self._id}>"

    def __repr__(self):
        return self.__str__()


def _seq_to_args_call(target, arglist):
    args = []
    if arglist is not None:
        if isinstance(arglist, ISeq):
            s = arglist
            while s is not None:
                args.append(s.first())
                s = s.next()
        else:
            args = list(arglist)
    return target(*args)


IFn.register(Ref)
IRef.register(Ref)
IDeref.register(Ref)
