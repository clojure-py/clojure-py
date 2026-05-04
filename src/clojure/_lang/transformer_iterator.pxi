# Port of clojure.lang.TransformerIterator.
#
# Iterator-as-transducer driver: given a transducer `xform` and a
# source iterator, produces a new iterator that yields transformed
# values lazily. Used by clojure.core/sequence's xform arities.
#
# A transducer is a function rf -> rf' that takes a reducing function
# and returns a new reducing function. A reducing function rf is:
#     rf()                init  → return seed
#     rf(result)          done  → return final
#     rf(result, input)   step  → return new result (or Reduced)
#
# To turn that into a pull-based iterator, we install a base rf that
# appends every step input into a buffer and have __next__ pull from
# the buffer, refilling by stepping the source iterator one element
# at a time.


import collections as _ti_collections


class TransformerIterator:

    def __init__(self, xform, source_iter):
        self._buf = _ti_collections.deque()
        self._src = iter(source_iter) if source_iter is not None else iter(())
        self._done = False
        buf = self._buf

        def base_rf(*args):
            if len(args) == 0:
                return None
            if len(args) == 1:
                return args[0]
            buf.append(args[1])
            return args[0]

        self._rf = xform(base_rf)

    def __iter__(self):
        return self

    def __next__(self):
        while True:
            if self._buf:
                return self._buf.popleft()
            if self._done:
                raise StopIteration
            try:
                src_val = next(self._src)
            except StopIteration:
                self._done = True
                # Completion step — flushes any buffered tail (e.g. from
                # partition-all, take-while-with-pending, etc.).
                self._rf(None)
                continue
            result = self._rf(None, src_val)
            if isinstance(result, Reduced):
                self._done = True
                # Run completion on the unwrapped value to flush.
                self._rf(result.deref())

    @staticmethod
    def create(xform, source_iter):
        return TransformerIterator(xform, source_iter)

    @staticmethod
    def createMulti(xform, iter_seq):
        """Multi-source variant used by (sequence xform & colls). Walks
        all source iterators in lock-step; the rf is invoked variadically
        with each set of items. When any source is exhausted iteration
        ends."""
        iters = []
        s = iter_seq
        while s is not None:
            it = s.first() if hasattr(s, "first") else None
            if it is None:
                it = s
            iters.append(iter(it))
            s = s.next() if hasattr(s, "next") else None

        def lockstep():
            while True:
                try:
                    yield tuple(next(i) for i in iters)
                except StopIteration:
                    return

        ti = TransformerIterator.__new__(TransformerIterator)
        ti._buf = _ti_collections.deque()
        ti._src = lockstep()
        ti._done = False
        buf = ti._buf

        def base_rf(*args):
            if len(args) == 0:
                return None
            if len(args) == 1:
                return args[0]
            buf.append(args[1])
            return args[0]

        def variadic_rf(*args):
            if len(args) == 0 or len(args) == 1:
                return base_rf(*args)
            buf.append(args[1:] if len(args) > 2 else args[1])
            return args[0]

        ti._rf = xform(variadic_rf)
        return ti
