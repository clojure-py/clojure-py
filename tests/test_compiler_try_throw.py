"""Compiler tests — throw + try/catch/finally.

Each (try ...) form is lifted into a nested 0-arg fn so the bytecode
library's "no nested TryBegin" restriction never bites. The catch
binding lives in the lifted frame; closures still work because the
lifted fn captures any outer locals normally."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
)


def _setup():
    ns = Compiler.current_ns()
    ns.import_class(ValueError)
    ns.import_class(TypeError)
    ns.import_class(ZeroDivisionError)
    ns.import_class(Exception)
    Var.intern(ns, Symbol.intern("ctf-mkve"), lambda msg: ValueError(msg))
    Var.intern(ns, Symbol.intern("ctf-mkte"), lambda msg: TypeError(msg))
    Var.intern(ns, Symbol.intern("ctf-add"), lambda a, b: a + b)


_setup()


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern(name, val):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), val)


# --- throw -------------------------------------------------------------

def test_throw_raises():
    with pytest.raises(ValueError, match="boom"):
        _eval('(throw (ctf-mkve "boom"))')

def test_throw_requires_one_arg():
    with pytest.raises(SyntaxError):
        _eval("(throw)")
    with pytest.raises(SyntaxError):
        _eval('(throw (ctf-mkve "a") (ctf-mkve "b"))')


# --- try without catch/finally degenerates to do -----------------------

def test_try_no_handlers_returns_body():
    assert _eval("(try 1 2 3)") == 3

def test_try_empty_body_is_nil():
    assert _eval("(try)") is None


# --- single catch ------------------------------------------------------

def test_catch_matching_class():
    assert _eval(
        '(try (throw (ctf-mkve "x")) (catch ValueError e "caught"))'
    ) == "caught"

def test_catch_binds_exception():
    _intern("ctf-msg", lambda e: str(e))
    assert _eval(
        '(try (throw (ctf-mkve "hello")) (catch ValueError e (ctf-msg e)))'
    ) == "hello"

def test_catch_non_matching_class_propagates():
    with pytest.raises(ValueError, match="oops"):
        _eval('(try (throw (ctf-mkve "oops")) (catch TypeError e :no))')

def test_catch_subclass_matches():
    _intern("ctf-mkze", lambda msg: ZeroDivisionError(msg))
    # ZeroDivisionError is a subclass of Exception
    assert _eval(
        '(try (throw (ctf-mkze "z")) (catch Exception e "any"))'
    ) == "any"


# --- multiple catches --------------------------------------------------

def test_multiple_catches_first_match_wins():
    assert _eval(
        '(try (throw (ctf-mkve "x")) '
        '  (catch ValueError e :ve) '
        '  (catch TypeError e :te))'
    ) == Keyword.intern(None, "ve")

def test_multiple_catches_second_matches():
    assert _eval(
        '(try (throw (ctf-mkte "x")) '
        '  (catch ValueError e :ve) '
        '  (catch TypeError e :te))'
    ) == Keyword.intern(None, "te")

def test_multiple_catches_none_match():
    with pytest.raises(ZeroDivisionError):
        _intern("ctf-mkze", lambda msg: ZeroDivisionError(msg))
        _eval(
            '(try (throw (ctf-mkze "z")) '
            '  (catch ValueError e :ve) '
            '  (catch TypeError e :te))'
        )


# --- finally -----------------------------------------------------------

def test_finally_runs_on_success():
    collected = []
    _intern("ctf-collect", lambda x: (collected.append(x), x)[1])
    assert _eval(
        '(try (ctf-collect :body) (finally (ctf-collect :final)))'
    ) == Keyword.intern(None, "body")
    assert collected == [Keyword.intern(None, "body"),
                         Keyword.intern(None, "final")]

def test_finally_runs_on_caught():
    collected = []
    _intern("ctf-collect2", lambda x: (collected.append(x), x)[1])
    assert _eval(
        '(try (throw (ctf-mkve "x")) '
        '  (catch ValueError e (ctf-collect2 :handler)) '
        '  (finally (ctf-collect2 :final)))'
    ) == Keyword.intern(None, "handler")
    assert collected == [Keyword.intern(None, "handler"),
                         Keyword.intern(None, "final")]

def test_finally_runs_on_uncaught_with_catches():
    collected = []
    _intern("ctf-collect3", lambda x: (collected.append(x), x)[1])
    with pytest.raises(ValueError):
        _eval(
            '(try (throw (ctf-mkve "x")) '
            '  (catch TypeError e :no) '
            '  (finally (ctf-collect3 :final)))'
        )
    assert collected == [Keyword.intern(None, "final")]

def test_finally_runs_on_uncaught_no_catches():
    collected = []
    _intern("ctf-collect4", lambda x: (collected.append(x), x)[1])
    with pytest.raises(ValueError):
        _eval(
            '(try (throw (ctf-mkve "x")) '
            '  (finally (ctf-collect4 :final)))'
        )
    assert collected == [Keyword.intern(None, "final")]


# --- nested try --------------------------------------------------------

def test_nested_try_inner_catch():
    """Inner try catches before outer sees the exception."""
    assert _eval(
        '(try (try (throw (ctf-mkve "x")) (catch ValueError e :inner)) '
        '  (catch Exception e :outer))'
    ) == Keyword.intern(None, "inner")

def test_nested_try_outer_catch():
    """Inner try doesn't match — outer catches."""
    assert _eval(
        '(try (try (throw (ctf-mkte "x")) (catch ValueError e :inner)) '
        '  (catch TypeError e :outer))'
    ) == Keyword.intern(None, "outer")


# --- closure into catch handler ---------------------------------------

def test_catch_handler_can_use_outer_locals():
    assert _eval(
        '(let* [base 100] '
        '  (try (throw (ctf-mkve "x")) '
        '    (catch ValueError e (ctf-add base 5))))'
    ) == 105

def test_try_value_in_expression_position():
    assert _eval(
        '(if (try (throw (ctf-mkve "x")) (catch ValueError e false)) :y :n)'
    ) == Keyword.intern(None, "n")


# --- error cases -------------------------------------------------------

def test_body_after_catch_raises():
    with pytest.raises(SyntaxError):
        _eval('(try (catch ValueError e :h) (do-something))')

def test_multiple_finally_raises():
    with pytest.raises(SyntaxError):
        _eval('(try 1 (finally :f1) (finally :f2))')
