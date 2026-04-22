"""Phase E3 — def + clojure.core shims."""

import pytest
from clojure._core import eval_string, symbol, keyword, create_ns, find_ns, Var, EvalError


def _ev(src): return eval_string(src)


# --- def ---

def test_def_creates_var():
    v = _ev("(def my-x 42)")
    assert isinstance(v, Var)


def test_def_bind_then_lookup():
    _ev("(def answer 42)")
    assert _ev("answer") == 42


def test_def_rebind():
    _ev("(def a 1)")
    _ev("(def a 2)")
    assert _ev("a") == 2


def test_def_without_init():
    _ev("(def unbound-var)")
    # unbound — deref'ing raises (we'd need a test that calls resolve)
    # For now just confirm the Var was created.
    ns = find_ns(symbol("clojure.user"))
    assert hasattr(ns, "unbound-var")


def test_def_qualified_symbol_raises():
    with pytest.raises(EvalError, match="qualified"):
        _ev("(def my.ns/x 1)")


# --- var special form ---

def test_var_returns_var_not_deref():
    _ev("(def q 99)")
    v = _ev("(var q)")
    assert isinstance(v, Var)
    assert v.deref() == 99


# --- Arithmetic ---

def test_plus():
    assert _ev("(+ 1 2)") == 3
    assert _ev("(+ 1 2 3 4)") == 10
    assert _ev("(+)") == 0


def test_minus():
    assert _ev("(- 10 3)") == 7
    assert _ev("(- 5)") == -5


def test_times():
    assert _ev("(* 3 4)") == 12
    assert _ev("(*)") == 1


def test_div():
    assert _ev("(/ 10 2)") == 5.0


def test_inc_dec():
    assert _ev("(inc 41)") == 42
    assert _ev("(dec 43)") == 42


def test_mixed_float():
    assert _ev("(+ 1 2.5)") == 3.5


# --- Comparison ---

def test_equality():
    assert _ev("(= 1 1)") is True
    assert _ev("(= 1 2)") is False
    assert _ev("(= :a :a)") is True


def test_less_than():
    assert _ev("(< 1 2 3)") is True
    assert _ev("(< 1 3 2)") is False


def test_greater_than():
    assert _ev("(> 3 2 1)") is True
    assert _ev("(> 1 2)") is False


# --- Logical ---

def test_not():
    assert _ev("(not true)") is False
    assert _ev("(not false)") is True
    assert _ev("(not nil)") is True
    assert _ev("(not 0)") is False  # 0 is truthy in Clojure


def test_nil_q():
    assert _ev("(nil? nil)") is True
    assert _ev("(nil? 42)") is False


# --- Collection fns ---

def test_count():
    assert _ev("(count [1 2 3])") == 3
    assert _ev("(count [])") == 0


def test_list_fn():
    l = _ev("(list 1 2 3)")
    assert list(l) == [1, 2, 3]


def test_vector_fn():
    v = _ev("(vector 1 2 3)")
    assert list(v) == [1, 2, 3]


def test_str():
    assert _ev("(str \"hello\" \" \" \"world\")") == "hello world"
    assert _ev("(str 1 2 3)") == "123"
    assert _ev("(str nil)") == ""


# --- Recursive fn via Var ---

def test_factorial():
    _ev("(def fact (fn [n] (if (= n 0) 1 (* n (fact (- n 1))))))")
    assert _ev("(fact 0)") == 1
    assert _ev("(fact 1)") == 1
    assert _ev("(fact 5)") == 120
    assert _ev("(fact 10)") == 3628800


def test_fibonacci():
    _ev("""
    (def fib (fn [n]
      (if (< n 2) n
          (+ (fib (- n 1)) (fib (- n 2))))))
    """)
    assert _ev("(fib 0)") == 0
    assert _ev("(fib 1)") == 1
    assert _ev("(fib 10)") == 55


# --- def + fn composition ---

def test_def_fn_simple():
    _ev("(def add (fn [a b] (+ a b)))")
    assert _ev("(add 3 4)") == 7


def test_higher_order():
    _ev("(def apply-twice (fn [f x] (f (f x))))")
    _ev("(def add-one (fn [x] (+ x 1)))")
    assert _ev("(apply-twice add-one 10)") == 12


# --- Seq ops ---

def test_first_rest():
    assert _ev("(first [1 2 3])") == 1
    r = _ev("(rest [1 2 3])")
    # rest on a vector-seq; collect via Python list()
    assert list(r) == [2, 3]
