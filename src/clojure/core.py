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

    # JVM has separate LongRange (int-only) and Range (any numeric).
    # Our Range covers both — alias LongRange to it.
    setattr(_lang, "LongRange", _lang.Range)

    # Math/ceil and friends — register Python's math module under "Math".
    import math as _math_mod
    setattr(_lang, "Math", _math_mod)

    # java.io.BufferedReader — wraps any object with .read() returning a
    # str (Python text files, io.StringIO, etc.). readLine matches Java's
    # contract: returns the next line *without* its terminator (\n, \r,
    # or \r\n, all three recognized) and returns None at EOF. We read
    # chars from an internal block buffer rather than relying on the
    # source's readline() so \r-only line breaks (e.g. in StringIO with
    # default newline) are split correctly.
    class _BufferedReader:
        __slots__ = ("_source", "_buf", "_pos", "_eof")

        _CHUNK = 4096

        def __init__(self, source):
            self._source = source
            self._buf = ""
            self._pos = 0
            self._eof = False

        def _refill_if_needed(self):
            if self._pos < len(self._buf):
                return True
            if self._eof:
                return False
            data = self._source.read(self._CHUNK)
            if not data:
                self._eof = True
                self._buf = ""
                self._pos = 0
                return False
            self._buf = data
            self._pos = 0
            return True

        def readLine(self):
            line_parts = []
            while True:
                if not self._refill_if_needed():
                    if line_parts:
                        return "".join(line_parts)
                    return None
                # Scan the current buffer for the next terminator.
                buf = self._buf
                pos = self._pos
                end = len(buf)
                start = pos
                while pos < end:
                    ch = buf[pos]
                    if ch == "\n":
                        line_parts.append(buf[start:pos])
                        self._pos = pos + 1
                        return "".join(line_parts)
                    if ch == "\r":
                        line_parts.append(buf[start:pos])
                        self._pos = pos + 1
                        # Look ahead for a paired \n (which we consume).
                        if self._refill_if_needed() and self._buf[self._pos] == "\n":
                            self._pos += 1
                        return "".join(line_parts)
                    pos += 1
                # No terminator in this chunk — append and refill.
                line_parts.append(buf[start:end])
                self._pos = end

        def close(self):
            close = getattr(self._source, "close", None)
            if close is not None:
                close()

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            self.close()
            return False

    # java.util.concurrent.TimeUnit — only the constants are used. Each
    # one converts a unit-quantity into seconds for Python's threading
    # primitives. Just the SI ladder; matches the JVM enum members.
    class _TimeUnit:
        __slots__ = ("_secs_per_unit", "_name")

        def __init__(self, secs_per_unit, name):
            self._secs_per_unit = secs_per_unit
            self._name = name

        def to_seconds(self, amount):
            return amount * self._secs_per_unit

        def __repr__(self):
            return f"<TimeUnit {self._name}>"

    _TimeUnit.NANOSECONDS  = _TimeUnit(1e-9,    "NANOSECONDS")
    _TimeUnit.MICROSECONDS = _TimeUnit(1e-6,    "MICROSECONDS")
    _TimeUnit.MILLISECONDS = _TimeUnit(1e-3,    "MILLISECONDS")
    _TimeUnit.SECONDS      = _TimeUnit(1.0,     "SECONDS")
    _TimeUnit.MINUTES      = _TimeUnit(60.0,    "MINUTES")
    _TimeUnit.HOURS        = _TimeUnit(3600.0,  "HOURS")
    _TimeUnit.DAYS         = _TimeUnit(86400.0, "DAYS")

    # java.util.concurrent.CountDownLatch — backs clojure.core/await
    # and friends. countDown decrements; await blocks until zero.
    # Note: Python forbids `def await(self):` at source level, but
    # bytecode-level LOAD_ATTR "await" works fine — we install via
    # setattr below.
    import threading as _threading_mod
    import time as _time_mod
    class _CountDownLatch:
        __slots__ = ("_count", "_cond")

        def __init__(self, count):
            if count < 0:
                raise ValueError(
                    "CountDownLatch count must be non-negative")
            self._count = count
            self._cond = _threading_mod.Condition()

        def countDown(self):
            with self._cond:
                if self._count > 0:
                    self._count -= 1
                    if self._count == 0:
                        self._cond.notify_all()

        def getCount(self):
            with self._cond:
                return self._count

        def _await_impl(self, *args):
            if len(args) == 0:
                with self._cond:
                    while self._count > 0:
                        self._cond.wait()
                return None
            if len(args) == 2:
                timeout, unit = args
                secs = unit.to_seconds(timeout)
                with self._cond:
                    if self._count == 0:
                        return True
                    deadline = _time_mod.monotonic() + secs
                    while self._count > 0:
                        remaining = deadline - _time_mod.monotonic()
                        if remaining <= 0:
                            return False
                        self._cond.wait(remaining)
                    return True
            raise TypeError(
                f"CountDownLatch.await takes 0 or 2 args, got {len(args)}")

    setattr(_CountDownLatch, "await", _CountDownLatch._await_impl)

    # java.util.Arrays/sort — minimal shim; sorts a list in place using a
    # comparator. Used by clojure.core/sort.
    #
    # The comparator may return either a 3-way int (-1/0/1, like
    # java.util.Comparator) or a boolean (like Clojure 2-arg predicates
    # such as `<` or `>`). JVM's clojure.lang.AFn.compare handles both:
    # if the call returns a Boolean, truthy means "x < y" (return -1);
    # otherwise re-invoke with swapped args to disambiguate equal vs.
    # greater. Mirror that here so `(sort > coll)` works.
    import functools as _functools
    class _JavaArrays:
        @staticmethod
        def sort(arr, comparator=None):
            if comparator is None:
                arr.sort()
            else:
                def _cmp(a, b):
                    r = comparator(a, b)
                    if isinstance(r, bool):
                        if r:
                            return -1
                        return 1 if comparator(b, a) else 0
                    return int(r)
                arr.sort(key=_functools.cmp_to_key(_cmp))
            return arr
    # Stash on the clojure.lang module under the dotted name. Our class
    # resolver handles `java.util.Arrays/sort` by importing the module
    # `java.util` and looking up Arrays — we don't have that. Easier:
    # register `java.util.Arrays` as a class attr on a fake module or
    # just install it where class_for_name finds it. The simplest is to
    # create a module-level attribute on a sub-package. Since
    # class_for_name does importlib.import_module on the leading
    # segment, we synthesize a `java.util` package in sys.modules.
    import types as _types_mod
    import sys as _sys_mod

    def _ensure_pkg(dotted):
        """Ensure sys.modules has a synthetic package for `dotted` and
        every ancestor, wiring each as an attribute of its parent.
        Returns the leaf module."""
        parts = dotted.split(".")
        parent = None
        for i in range(len(parts)):
            name = ".".join(parts[: i + 1])
            mod = _sys_mod.modules.get(name)
            if mod is None:
                mod = _types_mod.ModuleType(name)
                _sys_mod.modules[name] = mod
            if parent is not None:
                setattr(parent, parts[i], mod)
            parent = mod
        return parent

    _ensure_pkg("java.util").Arrays = _JavaArrays
    _ensure_pkg("java.io").BufferedReader = _BufferedReader
    _juc = _ensure_pkg("java.util.concurrent")
    _juc.CountDownLatch = _CountDownLatch
    _juc.TimeUnit = _TimeUnit

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
