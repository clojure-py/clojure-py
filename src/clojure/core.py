"""clojure.core — Python entry point for the Clojure core library.

Importing this module loads `core.clj` from the same package directory,
binding *ns* to `clojure.core` for the duration of the load. After
import, every Var defined in core.clj is accessible via the
`clojure.core` namespace.

This file should stay small — host-class shims (Delay, StringBuilder,
System, BufferedReader, CountDownLatch, TimeUnit, Arrays, Array,
LispReader, TransformerIterator, LazilyPersistentVector) all live in
`_lang/*.pxi` files now and are compiled into the clojure.lang
extension module. Java→Python type aliases that were once interned
here are defined in core.clj using the `py.X/Y` resolver path.
"""

import os as _os
import sys as _sys

from clojure.lang import (
    Compiler as _Compiler,
    Namespace as _Namespace,
    Symbol as _Symbol,
    Var as _Var,
    RT as _RT,
    LineNumberingPushbackReader as _LNPR,
)


def _bootstrap():
    """Pre-create the clojure.core namespace, install the few dynamic-var
    initial values that need a Python-side default, then load core.clj."""
    import clojure.lang as _lang

    # JVM has separate LongRange (int-only) and Range (any numeric).
    # Our Range covers both — alias LongRange to it. (This is the only
    # remaining clojure.lang attribute alias; it can't go in a .pxi
    # because Range is itself defined in a .pxi included before this
    # alias would land.)
    setattr(_lang, "LongRange", _lang.Range)

    # Math/ceil and friends — register Python's math module under "Math"
    # so JVM source's `(. Math (ceil x))` and `Math/ceil` calls resolve.
    import math as _math_mod
    setattr(_lang, "Math", _math_mod)

    core_ns = _Namespace.find_or_create(_Symbol.intern("clojure.core"))
    _RT.CURRENT_NS.bind_root(core_ns)

    # Pre-intern dynamic vars that core.clj references before they're
    # otherwise defined. *unchecked-math* is read inside :inline fn
    # bodies that compile (but never run) during the bootstrap.
    _Var.intern(core_ns,
                _Symbol.intern("*unchecked-math*"),
                False).set_dynamic()

    # Print machinery — bind initial values that core.clj's first
    # references depend on. core.clj redefines these (or shadows their
    # values) using py.sys/stdout etc. once the resolver is reachable.
    _Var.intern(core_ns,
                _Symbol.intern("*out*"),
                _sys.stdout).set_dynamic()
    _Var.intern(core_ns,
                _Symbol.intern("*in*"),
                _LNPR(_sys.stdin)).set_dynamic()

    here = _os.path.dirname(_os.path.abspath(__file__))
    try:
        _Compiler.load_file(_os.path.join(here, "core.clj"))
    finally:
        # Restore *ns* to the user namespace so REPL / tests that assume
        # the user ns aren't disrupted by the bootstrap.
        user_ns = _Namespace.find_or_create(_Symbol.intern("user"))
        _RT.CURRENT_NS.bind_root(user_ns)


_bootstrap()
