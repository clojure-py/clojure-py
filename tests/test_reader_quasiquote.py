"""Reader: ` (syntax-quote), ~ (unquote), ~@ (unquote-splicing), name# gensym."""

import pytest
from clojure._core import eval_string, read_string, symbol


def _ev(src): return eval_string(src)


def test_syntax_quote_literal_symbol():
    # `x → current-ns-qualified symbol. Vanilla: `\`x` in ns clojure.user
    # produces `clojure.user/x`.
    result = _ev("`x")
    assert result == symbol("clojure.user", "x")


def test_syntax_quote_empty_list():
    result = _ev("`()")
    assert list(result) == []


def test_syntax_quote_list_of_symbols():
    # Unqualified symbols inside a syntax-quote resolve to the current ns
    # (clojure.user at the REPL).
    result = _ev("`(a b c)")
    assert list(result) == [
        symbol("clojure.user", "a"),
        symbol("clojure.user", "b"),
        symbol("clojure.user", "c"),
    ]


def test_unquote():
    result = _ev("(let [b 100] `(a ~b c))")
    assert list(result) == [symbol("clojure.user", "a"), 100, symbol("clojure.user", "c")]


def test_unquote_splicing():
    result = _ev("(let [b (list 10 20)] `(a ~@b c))")
    assert list(result) == [symbol("clojure.user", "a"), 10, 20, symbol("clojure.user", "c")]


def test_syntax_quote_vector():
    result = _ev("`[1 2 3]")
    assert list(result) == [1, 2, 3]


def test_syntax_quote_unquote_in_vector():
    result = _ev("(let [x 42] `[1 ~x 3])")
    assert list(result) == [1, 42, 3]


def test_auto_gensym_consistent():
    # Same name# resolves to the same gensym within a single syntax-quote.
    result = _ev("`[x# x#]")
    elems = list(result)
    assert elems[0] == elems[1]
    assert elems[0].name.endswith("__auto__")


def test_auto_gensym_fresh_each_form():
    # Each syntax-quote form gets its own counter-advance.
    r1 = _ev("`x#")
    r2 = _ev("`x#")
    assert r1 != r2


def test_nested_unquote_inside_qq_list():
    # `+` is in clojure.core and referred into clojure.user, so syntax-quote
    # qualifies it to clojure.core (its home ns, not the current ns).
    result = _ev("(let [x 1 y 2] `(+ ~x ~y))")
    assert list(result) == [symbol("clojure.core", "+"), 1, 2]


def test_syntax_quote_keyword_self_evaluates():
    assert _ev("`:foo") == _ev(":foo")


def test_syntax_quote_int_self_evaluates():
    assert _ev("`42") == 42
