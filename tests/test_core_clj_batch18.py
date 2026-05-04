"""Tests for core.clj batch 18 (lines 3771-3846): read API.

Forms (4):
  read, read+string, read-line, read-string

Backend additions:
  clojure.lang.LispReader  — static class shim with `read` overloads.
                             Wraps the existing module-level
                             `clojure.lang.read` so the JVM source
                             `(. clojure.lang.LispReader (read ...))`
                             works verbatim.
  RT.read_string           — counterpart to JVM RT.readString.
  BufferedReader.read_line — snake_case alias matching the rest of
                             the interop surface.
  Throwable                — imported into clojure.core as Python's
                             BaseException; used by read+string's
                             (catch Throwable ...) form.

Initial *in* binding: a LineNumberingPushbackReader wrapping
sys.stdin, mirroring JVM's wrapping of System.in.
"""

import io

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace,
    PersistentArrayMap,
    LineNumberingPushbackReader,
    LispReader,
    RT,
)


def E(src):
    return Compiler.eval(read_string(src))


def _bind_in(stream_text, fn=None):
    """Run with *in* bound to a LineNumberingPushbackReader over the
    given text. fn defaults to evaluating one Clojure form."""
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    in_var = core_ns.find_interned_var(Symbol.intern("*in*"))
    stream = LineNumberingPushbackReader(io.StringIO(stream_text))
    Var.push_thread_bindings(PersistentArrayMap.create(in_var, stream))
    try:
        return fn(stream) if fn else stream
    finally:
        Var.pop_thread_bindings()


# --- LispReader shim ----------------------------------------------

def test_lispreader_one_arg():
    r = LineNumberingPushbackReader(io.StringIO(":a :b"))
    assert LispReader.read(r) == E(":a")

def test_lispreader_three_arg_eof_no_throw():
    r = LineNumberingPushbackReader(io.StringIO(""))
    sentinel = object()
    assert LispReader.read(r, False, sentinel) is sentinel

def test_lispreader_four_arg():
    r = LineNumberingPushbackReader(io.StringIO("42 :tail"))
    assert LispReader.read(r, True, None, False) == 42

def test_lispreader_two_arg_with_map_treated_as_opts():
    r = LineNumberingPushbackReader(io.StringIO(":x"))
    assert LispReader.read(r, PersistentArrayMap.create()) == E(":x")

def test_lispreader_arity_error():
    r = LineNumberingPushbackReader(io.StringIO(""))
    with pytest.raises(TypeError, match="1-4 args"):
        LispReader.read()


# --- RT.read_string -----------------------------------------------

def test_rt_read_string_basic():
    assert RT.read_string("42") == 42

def test_rt_read_string_form():
    out = RT.read_string("(+ 1 2)")
    assert str(out) == "(+ 1 2)"

def test_rt_read_string_with_opts():
    """Opts form is accepted; with empty opts behaves like the no-opts form."""
    assert RT.read_string("42", PersistentArrayMap.create()) == 42

def test_rt_read_string_arity_error():
    with pytest.raises(TypeError, match="1 or 2 args"):
        RT.read_string()


# --- read-string (clojure-side) -----------------------------------

def test_read_string_int():
    assert E('(clojure.core/read-string "42")') == 42

def test_read_string_keyword():
    assert E('(clojure.core/read-string ":hello")') == E(":hello")

def test_read_string_form():
    out = E('(clojure.core/read-string "(+ 1 2)")')
    assert str(out) == "(+ 1 2)"

def test_read_string_with_opts():
    """[opts s] arity."""
    assert E('(clojure.core/read-string {} "99")') == 99

def test_read_string_invalid_raises():
    with pytest.raises(Exception):
        E('(clojure.core/read-string "(unbalanced")')


# --- read ---------------------------------------------------------

def test_read_default_uses_in():
    """No-arg read pulls from *in*."""
    def doit(_):
        return E("(clojure.core/read)")
    out = _bind_in(":hello", doit)
    assert out == E(":hello")

def test_read_consecutive_pulls_successive_forms():
    def doit(_):
        return [E("(clojure.core/read)") for _ in range(3)]
    out = _bind_in(":a 42 (foo bar)", doit)
    assert out[0] == E(":a")
    assert out[1] == 42
    assert str(out[2]) == "(foo bar)"

def test_read_one_arg_explicit_stream():
    """1-arity: (read stream) — JVM defaults eof-error?=true, eof-value=nil."""
    def doit(stream):
        # Fully qualify *in* since user ns can't see it bare.
        return E("(clojure.core/read clojure.core/*in*)")
    assert _bind_in("[1 2 3]", doit) == E("[1 2 3]")

def test_read_three_arg_eof_value_returned():
    def doit(stream):
        return E("(clojure.core/read clojure.core/*in* false :EOF)")
    assert _bind_in("", doit) == E(":EOF")

def test_read_three_arg_eof_throws():
    def doit(stream):
        return E("(clojure.core/read clojure.core/*in* true nil)")
    with pytest.raises(Exception):
        _bind_in("", doit)

def test_read_four_arg_recursive_flag():
    """Recursive flag is just passed through; we just verify the arity works."""
    def doit(stream):
        return E("(clojure.core/read clojure.core/*in* true nil false)")
    assert _bind_in(":x", doit) == E(":x")

def test_read_opts_stream_form():
    """5-arity: ([opts stream]). Empty opts behaves like default."""
    def doit(stream):
        return E("(clojure.core/read {} clojure.core/*in*)")
    assert _bind_in(":opts-form", doit) == E(":opts-form")


# --- read+string --------------------------------------------------

def test_read_plus_string_returns_form_and_text():
    def doit(_):
        return E("(clojure.core/read+string)")
    out = _bind_in("(a b c)", doit)
    # JVM returns a 2-vector [form text]
    assert str(out[0]) == "(a b c)"
    assert out[1] == "(a b c)"

def test_read_plus_string_trims_whitespace():
    def doit(_):
        return E("(clojure.core/read+string)")
    out = _bind_in("   :ws-form   ", doit)
    assert out[0] == E(":ws-form")
    assert out[1] == ":ws-form"

def test_read_plus_string_one_arg_explicit_stream():
    def doit(_):
        return E("(clojure.core/read+string clojure.core/*in*)")
    out = _bind_in("42", doit)
    assert out[0] == 42
    assert out[1] == "42"

def test_read_plus_string_eof_error_false():
    def doit(_):
        return E("(clojure.core/read+string clojure.core/*in* false :EOF)")
    out = _bind_in("", doit)
    # form is the eof value, text is empty (after stripping)
    assert out[0] == E(":EOF")
    assert out[1] == ""

def test_read_plus_string_opts_stream_form():
    def doit(_):
        return E("(clojure.core/read+string {} clojure.core/*in*)")
    out = _bind_in(" :opts ", doit)
    assert out[0] == E(":opts")
    assert out[1] == ":opts"


# --- read-line ----------------------------------------------------

def test_read_line_basic():
    def doit(_):
        return E("(clojure.core/read-line)")
    assert _bind_in("hello world\n", doit) == "hello world"

def test_read_line_consecutive():
    def doit(_):
        return [E("(clojure.core/read-line)") for _ in range(3)]
    out = _bind_in("alpha\nbeta\ngamma\n", doit)
    assert out == ["alpha", "beta", "gamma"]

def test_read_line_eof_returns_nil():
    def doit(_):
        # Drain content first.
        E("(clojure.core/read-line)")
        return E("(clojure.core/read-line)")
    assert _bind_in("only line\n", doit) is None

def test_read_line_no_trailing_newline():
    def doit(_):
        return E("(clojure.core/read-line)")
    assert _bind_in("no newline", doit) == "no newline"


# --- *in* default binding -----------------------------------------

def test_in_default_is_lnpr_over_stdin():
    val = E("(clojure.core/var clojure.core/*in*)").deref()
    assert isinstance(val, LineNumberingPushbackReader)

def test_throwable_imported_in_clojure_core():
    """read+string uses (catch Throwable ...). Make sure the alias resolves."""
    # Throwable is imported into clojure.core, not user — so qualify.
    assert RT.class_for_name("clojure.lang.System") is not None  # sanity
    # Most direct check: read+string compiled successfully (which means
    # Throwable resolved) — test that here by using read+string itself.
    def doit(_):
        return E("(clojure.core/read+string)")
    _bind_in(":any", doit)
