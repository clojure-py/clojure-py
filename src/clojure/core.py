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
    core_ns.import_class(_Symbol.intern("StringBuilder"), _StringBuilder)
    core_ns.import_class(_Symbol.intern("Object"), object)

    here = _os.path.dirname(_os.path.abspath(__file__))
    try:
        _Compiler.load_file(_os.path.join(here, "core.clj"))
    finally:
        # Restore *ns* to the user namespace so REPL / tests that assume
        # the user ns aren't disrupted by the bootstrap.
        user_ns = _Namespace.find_or_create(_Symbol.intern("user"))
        _RT.CURRENT_NS.bind_root(user_ns)


_bootstrap()
