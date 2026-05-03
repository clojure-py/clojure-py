"""Compiler tests — loop* + recur + freshen-on-capture.

The freshen-on-capture invariant: when an inner fn closes over a `loop`
binding, each iteration's closure must see THAT iteration's value rather
than sharing a mutating cell with subsequent iterations (the JS
late-binding bug). The compiler enforces this by NOT promoting loop
bindings to cells and instead creating a fresh cell at each MAKE_FUNCTION
site (via types.CellType)."""

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


_intern_fn("clp-add", lambda a, b: a + b)
_intern_fn("clp-mul", lambda a, b: a * b)
_intern_fn("clp-dec", lambda x: x - 1)
_intern_fn("clp-inc", lambda x: x + 1)
_intern_fn("clp-zero?", lambda x: x == 0)
_intern_fn("clp-lt", lambda a, b: a < b)


# --- loop without recur (degenerate case = let*) -----------------------

def test_loop_no_recur_acts_like_let():
    assert _eval("(loop* [x 5] x)") == 5

def test_loop_empty_body_is_nil():
    assert _eval("(loop* [x 1])") is None

def test_loop_body_is_implicit_do():
    assert _eval("(loop* [x 5] x x x)") == 5


# --- loop + recur, single binding --------------------------------------

def test_countdown_to_zero():
    assert _eval(
        "(loop* [n 5] (if (clp-zero? n) :done (recur (clp-dec n))))"
    ) == Keyword.intern(None, "done")

def test_recur_with_inner_value_uses_old_binding():
    """(recur (inc i)) — the `i` in `inc i` is the OLD i, not the
    rebound one. Easy to get wrong if values are stored before they're
    all evaluated."""
    assert _eval(
        "(loop* [i 0] (if (clp-lt 9 i) i (recur (clp-inc i))))"
    ) == 10


# --- loop + recur, multiple bindings -----------------------------------

def test_factorial():
    assert _eval(
        "(loop* [n 5 acc 1] "
        "  (if (clp-zero? n) acc (recur (clp-dec n) (clp-mul acc n))))"
    ) == 120

def test_factorial_large():
    assert _eval(
        "(loop* [n 10 acc 1] "
        "  (if (clp-zero? n) acc (recur (clp-dec n) (clp-mul acc n))))"
    ) == 3628800

def test_sum_1_to_10():
    assert _eval(
        "(loop* [i 1 acc 0] "
        "  (if (clp-lt 10 i) acc (recur (clp-inc i) (clp-add acc i))))"
    ) == 55


# --- nested loops ------------------------------------------------------

def test_nested_loops_inner_recur_targets_inner_only():
    """Each loop establishes its own recur target — the inner recur
    must rebind the inner bindings, not the outer ones."""
    assert _eval(
        "(loop* [i 1 outer 0] "
        "  (if (clp-lt 3 i) outer "
        "    (recur (clp-inc i) "
        "      (clp-add outer "
        "        (loop* [j 1 inner 0] "
        "          (if (clp-lt 3 j) inner "
        "            (recur (clp-inc j) (clp-add inner j))))))))"
    ) == 18  # outer accumulates (1+2+3) three times

def test_loop_inside_let():
    assert _eval(
        "(let* [base 100] "
        "  (loop* [n 5 acc base] "
        "    (if (clp-zero? n) acc (recur (clp-dec n) (clp-add acc n)))))"
    ) == 115


# --- recur outside a loop ----------------------------------------------

def test_recur_outside_loop_raises():
    with pytest.raises(SyntaxError):
        _eval("(recur 1)")

def test_recur_in_fn_targets_fn_args():
    """fn-tail recur uses the fn's own args as recur targets — no
    enclosing loop required."""
    f = _eval(
        "(fn* [n] (if (clp-zero? n) :done (recur (clp-dec n))))"
    )
    from clojure.lang import Keyword
    assert f(5) == Keyword.intern(None, "done")
    assert f(0) == Keyword.intern(None, "done")


# --- recur arity check -------------------------------------------------

def test_recur_arity_mismatch_raises():
    with pytest.raises(SyntaxError):
        _eval("(loop* [a 1 b 2] (recur 99))")
    with pytest.raises(SyntaxError):
        _eval("(loop* [a 1] (recur 1 2))")


# --- freshen-on-capture: the centerpiece -------------------------------

def test_each_iteration_closure_captures_own_value():
    """Three iterations each create an fn closing over the loop binding
    `i`. Each closure must see THAT iteration's value, not all-3 or all-0."""
    collected = []
    Var.intern(Compiler.current_ns(), Symbol.intern("clp-collected"), collected)
    _intern_fn("clp-collect!", lambda f: (collected.append(f), f)[1])
    _eval(
        "(loop* [i 0] "
        "  (if (clp-lt 2 i) nil "
        "    (do (clp-collect! (fn* [] i)) (recur (clp-inc i)))))"
    )
    assert [f() for f in collected] == [0, 1, 2]
    collected.clear()

def test_captured_loop_var_persists_after_loop():
    """A fn returned from inside a loop body keeps its closure value even
    after the loop has exited."""
    f = _eval(
        "(loop* [i 7 result nil] "
        "  (if (clp-zero? i) result (recur (clp-dec i) (fn* [] i))))"
    )
    # Final iteration: i was 1 when we created the fn, then recur set i=0 and
    # returned result. The captured cell should hold 1 (its iteration value),
    # NOT 0 (the rebound parent local).
    assert f() == 1

def test_captured_loop_var_through_intermediate_fn():
    """Three-level capture across a loop: inner fn closes over loop binding
    via an intermediate fn that doesn't reference it. Each layer uses the
    fresh cell created at its OWN creation site inside the loop body."""
    collected = []
    Var.intern(Compiler.current_ns(), Symbol.intern("clp-c2"), collected)
    _intern_fn("clp-collect2!", lambda f: (collected.append(f), f)[1])
    _eval(
        "(loop* [i 0] "
        "  (if (clp-lt 2 i) nil "
        "    (do (clp-collect2! (fn* [] (fn* [] i))) (recur (clp-inc i)))))"
    )
    assert [f()() for f in collected] == [0, 1, 2]
    collected.clear()


# --- Python introspection still works ----------------------------------

def test_loop_compile_doesnt_pollute_dis_with_recur_temps():
    """`recur` is a JUMP_BACKWARD — it shouldn't introduce __t_N__
    gensyms in co_varnames the way an ANF-style lowering would."""
    from clojure.lang import _compile_to_thunk
    fn = _compile_to_thunk(read_string(
        "(loop* [n 3 acc 1] "
        "  (if (clp-zero? n) acc (recur (clp-dec n) (clp-mul acc n))))"
    ))
    # Only the loop-binding gensyms should appear; no anonymous temps.
    for vn in fn.__code__.co_varnames:
        assert vn.startswith("__n_") or vn.startswith("__acc_")
