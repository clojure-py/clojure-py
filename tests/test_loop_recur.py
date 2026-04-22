"""loop/recur via the bytecode compiler."""

import pytest
from clojure._core import eval_string


def test_loop_recur_countdown():
    assert eval_string("(loop [i 10] (if (= i 0) :done (recur (- i 1))))") == ":done" or \
           eval_string("(loop [i 10] (if (= i 0) :done (recur (- i 1))))").name == "done"


def test_loop_recur_sum():
    # 0+1+2+...+99 = 4950
    assert eval_string("(loop [i 0 s 0] (if (= i 100) s (recur (+ i 1) (+ s i))))") == 4950


def test_recur_in_fn_implicit():
    # factorial via `recur` inside plain `fn`, not loop.
    assert eval_string(
        "((fn fact [n acc] (if (= n 0) acc (recur (- n 1) (* acc n)))) 10 1)"
    ) == 3628800


def test_big_recur_bounded_stack():
    # If `recur` were compiling to a recursive call, 10000 iterations would
    # blow the Rust stack. It doesn't because `recur` is StoreLocals + Jump.
    assert eval_string(
        "(loop [i 0 s 0] (if (= i 10000) s (recur (+ i 1) (+ s i))))"
    ) == 49995000


def test_recur_outside_loop_is_compile_error():
    with pytest.raises(Exception):
        eval_string("(recur 1 2)")


def test_recur_non_tail_is_compile_error():
    # `(recur ...)` in the test of an `if` is non-tail.
    with pytest.raises(Exception):
        eval_string("(loop [i 0] (if (recur 1) :a :b))")


def test_recur_arity_mismatch():
    with pytest.raises(Exception):
        eval_string("(loop [i 0 j 0] (recur 1))")  # loop binds 2, recur passes 1
