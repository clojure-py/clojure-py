from clojure._core import keyword, invoke1, invoke2, invoke_variadic, ArityException
import pytest


def test_keyword_invoke1_dict():
    d = {keyword("a"): 1}
    assert invoke1(keyword("a"), d) == 1


def test_keyword_invoke2_default():
    d = {keyword("a"): 1}
    assert invoke2(keyword("b"), d, "nope") == "nope"


def test_keyword_invoke2_hit_default_not_returned():
    d = {keyword("a"): 1}
    assert invoke2(keyword("a"), d, "nope") == 1


def test_keyword_invoke_variadic_1_and_2_arg():
    d = {keyword("a"): 1}
    assert invoke_variadic(keyword("a"), d) == 1
    assert invoke_variadic(keyword("b"), d, "default") == "default"


def test_keyword_wrong_arity_raises():
    d = {keyword("a"): 1}
    with pytest.raises(ArityException, match="Keyword"):
        invoke_variadic(keyword("a"), d, "extra", "extra2")


def test_keyword_direct_call_still_works():
    """The #[pymethods] __call__ on Keyword is independent of IFn dispatch — both paths should work."""
    d = {keyword("a"): 1}
    assert keyword("a")(d) == 1
    assert keyword("b")(d, "fallback") == "fallback"
