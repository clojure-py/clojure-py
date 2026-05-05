# Port of clojure.lang.Delay.
#
# A Delay holds a 0-arg fn and evaluates it lazily on first force/deref,
# caching the result. Exceptions are also cached and re-raised on
# subsequent derefs to match JVM semantics.


class Delay:
    """clojure.lang.Delay — lazy 0-arg fn whose value is cached."""

    __slots__ = ("_fn", "_val", "_evaluated", "_exception")

    def __init__(self, fn):
        self._fn = fn
        self._val = None
        self._evaluated = False
        self._exception = None

    @staticmethod
    def force(x):
        """JVM static Delay.force — non-Delay x passes through."""
        if isinstance(x, Delay):
            if not x._evaluated:
                try:
                    x._val = x._fn()
                except BaseException as e:
                    x._exception = e
                    x._evaluated = True
                    x._fn = None
                    raise
                x._evaluated = True
                x._fn = None
            if x._exception is not None:
                raise x._exception
            return x._val
        return x

    def deref(self):
        return Delay.force(self)

    def is_realized(self):
        return self._evaluated


IDeref.register(Delay)
IPending.register(Delay)
