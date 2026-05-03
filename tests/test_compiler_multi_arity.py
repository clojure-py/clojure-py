"""Compiler tests — multi-arity fn*.

Each arity body compiles to its own Python function; a runtime
dispatcher (_make_arity_dispatcher) selects the right one based on
argument count. For named multi-arity, the self-cell ends up holding
the dispatcher so recursive calls through the name go through dispatch."""

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


_intern_fn("cma-add", lambda a, b: a + b)
_intern_fn("cma-mul", lambda a, b: a * b)
_intern_fn("cma-zero?", lambda x: x == 0)
_intern_fn("cma-dec", lambda x: x - 1)


# --- two arities -------------------------------------------------------

def test_two_arities_zero_and_one():
    f = _eval("(fn* ([] :zero) ([x] x))")
    assert f() == Keyword.intern(None, "zero")
    assert f(7) == 7

def test_two_arities_one_and_two():
    f = _eval("(fn* ([x] x) ([x y] (cma-add x y)))")
    assert f(5) == 5
    assert f(10, 32) == 42


# --- three arities + varargs -------------------------------------------

def test_three_arities_with_vararg():
    f = _eval("(fn* ([] :none) ([x] x) ([x & rest] rest))")
    assert f() == Keyword.intern(None, "none")
    assert f(99) == 99
    result = f(1, 2, 3)
    assert list(result) == [2, 3]

def test_vararg_arity_matches_when_others_dont():
    f = _eval("(fn* ([] :none) ([x & rest] (cma-add x 1)))")
    assert f() == Keyword.intern(None, "none")
    assert f(41) == 42
    assert f(41, 99) == 42


# --- named multi-arity recursion ---------------------------------------

def test_named_multi_arity_recursion():
    """Recursion through name: each arity calls the dispatcher (held in
    the self-cell), which dispatches to the right overload."""
    fact = _eval(
        "(fn* fact "
        "  ([n] (fact n 1)) "
        "  ([n acc] (if (cma-zero? n) acc (fact (cma-dec n) (cma-mul acc n)))))"
    )
    assert fact(0) == 1
    assert fact(5) == 120
    assert fact(10) == 3628800

def test_named_multi_arity_self_invoke_via_var_after_def():
    _eval(
        "(def cma-myfn "
        "  (fn* mf ([] (mf 0 0)) ([a b] (cma-add a b))))"
    )
    assert _eval("(cma-myfn)") == 0
    assert _eval("(cma-myfn 10 32)") == 42


# --- closures across arities -------------------------------------------

def test_each_arity_can_capture_outer():
    """Each arity body is compiled with the same outer chain — closures
    work uniformly."""
    f = _eval(
        "(let* [base 100] "
        "  (fn* ([] base) ([x] (cma-add base x))))"
    )
    assert f() == 100
    assert f(5) == 105


# --- error cases -------------------------------------------------------

def test_no_matching_arity_raises():
    f = _eval("(fn* ([x] x) ([x y] x))")
    with pytest.raises(TypeError):
        f()
    with pytest.raises(TypeError):
        f(1, 2, 3)

def test_more_than_one_variadic_overload_raises():
    """The dispatcher rejects this at construction time."""
    with pytest.raises(SyntaxError):
        _eval("(fn* ([& a] a) ([& b] b))")


# --- single arity in parens still works (no dispatcher needed) ---------

def test_single_arity_wrapped_in_parens():
    """`(fn* ([x] x))` is one arity in extra parens. We still skip the
    dispatcher (single-arity special case) — Python's own arity check
    handles wrong counts."""
    f = _eval("(fn* ([x] x))")
    assert f(7) == 7
    with pytest.raises(TypeError):
        f()
