"""Tests for core.clj batch 17 (lines 3691-3769): print machinery.

Forms (10 + 3 vars):
  print-method (multimethod), print-dup (multimethod),
  pr-on (private), pr,
  system-newline (private), newline, flush,
  prn, print, println.

Plus dynamic vars *flush-on-newline* and *print-readably* (defined in
core.clj here) and *out* (pre-bound to sys.stdout by core.py
bootstrap).

Backend additions:
  clojure.lang.System    — minimal getProperty shim used to read
                           "line.separator" → os.linesep.
  JAVA_METHOD_FALLBACKS["append"] — Writer.append(charOrCS) on a
                           Python file-like falls through to .write,
                           returning the writer for chainability.

The :default print-method handles nil/bool/string conversions that
Python's str() gets wrong (None → "nil", True → "true", string with
quotes when *print-readably* is true). Per-type implementations for
collections, numbers, etc. inherit from Python's __str__, which our
PersistentVector / PersistentArrayMap / Keyword / Symbol / Ratio
already produce in JVM-readable form. A future batch will port
core_print.clj to fill in the dispatch table fully.
"""

import io
import sys

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace,
    PersistentArrayMap,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


# Capture *out* during a body of clojure forms, return what was written.
def _capture_out(*forms):
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    buf = io.StringIO()
    Var.push_thread_bindings(PersistentArrayMap.create(out_var, buf))
    try:
        for f in forms:
            E(f)
    finally:
        Var.pop_thread_bindings()
    return buf.getvalue()


# --- the System shim ----------------------------------------------

def test_system_get_property_line_separator():
    """Used by `system-newline`. JVM source: (System/getProperty "line.separator")."""
    sep = RT.class_for_name("clojure.lang.System").getProperty("line.separator")
    import os
    assert sep == os.linesep

def test_system_get_property_unknown_returns_default():
    sys_cls = RT.class_for_name("clojure.lang.System")
    assert sys_cls.getProperty("nonexistent.key") is None
    assert sys_cls.getProperty("nonexistent.key", "fallback") == "fallback"


# --- system-newline -----------------------------------------------

def test_system_newline_is_os_linesep():
    val = E("(clojure.core/var clojure.core/system-newline)").deref()
    import os
    assert val == os.linesep


# --- the dynamic vars ---------------------------------------------

def test_flush_on_newline_default_true():
    val = E("(clojure.core/var clojure.core/*flush-on-newline*)").deref()
    assert val is True

def test_print_readably_default_true():
    val = E("(clojure.core/var clojure.core/*print-readably*)").deref()
    assert val is True

def test_out_default_is_sys_stdout():
    val = E("(clojure.core/var clojure.core/*out*)").deref()
    assert val is sys.stdout


# --- pr -----------------------------------------------------------

def test_pr_no_args_returns_nil_and_writes_nothing():
    out = _capture_out("(clojure.core/pr)")
    assert out == ""

def test_pr_int():
    assert _capture_out("(clojure.core/pr 42)") == "42"

def test_pr_negative_int():
    assert _capture_out("(clojure.core/pr -7)") == "-7"

def test_pr_float():
    assert _capture_out("(clojure.core/pr 1.5)") == "1.5"

def test_pr_nil():
    assert _capture_out("(clojure.core/pr nil)") == "nil"

def test_pr_true():
    assert _capture_out("(clojure.core/pr true)") == "true"

def test_pr_false():
    assert _capture_out("(clojure.core/pr false)") == "false"

def test_pr_keyword():
    assert _capture_out("(clojure.core/pr :a)") == ":a"

def test_pr_symbol():
    assert _capture_out("(clojure.core/pr (quote foo))") == "foo"

def test_pr_string_readable():
    """Default *print-readably* is true → strings get quoted."""
    assert _capture_out('(clojure.core/pr "hello")') == '"hello"'

def test_pr_vector():
    assert _capture_out("(clojure.core/pr [1 2 3])") == "[1 2 3]"

def test_pr_map():
    """Map ordering depends on map type; just verify the format shape."""
    out = _capture_out("(clojure.core/pr {:a 1})")
    assert out == "{:a 1}"

def test_pr_list():
    assert _capture_out("(clojure.core/pr (quote (1 2 3)))") == "(1 2 3)"

def test_pr_variadic_inserts_spaces():
    assert _capture_out("(clojure.core/pr 1 2 3)") == "1 2 3"

def test_pr_variadic_mixed_types():
    out = _capture_out("(clojure.core/pr :a 1 nil)")
    assert out == ":a 1 nil"


# --- prn ----------------------------------------------------------

def test_prn_appends_newline():
    out = _capture_out("(clojure.core/prn 42)")
    assert out == "42\n"

def test_prn_no_args_just_newline():
    assert _capture_out("(clojure.core/prn)") == "\n"

def test_prn_variadic():
    assert _capture_out("(clojure.core/prn :a :b)") == ":a :b\n"


# --- print --------------------------------------------------------

def test_print_strips_quotes_from_strings():
    """print binds *print-readably* to nil, so strings render without quotes."""
    assert _capture_out('(clojure.core/print "hello")') == "hello"

def test_print_int_same_as_pr():
    assert _capture_out("(clojure.core/print 42)") == "42"


# --- println ------------------------------------------------------

def test_println_strips_quotes_and_adds_newline():
    assert _capture_out('(clojure.core/println "hello")') == "hello\n"

def test_println_no_args():
    assert _capture_out("(clojure.core/println)") == "\n"

def test_println_variadic():
    assert _capture_out("(clojure.core/println :a :b)") == ":a :b\n"


# --- newline / flush ----------------------------------------------

def test_newline_writes_system_newline():
    import os
    out = _capture_out("(clojure.core/newline)")
    assert out == os.linesep

def test_flush_does_not_throw():
    """StringIO has a no-op flush. Just verify the form returns nil."""
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    buf = io.StringIO()
    Var.push_thread_bindings(PersistentArrayMap.create(out_var, buf))
    try:
        assert E("(clojure.core/flush)") is None
    finally:
        Var.pop_thread_bindings()


# --- print-method / print-dup multimethods ------------------------

def test_print_method_is_a_multimethod():
    """print-method should be a clojure.lang.MultiFn."""
    from clojure.lang import MultiFn
    pm = E("(clojure.core/var clojure.core/print-method)").deref()
    assert isinstance(pm, MultiFn)

def test_print_dup_is_a_multimethod():
    from clojure.lang import MultiFn
    pd = E("(clojure.core/var clojure.core/print-dup)").deref()
    assert isinstance(pd, MultiFn)

def test_pr_on_dispatches_to_print_method_by_default():
    """*print-dup* defaults to false → pr-on uses print-method."""
    assert _capture_out("(clojure.core/pr 1)") == "1"

def test_pr_on_dispatches_to_print_dup_when_print_dup_set():
    """When *print-dup* is bound true, pr-on uses print-dup. Once
    core_print loads, print-dup for collections emits the JVM-style
    `#=(Class/create [...])` reader form so the output round-trips back
    to the same value via *read-eval*."""
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    print_dup_var = core_ns.find_interned_var(Symbol.intern("*print-dup*"))
    buf = io.StringIO()
    Var.push_thread_bindings(
        PersistentArrayMap.create(out_var, buf, print_dup_var, True))
    try:
        E("(clojure.core/pr [1 2 3])")
    finally:
        Var.pop_thread_bindings()
    out = buf.getvalue()
    assert out.startswith("#=(PersistentVector/create [")
    assert out.endswith("])")
    # Each element should print as #=(int. "N")
    for n in (1, 2, 3):
        assert ('#=(int. "%d")' % n) in out


# --- *flush-on-newline* observance --------------------------------

def test_prn_flushes_when_var_true():
    """Track flush calls via a writer that records them."""
    class CountingWriter:
        def __init__(self):
            self.buf = []
            self.flushes = 0
        def append(self, x):
            self.buf.append(str(x))
            return self
        def write(self, x):
            self.buf.append(x)
        def flush(self):
            self.flushes += 1

    w = CountingWriter()
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    Var.push_thread_bindings(PersistentArrayMap.create(out_var, w))
    try:
        E("(clojure.core/prn 42)")
    finally:
        Var.pop_thread_bindings()
    assert w.flushes == 1

def test_prn_does_not_flush_when_var_false():
    class CountingWriter:
        def __init__(self):
            self.buf = []
            self.flushes = 0
        def append(self, x):
            self.buf.append(str(x))
            return self
        def write(self, x):
            self.buf.append(x)
        def flush(self):
            self.flushes += 1

    w = CountingWriter()
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    flush_var = core_ns.find_interned_var(Symbol.intern("*flush-on-newline*"))
    Var.push_thread_bindings(
        PersistentArrayMap.create(out_var, w, flush_var, False))
    try:
        E("(clojure.core/prn 42)")
    finally:
        Var.pop_thread_bindings()
    assert w.flushes == 0


# --- *print-readably* observance ----------------------------------

def test_pr_with_print_readably_false_drops_string_quotes():
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    pr_readably = core_ns.find_interned_var(Symbol.intern("*print-readably*"))
    buf = io.StringIO()
    Var.push_thread_bindings(
        PersistentArrayMap.create(out_var, buf, pr_readably, None))
    try:
        E('(clojure.core/pr "hello")')
    finally:
        Var.pop_thread_bindings()
    assert buf.getvalue() == "hello"
