"""Multi-arity + variadic fn dispatch."""

import pytest
from clojure._core import eval_string


def test_multi_arity_dispatch():
    f = eval_string("(fn ([x] (* x 10)) ([x y] (+ x y)) ([x y z] (* x y z)))")
    assert f(5) == 50
    assert f(3, 4) == 7
    assert f(2, 3, 4) == 24


def test_multi_arity_wrong_arity_raises():
    f = eval_string("(fn ([x] x) ([x y] (+ x y)))")
    with pytest.raises(Exception):
        f(1, 2, 3)


def test_variadic_collects_rest():
    f = eval_string("(fn [a & rest] (vector a rest))")
    v = f(1, 2, 3, 4)
    # v is [1 (2 3 4)] — first is 1, second is a list.
    assert f(1) == eval_string("[1 nil]")


def test_variadic_empty_rest_is_nil():
    f = eval_string("(fn [x & rest] rest)")
    assert f(99) is None


def test_variadic_with_fixed_arities():
    f = eval_string(
        "(fn ([] :zero) ([x] :one) ([x y & rest] (count rest)))"
    )
    assert f().name == "zero"
    assert f(1).name == "one"
    # Variadic: 2 required + rest → count of rest
    assert f(1, 2, 3, 4, 5) == 3
