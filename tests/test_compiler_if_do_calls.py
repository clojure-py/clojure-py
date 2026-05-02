"""Compiler tests — `if`, `do`, and ordinary function-call forms."""

import pytest

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol,
)


def _eval(src):
    return Compiler.eval(read_string(src))


def _intern_fn(name, fn):
    Var.intern(Compiler.current_ns(), Symbol.intern(name), fn)


# --- if ----------------------------------------------------------------

def test_if_true():
    assert _eval("(if true 1 2)") == 1

def test_if_false():
    assert _eval("(if false 1 2)") == 2

def test_if_nil_is_falsy():
    assert _eval("(if nil :y :n)") == read_string(":n")

def test_if_zero_is_truthy_clojure():
    """In Clojure, only false and nil are falsy. 0, '', and empty colls
    are all truthy — unlike Python."""
    assert _eval("(if 0 :y :n)") == read_string(":y")

def test_if_empty_string_is_truthy_clojure():
    assert _eval('(if "" :y :n)') == read_string(":y")

def test_if_empty_list_is_truthy_clojure():
    assert _eval("(if '() :y :n)") == read_string(":y")

def test_if_no_else_branch_yields_nil_when_false():
    assert _eval("(if false 1)") is None

def test_if_no_else_branch_evaluates_then_when_true():
    assert _eval("(if true 99)") == 99

def test_if_too_many_args_raises():
    with pytest.raises(SyntaxError):
        _eval("(if true 1 2 3)")

def test_if_too_few_args_raises():
    with pytest.raises(SyntaxError):
        _eval("(if)")
    with pytest.raises(SyntaxError):
        _eval("(if true)")

def test_nested_if():
    assert _eval("(if true (if false :a :b) :c)") == read_string(":b")


# --- do ----------------------------------------------------------------

def test_do_returns_last():
    assert _eval("(do 1 2 3)") == 3

def test_do_evaluates_in_order():
    """Side-effect ordering — use a Var as a counter."""
    Var.intern(Compiler.current_ns(), Symbol.intern("c-counter"), [])
    _intern_fn("c-push!",
               lambda x: Compiler.eval(read_string("c-counter")).append(x) or x)
    _eval("(do (c-push! 1) (c-push! 2) (c-push! 3))")
    assert _eval("c-counter") == [1, 2, 3]

def test_do_empty_is_nil():
    assert _eval("(do)") is None

def test_do_single_form():
    assert _eval("(do 42)") == 42


# --- function calls ----------------------------------------------------

def test_call_one_arg():
    _intern_fn("c-inc", lambda x: x + 1)
    assert _eval("(c-inc 41)") == 42

def test_call_two_args():
    _intern_fn("c-add", lambda a, b: a + b)
    assert _eval("(c-add 10 32)") == 42

def test_call_three_args():
    _intern_fn("c-sum3", lambda a, b, c: a + b + c)
    assert _eval("(c-sum3 1 2 3)") == 6

def test_call_no_args():
    _intern_fn("c-five", lambda: 5)
    assert _eval("(c-five)") == 5

def test_call_nested():
    _intern_fn("c-double", lambda x: x * 2)
    _intern_fn("c-incr", lambda x: x + 1)
    assert _eval("(c-double (c-incr 4))") == 10

def test_call_with_quote_arg():
    _intern_fn("c-id", lambda x: x)
    assert _eval("(c-id 'foo)") == Symbol.intern("foo")

def test_call_inside_if():
    _intern_fn("c-triple", lambda x: x * 3)
    assert _eval("(if true (c-triple 7) 0)") == 21

def test_call_in_each_branch_of_if():
    _intern_fn("c-yes", lambda: "yes!")
    _intern_fn("c-no", lambda: "no!")
    assert _eval("(if true (c-yes) (c-no))") == "yes!"
    assert _eval("(if false (c-yes) (c-no))") == "no!"
