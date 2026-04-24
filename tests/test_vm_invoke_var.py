"""Op::InvokeVar fuses Deref + Invoke. Verify semantic equivalence."""

import clojure  # registers Python importer
from clojure._core import eval_string


def test_invoke_var_equivalent_to_deref_plus_invoke():
    # Simple numeric: calls `+` via its Var.
    f = eval_string("(fn [a b] (+ a b))")
    assert f(3, 4) == 7


def test_invoke_var_loop_recur_sum():
    f = eval_string("""
    (fn [n]
      (loop [i 0 acc 0]
        (if (< i n) (recur (inc i) (+ acc i)) acc)))
    """)
    assert f(100) == sum(range(100))  # 4950
    assert f(1000) == sum(range(1000))  # 499500


def test_invoke_var_with_keyword_and_string():
    # Calls through core fns that vary in arity + return type.
    f = eval_string("""
    (fn [s]
      (str "hello, " s "!"))
    """)
    assert f("world") == "hello, world!"


def test_invoke_var_variadic_still_works():
    # + with many args: should still dispatch correctly even though
    # InvokeVar packs nargs into a u8 — up to 255 args, and variadic
    # fall-through handles the rest.
    f = eval_string("(fn [] (+ 1 2 3 4 5 6 7 8 9 10))")
    assert f() == 55
