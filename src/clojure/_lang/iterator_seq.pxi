# Port of clojure.lang.IteratorSeq — wraps a Java Iterator (Python iterator
# in our port) into a stable, immutable ISeq.
#
# Each node holds its first value plus a lazy slot for the next IteratorSeq.
# The underlying iterator is shared, so traversing a seq advances the
# iterator only once per element. Forcing a node's `next()` may mutate the
# shared iterator state, so we lock per-node to keep concurrent traversals
# safe under 3.14t free-threading.


_NOT_REALIZED = object()


cdef class IteratorSeq(ASeq):
    """Stable seq view over an iterator. The iterator is shared with downstream
    nodes; forcing first()/next() pulls values from it once each."""

    cdef object _iter
    cdef object _val_slot     # _NOT_REALIZED until first() forces
    cdef object _next_slot    # _NOT_REALIZED until next() forces
    cdef object _lock

    def __cinit__(self, iter_or_iterable=None):
        # iter_or_iterable=None is the internal construction path (caller sets
        # fields via __new__ + direct attribute writes for chaining nodes).
        if iter_or_iterable is None:
            self._val_slot = _NOT_REALIZED
            self._next_slot = _NOT_REALIZED
            self._lock = Lock()
            return
        self._iter = iter(iter_or_iterable)
        self._val_slot = _NOT_REALIZED
        self._next_slot = _NOT_REALIZED
        self._lock = Lock()

    @staticmethod
    def from_iterable(it):
        # Construct from anything iterable; returns None if iter is exhausted.
        # We force the first slot eagerly here so we can distinguish "iterator
        # produced None as first value" (legitimate seq of length ≥ 1) from
        # "iterator empty" (returns None).
        cdef IteratorSeq seq = IteratorSeq(it)
        seq.first()  # force _val_slot
        if seq._val_slot is _SENTINEL_END:
            return None
        return seq

    def first(self):
        with self._lock:
            if self._val_slot is _NOT_REALIZED:
                try:
                    self._val_slot = next(self._iter)
                except StopIteration:
                    self._val_slot = _SENTINEL_END
            if self._val_slot is _SENTINEL_END:
                return None
            return self._val_slot

    def next(self):
        cdef IteratorSeq new_seq
        with self._lock:
            if self._next_slot is _NOT_REALIZED:
                # Make sure first() is realized before pulling further.
                if self._val_slot is _NOT_REALIZED:
                    try:
                        self._val_slot = next(self._iter)
                    except StopIteration:
                        self._val_slot = _SENTINEL_END
                if self._val_slot is _SENTINEL_END:
                    self._next_slot = None
                else:
                    # Try to pull the next value into a fresh node. Construct
                    # via the no-arg path so __cinit__ initializes slots, then
                    # overwrite _iter and _val_slot directly.
                    try:
                        nxt = next(self._iter)
                        new_seq = IteratorSeq()
                        new_seq._iter = self._iter
                        new_seq._val_slot = nxt
                        self._next_slot = new_seq
                    except StopIteration:
                        self._next_slot = None
            return self._next_slot

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        # IteratorSeq doesn't have a clean way to clone (the iterator is
        # consumed). Wrap in a LazySeq that returns self.
        cdef IteratorSeq new_seq = IteratorSeq.__new__(IteratorSeq)
        new_seq._iter = self._iter
        new_seq._val_slot = self._val_slot
        new_seq._next_slot = self._next_slot
        new_seq._lock = self._lock  # share — cloning the lock would defeat thread-safety
        new_seq._meta = meta
        return new_seq


cdef object _SENTINEL_END = object()
