# Port of clojure.lang.AFn / RestFn / ArityException.
#
# JVM AFn declares 21 invoke() overloads (arity 0..20 + rest); each defaults
# to throwing ArityException, and concrete subclasses override the arities
# they actually support. RestFn extends this with another ~800 invoke()
# overloads to handle variadic dispatch.
#
# In Python with *args / **kwargs that all collapses to a single __call__.
# AFn is a Cython base class with a raising __call__; subclasses override
# with their actual signature. RestFn adds a `required_arity` slot and
# splits args into (fixed..., rest_seq).


class ArityException(Exception):
    """Raised when an IFn is invoked with an unsupported number of args."""

    def __init__(self, actual, name="<fn>"):
        self.actual = actual
        self.name = name
        super().__init__(f"Wrong number of args ({actual}) passed to: {name}")


cdef class AFn:
    """Concrete base for IFn implementations. Subclasses override __call__."""

    cdef object __weakref__

    def __call__(self, *args):
        raise ArityException(len(args), type(self).__name__)

    def apply_to(self, arglist):
        """Walk an ISeq (or any iterable) into positional args and invoke."""
        args = []
        if arglist is None:
            pass
        elif isinstance(arglist, ISeq):
            s = arglist
            while s is not None:
                args.append(s.first())
                s = s.next()
        elif isinstance(arglist, Seqable):
            s = arglist.seq()
            while s is not None:
                args.append(s.first())
                s = s.next()
        else:
            args = list(arglist)
        return self(*args)


IFn.register(AFn)


cdef class RestFn(AFn):
    """Variadic function base. Subclasses set required_arity to enforce a
    minimum arity and override do_invoke to receive (*fixed, rest_seq)."""

    def required_arity(self):
        return 0

    def do_invoke(self, *args):
        raise NotImplementedError("RestFn subclass must override do_invoke")

    def __call__(self, *args):
        cdef int req = self.required_arity()
        if len(args) < req:
            raise ArityException(len(args), type(self).__name__)
        fixed = args[:req]
        rest = args[req:]
        rest_seq = IteratorSeq.from_iterable(rest) if rest else None
        return self.do_invoke(*fixed, rest_seq)
