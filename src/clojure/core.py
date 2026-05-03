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
