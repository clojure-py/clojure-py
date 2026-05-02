"""Compiler tests — fn* and closures.

Slice 5 — single-arity fn* only (multi-arity, varargs, and the recur
target come later). Closures use real Python __closure__ tuples; let*
bindings or args that get captured are promoted FAST→CELL in their
binding frame and propagated through any intermediate fn frames as
freevars."""

import types
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


_intern_fn("cfn-add", lambda a, b: a + b)
_intern_fn("cfn-mul", lambda a, b: a * b)
_intern_fn("cfn-dec", lambda x: x - 1)
_intern_fn("cfn-zero?", lambda x: x == 0)
_intern_fn("cfn-id", lambda x: x)


# --- basic fn* shape ---------------------------------------------------

def test_identity_fn():
    f = _eval("(fn* [x] x)")
    assert isinstance(f, types.FunctionType)
    assert f(42) == 42

def test_constant_fn_no_args():
    f = _eval("(fn* [] 99)")
    assert f() == 99
    # No captures => no closure tuple at all
    assert f.__closure__ is None

def test_two_arg_fn():
    f = _eval("(fn* [a b] (cfn-add a b))")
    assert f(10, 32) == 42

def test_three_arg_fn():
    f = _eval("(fn* [a b c] (cfn-add a (cfn-add b c)))")
    assert f(1, 2, 3) == 6

def test_fn_body_is_implicit_do():
    f = _eval("(fn* [x] x x x)")
    assert f(7) == 7
    f = _eval("(fn* [x] (cfn-add x 1) (cfn-add x 2))")
    assert f(10) == 12


# --- closures over let* bindings ---------------------------------------

def test_closure_over_let_binding():
    f = _eval("(let* [x 100] (fn* [y] (cfn-add x y)))")
    assert f(5) == 105
    assert f.__closure__ is not None
    assert len(f.__closure__) == 1

def test_closure_over_multiple_bindings():
    f = _eval("(let* [a 1 b 2 c 3] (fn* [] (cfn-add a (cfn-add b c))))")
    assert f() == 6
    assert len(f.__closure__) == 3

def test_closure_over_arg_of_outer_fn():
    f = _eval("(fn* [x] (fn* [y] (cfn-add x y)))")
    g = f(10)
    assert g(32) == 42

def test_currying():
    add = _eval("(fn* [a] (fn* [b] (fn* [c] (cfn-add a (cfn-add b c)))))")
    assert add(1)(2)(3) == 6


# --- multi-level closures (3+ deep) ------------------------------------

def test_three_level_closure_passthrough():
    """The middle fn does not reference x, but inner does — middle must
    plumb x through as a freevar so inner's __closure__ can receive it."""
    f = _eval("(let* [x 99] (fn* [] (fn* [] x)))")
    assert f()() == 99

def test_four_level_closure():
    f = _eval("(let* [x 7] (fn* [] (fn* [] (fn* [] x))))")
    assert f()()() == 7


# --- shadowing across closures -----------------------------------------

def test_inner_let_shadows_outer():
    f = _eval("(let* [x 1] (fn* [] (let* [x 99] x)))")
    assert f() == 99

def test_inner_arg_shadows_outer_let():
    f = _eval("(let* [x 1] (fn* [x] x))")
    assert f(99) == 99

def test_outer_x_visible_after_inner_shadow_ends():
    """Inside the inner fn body, after the inner let* ends, the outer x
    is again visible."""
    f = _eval("(let* [x 1] (fn* [] (let* [x 99] nil) x))")
    assert f() == 1


# --- recursion via named fn* -------------------------------------------

def test_named_fn_can_recurse():
    fact = _eval(
        "(fn* fact [n] (if (cfn-zero? n) 1 (cfn-mul n (fact (cfn-dec n)))))"
    )
    assert fact(0) == 1
    assert fact(1) == 1
    assert fact(5) == 120
    assert fact(10) == 3628800

def test_named_fn_name_invisible_outside():
    """The fn-name binding is in scope only inside the body."""
    Var.intern(Compiler.current_ns(), Symbol.intern("cfn-myname"), 999)
    # The (named) fn evaluates to a function, then we deref the var
    # `cfn-myname` outside it — should not be the function.
    val = _eval("(do (fn* cfn-myname [] cfn-myname) cfn-myname)")
    assert val == 999

def test_named_fn_with_closure():
    """Named fn that also closes over outer locals."""
    f = _eval(
        "(let* [base 100] "
        "  (fn* go [n] (if (cfn-zero? n) base (go (cfn-dec n)))))"
    )
    assert f(0) == 100
    assert f(5) == 100


# --- closure values change with the binding ----------------------------

def test_each_call_creates_fresh_closure_with_correct_value():
    """Each invocation of the outer fn binds a fresh `x` and produces a
    closure that captures THAT value."""
    make = _eval("(fn* [x] (fn* [] x))")
    f1 = make(1)
    f2 = make(2)
    f3 = make(3)
    assert f1() == 1
    assert f2() == 2
    assert f3() == 3


# --- mixed with if / do ------------------------------------------------

def test_fn_in_if_branch():
    f = _eval("(if true (fn* [x] (cfn-mul x 2)) nil)")
    assert f(5) == 10

def test_if_inside_fn_body():
    f = _eval("(fn* [x] (if (cfn-zero? x) :zero :nonzero))")
    from clojure.lang import Keyword
    assert f(0) == Keyword.intern(None, "zero")
    assert f(1) == Keyword.intern(None, "nonzero")

def test_fn_calls_var():
    """Inside fn body, a bare var symbol should still resolve and
    deref, exactly like at the top level."""
    Var.intern(Compiler.current_ns(), Symbol.intern("cfn-pi"), 3.14)
    f = _eval("(fn* [] cfn-pi)")
    assert f() == 3.14


# --- error cases -------------------------------------------------------

def test_fn_arglist_must_be_vector():
    with pytest.raises(SyntaxError):
        _eval("(fn* (x) x)")

def test_fn_arg_must_be_unqualified_symbol():
    with pytest.raises(SyntaxError):
        _eval("(fn* [foo/bar] bar)")
    with pytest.raises(SyntaxError):
        _eval("(fn* [42] 42)")

def test_fn_requires_arglist():
    with pytest.raises(SyntaxError):
        _eval("(fn*)")
