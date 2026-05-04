"""Tests for core.clj batch 26 (JVM lines 4825-4868):
the ExceptionInfo block.

Forms (5):
  ex-info, ex-data, ex-message, ex-cause,
  elide-top-frames (private, no-op in our port).

Plus:
  ExceptionInfo / IExceptionInfo aliases (def'd in core.clj header
  region rather than imported, so they're referred into `user`
  naturally).

Backend additions:
  clojure.lang.ExceptionInfo
    Python Exception subclass that carries a data map.
    .getData / .get_data / .getMessage / .getCause exposed for both
    JVM-style and snake_case access.
  clojure.lang.IExceptionInfo
    ABC with a single `getData` abstract method. ExceptionInfo is
    registered on it so isinstance? checks work.
  JAVA_METHOD_FALLBACKS["getMessage"] / ["getCause"]
    Throwable accessors that fall through to args[0]/__cause__ on
    plain Python exceptions, so ex-message / ex-cause work uniformly.

One adaptation from JVM source:
  elide-top-frames is a no-op. JVM rewrites the stack trace to drop
  the top frames matching a class name (so the helper itself doesn't
  show up in the user's stack). Python tracebacks are linked-list
  TBs without per-frame class names — there's no safe analog. We
  just return the exception unchanged. Functionally equivalent for
  ex-info's only callers.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    ExceptionInfo, IExceptionInfo,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- ExceptionInfo class ------------------------------------------

def test_exception_info_constructs_with_msg_and_data():
    e = ExceptionInfo("boom", {"k": 1})
    assert isinstance(e, Exception)
    assert e.args[0] == "boom"
    assert e.getData() == {"k": 1}

def test_exception_info_with_cause():
    inner = RuntimeError("inner")
    e = ExceptionInfo("outer", {}, inner)
    assert e.__cause__ is inner

def test_exception_info_is_iexception_info():
    e = ExceptionInfo("x", {})
    assert isinstance(e, IExceptionInfo)

def test_exception_info_repr():
    e = ExceptionInfo("hi", {"a": 1})
    r = repr(e)
    assert "ExceptionInfo" in r
    assert "hi" in r


# --- ex-info -------------------------------------------------------

def test_ex_info_two_arg():
    e = E('(ex-info "boom" {:k :v})')
    assert isinstance(e, ExceptionInfo)
    assert e.args[0] == "boom"
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-e"), e)
    data = E("(ex-data user/tcb26-e)")
    assert dict(data) == {K("k"): K("v")}

def test_ex_info_three_arg_with_cause():
    e = E('(ex-info "outer" {:why :test} (RuntimeException. "inner"))')
    assert e.__cause__ is not None
    assert e.__cause__.args[0] == "inner"

def test_ex_info_throw_and_catch():
    out = E("""
      (try
        (throw (ex-info "oops" {:reason :test :code 42}))
        (catch ExceptionInfo e
          (ex-data e)))""")
    assert dict(out) == {K("reason"): K("test"), K("code"): 42}


# --- ex-data -------------------------------------------------------

def test_ex_data_on_ex_info():
    out = E('(ex-data (ex-info "msg" {:a 1 :b 2}))')
    assert dict(out) == {K("a"): 1, K("b"): 2}

def test_ex_data_on_plain_exception_is_nil():
    assert E('(ex-data (RuntimeException. "plain"))') is None

def test_ex_data_on_nil_is_nil():
    assert E("(ex-data nil)") is None

def test_ex_data_on_non_exception_is_nil():
    """ex-data only returns when the arg is an IExceptionInfo."""
    assert E('(ex-data "just a string")') is None
    assert E("(ex-data 42)") is None


# --- ex-message ----------------------------------------------------

def test_ex_message_on_ex_info():
    assert E('(ex-message (ex-info "the msg" {:k :v}))') == "the msg"

def test_ex_message_on_plain_exception():
    """The JAVA_METHOD_FALLBACKS["getMessage"] takes care of plain
    Python exceptions — args[0] is treated as the message."""
    assert E('(ex-message (RuntimeException. "boom"))') == "boom"

def test_ex_message_on_nil():
    assert E("(ex-message nil)") is None

def test_ex_message_on_non_throwable():
    """Anything that isn't a Throwable returns nil."""
    assert E('(ex-message "string")') is None
    assert E("(ex-message 42)") is None

def test_ex_message_on_no_args_exception():
    """An exception with no args returns nil for getMessage."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-empty-ex"),
               RuntimeError())  # no args
    assert E("(ex-message user/tcb26-empty-ex)") is None


# --- ex-cause ------------------------------------------------------

def test_ex_cause_on_ex_info_with_cause():
    """Three-arg ex-info preserves the cause."""
    out = E("""
      (let [inner (RuntimeException. "root")
            outer (ex-info "wrapper" {} inner)]
        (= inner (ex-cause outer)))""")
    assert out is True

def test_ex_cause_on_ex_info_without_cause():
    assert E('(ex-cause (ex-info "msg" {}))') is None

def test_ex_cause_on_plain_exception_no_cause():
    assert E('(ex-cause (RuntimeException. "x"))') is None

def test_ex_cause_on_nil():
    assert E("(ex-cause nil)") is None

def test_ex_cause_uses_dunder_cause_not_context():
    """JVM's getCause semantics map to Python's __cause__ (explicit
    `raise X from Y`), not __context__ (implicit during-handling)."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-set-cause"),
               lambda e, c: setattr(e, "__cause__", c) or e)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-set-ctx"),
               lambda e, c: setattr(e, "__context__", c) or e)
    out_cause = E("""(let [inner (RuntimeException. "root")
                           outer (RuntimeException. "outer")
                           _    (user/tcb26-set-cause outer inner)]
                       (ex-cause outer))""")
    assert out_cause is not None
    assert out_cause.args[0] == "root"

    # __context__ alone shouldn't surface as cause.
    out_ctx = E("""(let [inner (RuntimeException. "ctx")
                         outer (RuntimeException. "x")
                         _    (user/tcb26-set-ctx outer inner)]
                     (ex-cause outer))""")
    assert out_ctx is None


# --- elide-top-frames (private no-op) -----------------------------

def test_elide_top_frames_returns_ex_unchanged():
    """Our port stubs elide-top-frames as a passthrough since Python
    tracebacks lack per-frame class names. Verify it doesn't mutate
    the exception."""
    elide_var = E("(clojure.core/var clojure.core/elide-top-frames)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-elide-fn"),
               elide_var.deref())
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-elide-ex"),
               RuntimeError("preserved"))
    out = E('(user/tcb26-elide-fn user/tcb26-elide-ex "any.classname")')
    assert out.args[0] == "preserved"


# --- JAVA_METHOD_FALLBACKS["getMessage"/"getCause"] ---------------

def test_get_message_fallback_on_python_exception():
    """Calling .getMessage on a plain Python exception goes through
    the fallback table — args[0] becomes the message."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-pyex"),
               RuntimeError("py-side"))
    assert E("(.getMessage user/tcb26-pyex)") == "py-side"

def test_get_cause_fallback_on_python_exception():
    """And .getCause returns __cause__."""
    inner = RuntimeError("root")
    outer = RuntimeError("outer")
    outer.__cause__ = inner
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb26-outer"), outer)
    out = E("(.getCause user/tcb26-outer)")
    assert out is inner
