"""Phase E5 — evaluator integration tests. Multi-line Clojure programs."""

import pytest
from clojure._core import eval_string, keyword


def _ev(src): return eval_string(src)


def _ev_many(program):
    """Evaluate a sequence of top-level forms and return the last result."""
    import re
    # Simple split by top-level paren balancing — good enough for our tests.
    forms = []
    depth = 0
    current = ""
    in_string = False
    escape = False
    for c in program:
        if escape:
            current += c
            escape = False
            continue
        if c == "\\":
            escape = True
            current += c
            continue
        if c == '"':
            in_string = not in_string
        if not in_string:
            if c == "(" or c == "[" or c == "{":
                depth += 1
            elif c == ")" or c == "]" or c == "}":
                depth -= 1
        current += c
        if depth == 0 and current.strip() and not in_string:
            # Try to parse
            s = current.strip()
            if s and not s.startswith(";"):
                forms.append(s)
            current = ""
    result = None
    for f in forms:
        result = _ev(f)
    return result


# --- Factorial ---

def test_factorial_integration():
    program = """
    (defn fact [n] (if (= n 0) 1 (* n (fact (- n 1)))))
    (fact 10)
    """
    assert _ev_many(program) == 3628800


# --- Fibonacci ---

def test_fibonacci_integration():
    program = """
    (defn fib [n]
      (if (< n 2)
          n
          (+ (fib (- n 1)) (fib (- n 2)))))
    (fib 15)
    """
    assert _ev_many(program) == 610


# --- Higher-order fn ---

def test_compose():
    program = """
    (defn compose [f g] (fn [x] (f (g x))))
    (defn inc1 [x] (+ x 1))
    (defn double [x] (* x 2))
    ((compose inc1 double) 5)
    """
    # (inc (double 5)) = (inc 10) = 11
    assert _ev_many(program) == 11


def test_closure_counter():
    program = """
    (defn make-adder [n] (fn [x] (+ x n)))
    (def add5 (make-adder 5))
    (add5 37)
    """
    assert _ev_many(program) == 42


# --- Data manipulation ---

def test_build_list():
    program = """
    (def xs (list 1 2 3))
    (first xs)
    """
    assert _ev_many(program) == 1


def test_build_vector():
    program = """
    (def xs (vector 10 20 30))
    (count xs)
    """
    assert _ev_many(program) == 3


def test_keyword_lookup():
    program = """
    (def m {:name "alice" :age 30})
    (:name m)
    """
    assert _ev_many(program) == "alice"


# --- cond + when composition ---

def test_abs():
    program = """
    (defn abs [n] (cond (< n 0) (- n) :else n))
    [(abs 5) (abs -5) (abs 0)]
    """
    result = _ev_many(program)
    assert list(result) == [5, 5, 0]


def test_sum_list():
    """Recursive sum of a list."""
    program = """
    (defn sum [xs]
      (if (nil? (seq xs))
          0
          (+ (first xs) (sum (rest xs)))))
    (sum (list 1 2 3 4 5))
    """
    assert _ev_many(program) == 15


def test_count_via_recursion():
    program = """
    (defn my-count [xs]
      (if (nil? (seq xs))
          0
          (+ 1 (my-count (rest xs)))))
    (my-count [10 20 30 40])
    """
    assert _ev_many(program) == 4
