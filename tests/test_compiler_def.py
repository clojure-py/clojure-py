"""Compiler tests — the `def` special form."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern_fn(name, fn):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), fn)


_intern_fn("cdf-add", lambda a, b: a + b)
_intern_fn("cdf-mul", lambda a, b: a * b)
_intern_fn("cdf-zero?", lambda x: x == 0)
_intern_fn("cdf-dec", lambda x: x - 1)


# --- def returns the Var -----------------------------------------------

def test_def_returns_var():
    v = _eval("(def cdf-x 42)")
    assert isinstance(v, Var)
    assert v.sym == Symbol.intern("cdf-x")

def test_def_sets_root():
    _eval("(def cdf-y 99)")
    assert _eval("cdf-y") == 99

def test_def_with_no_init_is_unbound():
    v = _eval("(def cdf-no-init)")
    assert isinstance(v, Var)
    assert not v.has_root()


# --- def evaluates the init form ---------------------------------------

def test_def_init_is_evaluated():
    _eval("(def cdf-sum (cdf-add 10 32))")
    assert _eval("cdf-sum") == 42

def test_def_can_use_other_vars_in_init():
    _eval("(def cdf-base 7)")
    _eval("(def cdf-derived (cdf-add cdf-base 1))")
    assert _eval("cdf-derived") == 8

def test_def_to_a_fn():
    _eval("(def cdf-inc (fn* [x] (cdf-add x 1)))")
    assert _eval("(cdf-inc 41)") == 42

def test_def_to_a_closure():
    _eval("(def cdf-make-adder (fn* [n] (fn* [x] (cdf-add n x))))")
    assert _eval("((cdf-make-adder 100) 5)") == 105


# --- forward reference via Var indirection -----------------------------

def test_def_can_reference_self_via_var():
    """The init form is a fn that references the same name being defined.
    Because the Var is interned at compile time, the inner lookup
    resolves and the resulting fn can recurse through the Var."""
    _eval(
        "(def cdf-fact "
        "  (fn* [n] (if (cdf-zero? n) 1 (cdf-mul n (cdf-fact (cdf-dec n))))))"
    )
    assert _eval("(cdf-fact 0)") == 1
    assert _eval("(cdf-fact 5)") == 120
    assert _eval("(cdf-fact 10)") == 3628800

def test_def_redef_replaces_root():
    _eval("(def cdf-redef 1)")
    assert _eval("cdf-redef") == 1
    _eval("(def cdf-redef 2)")
    assert _eval("cdf-redef") == 2


# --- docstring shape ---------------------------------------------------

def test_def_with_docstring():
    v = _eval('(def cdf-doc "the docstring" 42)')
    assert _eval("cdf-doc") == 42
    assert v.meta().val_at(Keyword.intern(None, "doc")) == "the docstring"

def test_def_docstring_only_with_init():
    """Without an init form following the docstring, the string is the
    init value, not metadata."""
    v = _eval('(def cdf-string-init "actually the value")')
    assert _eval("cdf-string-init") == "actually the value"


# --- error cases -------------------------------------------------------

def test_def_requires_a_name():
    with pytest.raises(SyntaxError):
        _eval("(def)")

def test_def_name_must_be_symbol():
    with pytest.raises(SyntaxError):
        _eval("(def 42 99)")
    with pytest.raises(SyntaxError):
        _eval('(def :kw 99)')

def test_def_name_must_be_unqualified():
    with pytest.raises(SyntaxError):
        _eval("(def other-ns/foo 1)")

def test_def_too_many_args():
    with pytest.raises(SyntaxError):
        _eval('(def cdf-too "doc" 1 2)')


# --- def works inside other forms --------------------------------------

def test_def_inside_do():
    _eval("(do (def cdf-in-do 7))")
    assert _eval("cdf-in-do") == 7

def test_def_inside_if_branch():
    _eval("(if true (def cdf-in-if 8) nil)")
    assert _eval("cdf-in-if") == 8
