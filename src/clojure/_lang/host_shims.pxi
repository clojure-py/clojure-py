# Java host-class shims — counterparts to JVM classes that core.clj
# touches but Python doesn't naturally expose with the same shape.
# Each is a regular Python class (not cdef) included in the
# clojure.lang module via lang.pyx's include directives.
#
#   System         — java.lang.System (getProperty, nanoTime, currentTimeMillis)
#   BufferedReader — java.io.BufferedReader (readLine recognizing \n/\r/\r\n)
#   CountDownLatch — java.util.concurrent.CountDownLatch
#   TimeUnit       — java.util.concurrent.TimeUnit (constants only)
#   Arrays         — java.util.Arrays (just sort)
#   Array          — java.lang.reflect.Array (newInstance, get, set, typed setters)


import os as _os_mod
import sys as _sys_mod
import time as _time_mod
import threading as _threading_mod
import functools as _functools_mod
import array as _stdlib_array


# --- System -------------------------------------------------------

class System:
    """Subset of java.lang.System used by core.clj. getProperty maps a
    handful of well-known JVM properties to their Python equivalents;
    nanoTime and currentTimeMillis use Python's time module."""

    _props = {
        "line.separator": _os_mod.linesep,
        "file.separator": _os_mod.sep,
        "path.separator": _os_mod.pathsep,
    }

    @staticmethod
    def getProperty(name, default=None):
        return System._props.get(name, default)

    @staticmethod
    def nanoTime():
        """Monotonic high-resolution timer in nanoseconds."""
        return _time_mod.monotonic_ns()

    @staticmethod
    def currentTimeMillis():
        return int(_time_mod.time() * 1000)


# --- JavaMatcher --------------------------------------------------

import re as _re_mod


class JavaMatcher:
    """clojure.lang.JavaMatcher — counterpart to JVM
    java.util.regex.Matcher. JVM Matcher is stateful (tracks position
    across .find() calls); Python's re.Match is stateless. This
    wrapper bridges the gap by holding (pattern, string, pos,
    last_match) and exposing the JVM Matcher API surface that
    core.clj's re-* fns reach for: find, matches, group, groupCount."""

    __slots__ = ("_pattern", "_string", "_pos", "_last_match")

    def __init__(self, pattern, string):
        self._pattern = pattern
        self._string = string
        self._pos = 0
        self._last_match = None

    def find(self):
        """Advance past the last match and find the next. Returns True
        if a match is found, False at the end of the string. JVM
        Matcher.find avoids re-finding zero-length matches at the same
        position by advancing past them."""
        start = self._pos
        if self._last_match is not None:
            # If the previous match was zero-length, force advance by 1
            # to avoid an infinite loop.
            if self._last_match.start() == self._last_match.end():
                start = self._last_match.end() + 1
            else:
                start = self._last_match.end()
        if start > len(self._string):
            self._last_match = None
            return False
        m = self._pattern.search(self._string, start)
        if m is None:
            self._last_match = None
            return False
        self._last_match = m
        self._pos = m.end()
        return True

    def matches(self):
        """Try to match the entire input string."""
        m = self._pattern.fullmatch(self._string)
        if m is None:
            self._last_match = None
            return False
        self._last_match = m
        return True

    def group(self, *args):
        """No args → the full matched text. One int arg → the indexed
        capture group (0 = full match, 1+ = nested groups)."""
        if self._last_match is None:
            raise IllegalStateError("No match available")
        if len(args) == 0:
            return self._last_match.group()
        return self._last_match.group(args[0])

    def groupCount(self):
        """Number of capture groups in the pattern (excluding group 0,
        the full match) — matches JVM."""
        return self._pattern.groups


class IllegalStateError(RuntimeError):
    """Raised when a Matcher operation is called before a match has
    been established. Mirrors JVM's IllegalStateException."""
    pass


# --- ExceptionInfo ------------------------------------------------

class ExceptionInfo(Exception):
    """clojure.lang.ExceptionInfo — RuntimeException subclass that
    carries a map of additional data. JVM extends RuntimeException and
    implements IExceptionInfo; we extend Python's Exception (the
    closest equivalent) and register IExceptionInfo on the class."""

    def __init__(self, msg, data, cause=None):
        super().__init__(msg)
        self._data = data
        if cause is not None:
            self.__cause__ = cause

    def getData(self):
        """JVM-style accessor matching IExceptionInfo."""
        return self._data

    def get_data(self):
        return self._data

    def getMessage(self):
        return self.args[0] if self.args else None

    def getCause(self):
        return self.__cause__

    def __repr__(self):
        return ("ExceptionInfo("
                + repr(self.args[0] if self.args else None)
                + ", " + repr(self._data) + ")")


IExceptionInfo.register(ExceptionInfo)


# --- StringWriter -------------------------------------------------

class StringWriter:
    """clojure.lang.StringWriter — counterpart to java.io.StringWriter.
    Mutable text buffer that satisfies the same surface as our other
    Writer-like targets (.write / .append / .flush / .close) plus
    .toString and __str__ that return the accumulated text. Used by
    with-out-str to capture *out*."""

    __slots__ = ("_parts",)

    def __init__(self, s=""):
        self._parts = [s] if s else []

    def write(self, s):
        if s is None:
            return
        self._parts.append(s if isinstance(s, str) else str(s))

    def append(self, s):
        self.write(s)
        return self

    def flush(self):
        pass

    def close(self):
        pass

    def getvalue(self):
        return "".join(self._parts)

    def toString(self):
        return "".join(self._parts)

    def __str__(self):
        return "".join(self._parts)


# --- BufferedReader -----------------------------------------------

class BufferedReader:
    """clojure.lang.BufferedReader — char-and-buffer reader that
    recognizes \\n, \\r, and \\r\\n as line terminators (matching JVM's
    java.io.BufferedReader.readLine contract). Wraps any object with
    .read(n) returning a str."""

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
                    if self._refill_if_needed() and self._buf[self._pos] == "\n":
                        self._pos += 1
                    return "".join(line_parts)
                pos += 1
            line_parts.append(buf[start:end])
            self._pos = end

    # snake_case alias matching the rest of the interop surface.
    def read_line(self):
        return self.readLine()

    def close(self):
        close = getattr(self._source, "close", None)
        if close is not None:
            close()

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False


# --- TimeUnit -----------------------------------------------------

class TimeUnit:
    """Subset of java.util.concurrent.TimeUnit. Just the SI ladder; each
    constant carries a seconds-per-unit factor used by CountDownLatch."""

    __slots__ = ("_secs_per_unit", "_name")

    def __init__(self, secs_per_unit, name):
        self._secs_per_unit = secs_per_unit
        self._name = name

    def to_seconds(self, amount):
        return amount * self._secs_per_unit

    def __repr__(self):
        return "<TimeUnit " + self._name + ">"


TimeUnit.NANOSECONDS  = TimeUnit(1e-9,    "NANOSECONDS")
TimeUnit.MICROSECONDS = TimeUnit(1e-6,    "MICROSECONDS")
TimeUnit.MILLISECONDS = TimeUnit(1e-3,    "MILLISECONDS")
TimeUnit.SECONDS      = TimeUnit(1.0,     "SECONDS")
TimeUnit.MINUTES      = TimeUnit(60.0,    "MINUTES")
TimeUnit.HOURS        = TimeUnit(3600.0,  "HOURS")
TimeUnit.DAYS         = TimeUnit(86400.0, "DAYS")


# --- CountDownLatch -----------------------------------------------

class CountDownLatch:
    """clojure.lang.CountDownLatch — Condition-backed countdown.
    Counterpart to JVM's java.util.concurrent.CountDownLatch."""

    __slots__ = ("_count", "_cond")

    def __init__(self, count):
        if count < 0:
            raise ValueError("CountDownLatch count must be non-negative")
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
            "CountDownLatch.await takes 0 or 2 args, got " + str(len(args)))


# `await` is reserved at Python source level, but the bytecode-level
# LOAD_ATTR "await" works fine — install via setattr.
setattr(CountDownLatch, "await", CountDownLatch._await_impl)


# --- Arrays -------------------------------------------------------

class Arrays:
    """clojure.lang.Arrays — counterpart to java.util.Arrays. Only the
    static `sort` method is exposed since that's all clojure.core/sort
    needs. Comparator is treated JVM AFn-style: a Boolean return
    becomes -1 (truthy) or re-invoke-with-swapped-args, a 3-way int
    return passes through."""

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
            arr.sort(key=_functools_mod.cmp_to_key(_cmp))
        return arr


# --- Array (java.lang.reflect.Array equivalent) -------------------

# Type code mapping for Python's array.array. Numeric primitives use
# fixed-width homogeneous storage; everything else falls back to list.
_ARRAY_TYPE_CODES = {
    int:   "q",   # signed 64-bit
    float: "d",   # double, 64-bit
}


def _new_array_1d(type, size):
    code = _ARRAY_TYPE_CODES.get(type)
    if code is not None:
        return _stdlib_array.array(code, [0] * size)
    return [None] * size


def _new_array_multidim(type, dims):
    if len(dims) == 1:
        return _new_array_1d(type, dims[0])
    return [_new_array_multidim(type, dims[1:]) for _ in range(dims[0])]


class Array:
    """clojure.lang.Array — counterpart to java.lang.reflect.Array.
    Backs the alength/aclone/aget/aset/aset-X/make-array forms."""

    @staticmethod
    def newInstance(type, size_or_dimarray):
        if isinstance(size_or_dimarray, int) and not isinstance(size_or_dimarray, bool):
            return _new_array_1d(type, size_or_dimarray)
        return _new_array_multidim(type, list(size_or_dimarray))

    @staticmethod
    def getLength(arr):
        return len(arr)

    @staticmethod
    def get(arr, idx):
        return arr[idx]

    @staticmethod
    def set(arr, idx, val):
        arr[idx] = val
        return val

    @staticmethod
    def setInt(arr, idx, val):     arr[idx] = int(val);   return val
    @staticmethod
    def setLong(arr, idx, val):    arr[idx] = int(val);   return val
    @staticmethod
    def setShort(arr, idx, val):   arr[idx] = int(val);   return val
    @staticmethod
    def setByte(arr, idx, val):    arr[idx] = int(val);   return val
    @staticmethod
    def setFloat(arr, idx, val):   arr[idx] = float(val); return val
    @staticmethod
    def setDouble(arr, idx, val):  arr[idx] = float(val); return val
    @staticmethod
    def setBoolean(arr, idx, val): arr[idx] = bool(val);  return val

    @staticmethod
    def setChar(arr, idx, val):
        if isinstance(val, int):
            val = chr(val)
        elif isinstance(val, str) and len(val) == 1:
            pass
        else:
            raise TypeError("setChar: expected int or 1-char str")
        arr[idx] = val
        return val
