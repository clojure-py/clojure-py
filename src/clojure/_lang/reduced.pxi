# Port of clojure.lang.Reduced.
#
# A wrapper signaling "reduce should stop early and unwrap me". Used by
# IReduce / IReduceInit implementations and by Clojure's `reduced` /
# `reduced?` core fns.


cdef class Reduced:
    """Sentinel wrapper; deref to get the wrapped value."""

    cdef readonly object _val
    cdef object __weakref__

    def __cinit__(self, val):
        self._val = val

    def deref(self):
        return self._val

    def __repr__(self):
        return f"#<Reduced {self._val!r}>"


IDeref.register(Reduced)


def is_reduced(x):
    """Java RT.isReduced. True iff x is a Reduced."""
    return isinstance(x, Reduced)
