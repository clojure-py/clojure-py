"""Compiler tests — recur in fn-tail position (no enclosing loop).

The fn's own args act as recur targets. Implementation-wise: fn args are
marked FAST_LOOP and a recur target is pushed at the start of the body
(after the prologue), so subsequent iterations skip prologue work like
the *rest seq-conversion."""

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


_intern_fn("ftr-add", lambda a, b: a + b)
_intern_fn("ftr-mul", lambda a, b: a * b)
_intern_fn("ftr-dec", lambda x: x - 1)
_intern_fn("ftr-inc", lambda x: x + 1)
_intern_fn("ftr-zero?", lambda x: x == 0)
_intern_fn("ftr-lt", lambda a, b: a < b)


# --- single-arg recur --------------------------------------------------

def test_simple_countdown():
    f = _eval("(fn* [n] (if (ftr-zero? n) :done (recur (ftr-dec n))))")
    assert f(0) == Keyword.intern(None, "done")
    assert f(5) == Keyword.intern(None, "done")
    assert f(100) == Keyword.intern(None, "done")


# --- multi-arg recur ---------------------------------------------------

def test_factorial_via_fn_tail_recur():
    fact = _eval(
        "(fn* [n acc] "
        "  (if (ftr-zero? n) acc (recur (ftr-dec n) (ftr-mul acc n))))"
    )
    assert fact(0, 1) == 1
    assert fact(5, 1) == 120
    assert fact(10, 1) == 3628800

def test_sum_loop_via_fn_recur():
    f = _eval(
        "(fn* [i acc] "
        "  (if (ftr-lt 10 i) acc (recur (ftr-inc i) (ftr-add acc i))))"
    )
    assert f(1, 0) == 55


# --- recur arity check -------------------------------------------------

def test_recur_arity_must_match_fn_args():
    with pytest.raises(SyntaxError):
        _eval("(fn* [a b] (recur 1))")
    with pytest.raises(SyntaxError):
        _eval("(fn* [a] (recur 1 2))")


# --- nested loop+recur shadows fn-args -------------------------------

def test_loop_inside_fn_recur_targets_loop_not_fn():
    """When both a loop* and the enclosing fn have recur targets, the
    inner recur targets the innermost (loop)."""
    f = _eval(
        "(fn* [start] "
        "  (loop* [i start acc 0] "
        "    (if (ftr-zero? i) acc (recur (ftr-dec i) (ftr-add acc i)))))"
    )
    assert f(5) == 15  # 1+2+3+4+5

def test_loop_recur_then_outer_arg_still_visible():
    f = _eval(
        "(fn* [base] "
        "  (loop* [i 0 acc 0] "
        "    (if (ftr-lt 4 i) "
        "      (ftr-add base acc) "
        "      (recur (ftr-inc i) (ftr-add acc i)))))"
    )
    assert f(100) == 110  # base=100, 0+1+2+3+4=10 → 110


# --- recur with vararg fn ----------------------------------------------

def test_recur_in_vararg_fn():
    """Vararg fn's rest-arg-as-seq is the recur target slot. Recur
    passes a seq directly; subsequent iterations skip the prologue
    seq-conversion (the recur label is past it)."""
    f = _eval(
        "(fn* [n & rest] "
        "  (if (ftr-zero? n) rest (recur (ftr-dec n) rest)))"
    )
    result = f(3, 1, 2, 3)
    assert list(result) == [1, 2, 3]


# --- captured fn-arg through inner closure -----------------------------

def test_inner_closure_captures_fn_arg_value_at_creation():
    """An inner fn that captures an outer fn's arg should see the value
    at the time of inner-fn creation. With fn args being FAST_LOOP, the
    capture goes through freshen — same semantics as if the arg had
    never been recurred."""
    make = _eval("(fn* [n] (fn* [] n))")
    assert make(7)() == 7
    # If the outer fn recurs, each iteration's inner closure sees the
    # value from THAT iteration:
    collected = []
    Var.intern(Compiler.current_ns(), Symbol.intern("ftr-collected"), collected)
    _intern_fn("ftr-collect!", lambda f: (collected.append(f), f)[1])
    f = _eval(
        "(fn* [i] "
        "  (do (ftr-collect! (fn* [] i)) "
        "      (if (ftr-zero? i) :done (recur (ftr-dec i)))))"
    )
    f(2)
    assert [x() for x in collected] == [2, 1, 0]
    collected.clear()


# --- works in named fn ------------------------------------------------

def test_named_fn_with_tail_recur():
    f = _eval(
        "(fn* loop-fn [n] "
        "  (if (ftr-zero? n) :ok (recur (ftr-dec n))))"
    )
    assert f(50) == Keyword.intern(None, "ok")


# --- works in multi-arity arms ----------------------------------------

def test_multi_arity_each_arm_has_own_recur_target():
    f = _eval(
        "(fn* "
        "  ([n] (if (ftr-zero? n) :z (recur (ftr-dec n)))) "
        "  ([n acc] (if (ftr-zero? n) acc (recur (ftr-dec n) (ftr-inc acc)))))"
    )
    assert f(5) == Keyword.intern(None, "z")
    assert f(5, 0) == 5
