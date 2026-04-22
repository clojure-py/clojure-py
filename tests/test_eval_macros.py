"""Phase E4 — hardcoded macros."""

import pytest
from clojure._core import eval_string, keyword, EvalError


def _ev(src): return eval_string(src)


# --- defn ---

def test_defn_basic():
    _ev("(defn inc1 [x] (+ x 1))")
    assert _ev("(inc1 41)") == 42


def test_defn_recursive():
    _ev("(defn fact [n] (if (= n 0) 1 (* n (fact (- n 1)))))")
    assert _ev("(fact 5)") == 120


def test_defn_multi_arg():
    _ev("(defn add3 [a b c] (+ a b c))")
    assert _ev("(add3 1 2 3)") == 6


def test_defn_with_body_sequence():
    _ev("(defn ignore-x [x] 1 2 3)")
    assert _ev("(ignore-x 99)") == 3  # last body form wins (do-semantics)


# --- when ---

def test_when_truthy():
    assert _ev("(when true 42)") == 42


def test_when_falsy():
    assert _ev("(when false 42)") is None


def test_when_nil_is_falsy():
    assert _ev("(when nil 42)") is None


def test_when_body_sequence():
    assert _ev("(when true 1 2 3)") == 3


# --- when-not ---

def test_when_not_truthy():
    assert _ev("(when-not false 42)") == 42


def test_when_not_nil():
    assert _ev("(when-not nil :yes)") == keyword("yes")


def test_when_not_with_truthy_returns_nil():
    assert _ev("(when-not true :no)") is None


# --- cond ---

def test_cond_first_match():
    assert _ev("(cond true 1 true 2)") == 1


def test_cond_second_match():
    assert _ev("(cond false 1 true 2)") == 2


def test_cond_no_match_returns_nil():
    assert _ev("(cond false 1 false 2)") is None


def test_cond_with_else():
    assert _ev("(cond false :a :else :b)") == keyword("b")


def test_cond_empty():
    assert _ev("(cond)") is None


# --- or ---

def test_or_empty():
    assert _ev("(or)") is None


def test_or_single():
    assert _ev("(or 42)") == 42


def test_or_first_truthy():
    assert _ev("(or 1 2)") == 1


def test_or_skip_falsy():
    assert _ev("(or false nil 42)") == 42


def test_or_all_falsy():
    assert _ev("(or false nil)") is None


# --- and ---

def test_and_empty():
    assert _ev("(and)") is True


def test_and_single():
    assert _ev("(and 42)") == 42


def test_and_last_when_all_truthy():
    assert _ev("(and 1 2 3)") == 3


def test_and_short_circuit():
    assert _ev("(and false 1)") is False
    assert _ev("(and nil 42)") is None


# --- Nested macros ---

def test_defn_with_when():
    _ev("(defn maybe-double [x] (when (> x 0) (* x 2)))")
    assert _ev("(maybe-double 5)") == 10
    assert _ev("(maybe-double -1)") is None


def test_defn_with_cond():
    _ev("(defn sign [n] (cond (> n 0) :pos (< n 0) :neg :else :zero))")
    assert _ev("(sign 10)") == keyword("pos")
    assert _ev("(sign -5)") == keyword("neg")
    assert _ev("(sign 0)") == keyword("zero")
