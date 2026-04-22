"""Python interop via `.`, `.-`, `set!` compiler special forms."""

from clojure._core import eval_string


def test_method_call_no_args():
    assert eval_string('(.upper "hello")') == "HELLO"


def test_method_call_one_arg():
    assert eval_string('(.startswith "hello" "he")') is True
    assert eval_string('(.startswith "hello" "xx")') is False


def test_method_call_multi_args():
    # str.replace(old, new)
    assert eval_string('(.replace "aaabbb" "a" "x")') == "xxxbbb"


def test_legacy_dot_form():
    assert eval_string('(. "world" upper)') == "WORLD"


def test_legacy_dot_parenthesized():
    assert eval_string('(. "hello" (startswith "h"))') is True


def test_get_attr_sugar():
    # complex.real
    import builtins
    eval_string("(def c nil)")
    from clojure._core import find_ns, symbol
    user_ns = find_ns(symbol("clojure.user"))
    user_ns.c.bind_root(complex(3, 4))
    assert eval_string('(.-real c)') == 3.0
    assert eval_string('(.-imag c)') == 4.0


def test_legacy_dot_attr():
    from clojure._core import find_ns, symbol
    user_ns = find_ns(symbol("clojure.user"))
    user_ns.c.bind_root(complex(7, 0))
    assert eval_string('(. c -real)') == 7.0


def test_set_attr_sugar():
    import types
    ns = types.SimpleNamespace()
    from clojure._core import find_ns, symbol
    user_ns = find_ns(symbol("clojure.user"))
    eval_string("(def target nil)")
    user_ns.target.bind_root(ns)
    eval_string('(set! (.-foo target) 42)')
    assert ns.foo == 42
    # Round-trip via eval.
    assert eval_string('(.-foo target)') == 42


def test_python_class_as_callable():
    # Python classes are callable; `(Foo a b)` compiles as an invoke with
    # no dedicated New op.
    from fractions import Fraction
    from clojure._core import find_ns, symbol
    user_ns = find_ns(symbol("clojure.user"))
    eval_string("(def Frac nil)")
    user_ns.Frac.bind_root(Fraction)
    result = eval_string("(Frac 3 4)")
    assert result == Fraction(3, 4)
