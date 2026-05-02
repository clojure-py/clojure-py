# Port of clojure.lang.LongRange (and Range — collapsed since Python int has
# no overflow distinction).
#
# A Range is an immutable seq over [start, end) with a given step. start, end,
# and step are Python ints. step=0 means the iterate semantics — but Clojure's
# range does not allow step=0 (it'd be infinite); we preserve that constraint.


cdef class Range(ASeq):
    """An efficient seq over an integer interval [start, end) with given step."""

    cdef readonly object _start
    cdef readonly object _end
    cdef readonly object _step

    def __cinit__(self, start, end, step):
        if step == 0:
            raise ValueError("Range step cannot be zero")
        self._start = start
        self._end = end
        self._step = step

    @staticmethod
    def create(*args):
        """Range.create() / Range.create(end) / Range.create(start, end) /
        Range.create(start, end, step). Returns the empty list if the range
        is empty."""
        if len(args) == 0:
            # (range) — infinite from 0 with step 1
            return Iterate(lambda x: x + 1, 0)
        if len(args) == 1:
            return Range._from(0, args[0], 1)
        if len(args) == 2:
            return Range._from(args[0], args[1], 1)
        if len(args) == 3:
            return Range._from(args[0], args[1], args[2])
        raise TypeError(f"Range.create takes 0-3 args, got {len(args)}")

    @staticmethod
    cdef object _from(object start, object end, object step):
        if step > 0 and start >= end:
            return _empty_list
        if step < 0 and start <= end:
            return _empty_list
        return Range(start, end, step)

    def first(self):
        return self._start

    def next(self):
        cdef object nxt = self._start + self._step
        if self._step > 0 and nxt >= self._end:
            return None
        if self._step < 0 and nxt <= self._end:
            return None
        return Range(nxt, self._end, self._step)

    def count(self):
        # ceil((end - start) / step), clamped to >= 0.
        cdef object diff = self._end - self._start
        if (self._step > 0 and diff <= 0) or (self._step < 0 and diff >= 0):
            return 0
        # Integer ceiling division: (diff + step - sign) // step (sign of step).
        if self._step > 0:
            return (diff + self._step - 1) // self._step
        return (diff + self._step + 1) // self._step

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef Range r = Range(self._start, self._end, self._step)
        r._meta = meta
        return r

    def __contains__(self, o):
        # Optimized membership for Range when o is in the right form.
        if not (isinstance(o, int) and not isinstance(o, bool)):
            # Fall back to walking.
            return ASeq.__contains__(self, o)
        if self._step > 0:
            if o < self._start or o >= self._end:
                return False
        else:
            if o > self._start or o <= self._end:
                return False
        return (o - self._start) % self._step == 0


Counted.register(Range)
Indexed.register(Range)
