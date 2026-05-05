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


# Python function types — fn* compiles to a plain Python function, and
# built-in functions / bound methods are also "callable as IFn" for
# (ifn? f) checks. Register the common types so isinstance? / IFn-aware
# code sees them all as IFn.
import types as _types_for_ifn
IFn.register(_types_for_ifn.FunctionType)
IFn.register(_types_for_ifn.BuiltinFunctionType)
IFn.register(_types_for_ifn.MethodType)
IFn.register(_types_for_ifn.LambdaType)


def _ifn_register_cython_fn():
    """Cython-compiled functions land in a different type
    (`cython_function_or_method`) than ordinary Python functions. The
    compiler's _make_arity_dispatcher and the inline-fn wrappers it
    returns end up as that type when produced inside Cython context.
    Register that type with IFn too so (ifn? f) is True for built-ins
    like `+`. We grab the type at runtime from this module's own
    function objects."""
    cyfn_type = type(_ifn_register_cython_fn)
    IFn.register(cyfn_type)


_ifn_register_cython_fn()


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
