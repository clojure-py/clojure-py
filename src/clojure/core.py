"""clojure.core — Python entry point for the Clojure core library.

Importing this module loads `core.clj` from the same package directory,
binding *ns* to `clojure.core` for the duration of the load. After
import, every Var defined in core.clj is accessible via the
`clojure.core` namespace.
"""

import os as _os

from clojure.lang import (
    Compiler as _Compiler,
    Namespace as _Namespace,
    Symbol as _Symbol,
    RT as _RT,
)


class _Delay:
    """Compat shim for clojure.lang.Delay. A Delay holds a 0-arg fn and
    evaluates it lazily on first force/deref, caching the result."""

    __slots__ = ("_fn", "_val", "_evaluated", "_exception")

    def __init__(self, fn):
        self._fn = fn
        self._val = None
        self._evaluated = False
        self._exception = None

    @staticmethod
    def force(x):
        if isinstance(x, _Delay):
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
        return _Delay.force(self)

    def is_realized(self):
        return self._evaluated


class _StringBuilder:
    """Compat shim for java.lang.StringBuilder. Mutable string buffer
    used by clojure.core/str's variadic arity. Methods match the JVM
    surface that core.clj reaches for (.append returns self for
    chaining; .toString returns the joined string)."""

    __slots__ = ("_parts",)

    def __init__(self, s=""):
        self._parts = [s] if s else []

    def append(self, s):
        if s is None:
            self._parts.append("")
        else:
            self._parts.append(s if isinstance(s, str) else str(s))
        return self

    def __str__(self):
        return "".join(self._parts)

    def toString(self):
        return "".join(self._parts)


import collections as _collections


class _TransformerIterator:
    """Iterator-as-transducer driver. Equivalent to JVM
    clojure.lang.TransformerIterator: given a transducer `xform` and a
    source iterator, produces a new iterator that yields transformed
    values lazily.

    A transducer is a function `rf -> rf'` that takes a reducing function
    and returns a new reducing function. A reducing function rf is:
        rf()                init  → return seed
        rf(result)          done  → return final
        rf(result, input)   step  → return new result (or Reduced)

    To turn that into a pull-based iterator, we install a base rf that
    appends every step input into a buffer and have __next__ pull from
    the buffer, refilling it by stepping the source iterator one
    element at a time."""

    def __init__(self, xform, source_iter):
        self._buf = _collections.deque()
        self._src = iter(source_iter) if source_iter is not None else iter(())
        self._done = False
        # Sentinel-free 0-arity init isn't relevant for iterator use;
        # we only call the 1- and 2-arity branches.
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
        from clojure.lang import Reduced as _Reduced
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
            if isinstance(result, _Reduced):
                self._done = True
                # Run completion on the unwrapped value to flush.
                self._rf(result.deref())

    @staticmethod
    def create(xform, source_iter):
        return _TransformerIterator(xform, source_iter)

    @staticmethod
    def createMulti(xform, iter_seq):
        """Multi-source variant — used by the (sequence xform & colls)
        arity. Walks all source iterators in lock-step and feeds the rf
        with each tuple (or rather, the rf is invoked variadically with
        each set of items). When any source is exhausted the iteration
        ends."""
        iters = []
        s = iter_seq
        while s is not None:
            it = s.first() if hasattr(s, "first") else None
            if it is None:
                # iter_seq is a Clojure seq containing iterators; fall
                # back to Python iter if needed.
                it = s
            iters.append(iter(it))
            s = s.next() if hasattr(s, "next") else None

        # Wrap each .next() lockstep into a single-source iterator that
        # yields the *tuple* of inputs, then run xform with a base rf
        # that splats the tuple as variadic args.
        def lockstep():
            while True:
                try:
                    yield tuple(next(i) for i in iters)
                except StopIteration:
                    return

        from clojure.lang import Reduced as _Reduced
        ti = _TransformerIterator.__new__(_TransformerIterator)
        ti._buf = _collections.deque()
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

        # The xform's step receives (result, *items). To pass our tuple
        # as multiple inputs, wrap base_rf to accept (result, items_tuple)
        # and re-emit each item separately. But xform is built assuming
        # variadic step, so we instead make the input-splatting at the
        # TransformerIterator/createMulti caller's contract: rf is called
        # with (result, *tuple_items). The xform function pipeline handles
        # variadics through its (result input & inputs) arity.
        def variadic_rf(*args):
            if len(args) == 0 or len(args) == 1:
                return base_rf(*args)
            # args = (result, *items)
            buf.append(args[1:] if len(args) > 2 else args[1])
            return args[0]

        ti._rf = xform(variadic_rf)
        return ti


class _LazilyPersistentVector:
    """Compat shim for clojure.lang.LazilyPersistentVector. JVM Clojure
    uses the lazy variant to defer materialization; in our port we just
    build a persistent vector eagerly."""

    @staticmethod
    def create(coll):
        from clojure.lang import PersistentVector as _PV
        if coll is None:
            return _PV.EMPTY
        if isinstance(coll, _PV):
            return coll
        return _PV.from_iterable(coll)


def _bootstrap():
    """Pre-create the clojure.core namespace, install Java→Python class
    aliases that the translation references, then load core.clj."""
    import clojure.lang as _lang
    # Register the LazilyPersistentVector shim on the clojure.lang
    # module so `class_for_name("clojure.lang.LazilyPersistentVector")`
    # resolves it.
    setattr(_lang, "LazilyPersistentVector", _LazilyPersistentVector)
    setattr(_lang, "StringBuilder", _StringBuilder)
    setattr(_lang, "Delay", _Delay)
    setattr(_lang, "TransformerIterator", _TransformerIterator)

    core_ns = _Namespace.find_or_create(_Symbol.intern("clojure.core"))
    _RT.CURRENT_NS.bind_root(core_ns)

    # Java→Python class aliases. JVM Clojure auto-imports java.lang.*; we
    # mirror that for the specific names that appear in the translation.
    core_ns.import_class(_Symbol.intern("IllegalArgumentException"), ValueError)
    core_ns.import_class(_Symbol.intern("Character"), str)
    core_ns.import_class(_Symbol.intern("String"), str)
    core_ns.import_class(_Symbol.intern("Class"), type)
    core_ns.import_class(_Symbol.intern("Exception"), Exception)
    core_ns.import_class(_Symbol.intern("Boolean"), bool)
    core_ns.import_class(_Symbol.intern("ClassCastException"), TypeError)
    core_ns.import_class(_Symbol.intern("IllegalStateException"), RuntimeError)
    core_ns.import_class(_Symbol.intern("RuntimeException"), RuntimeError)
    core_ns.import_class(_Symbol.intern("StringBuilder"), _StringBuilder)
    core_ns.import_class(_Symbol.intern("Object"), object)
    import numbers as _numbers_mod
    core_ns.import_class(_Symbol.intern("Number"), _numbers_mod.Number)

    # Java numeric type aliases. JVM has Integer/Long/Short/Byte all as
    # distinct fixed-width int classes; in Python all ints are arbitrary
    # precision and indistinguishable, so they all map to `int`.
    # BigInteger maps to our BigInt subclass; Double to float.
    from clojure.lang import BigInt as _BigInt
    core_ns.import_class(_Symbol.intern("Integer"), int)
    core_ns.import_class(_Symbol.intern("Long"), int)
    core_ns.import_class(_Symbol.intern("Short"), int)
    core_ns.import_class(_Symbol.intern("Byte"), int)
    core_ns.import_class(_Symbol.intern("BigInteger"), _BigInt)
    core_ns.import_class(_Symbol.intern("Double"), float)
    core_ns.import_class(_Symbol.intern("Float"), float)

    # Pre-intern dynamic vars that core.clj references before they're
    # otherwise defined. *unchecked-math* is read inside :inline fn
    # bodies that compile (but never run) during the bootstrap.
    from clojure.lang import Var as _Var
    _Var.intern(core_ns,
                _Symbol.intern("*unchecked-math*"),
                False).set_dynamic()

    here = _os.path.dirname(_os.path.abspath(__file__))
    try:
        _Compiler.load_file(_os.path.join(here, "core.clj"))
    finally:
        # Restore *ns* to the user namespace so REPL / tests that assume
        # the user ns aren't disrupted by the bootstrap.
        user_ns = _Namespace.find_or_create(_Symbol.intern("user"))
        _RT.CURRENT_NS.bind_root(user_ns)


_bootstrap()
