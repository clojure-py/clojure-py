"""ILookup protocol + dispatch-through-rt behavior."""

import pytest
from clojure._core import ILookup, val_at, IllegalArgumentException


def test_ilookup_protocol_exists():
    from clojure._core import Protocol
    assert isinstance(ILookup, Protocol)
    assert ILookup.via_metadata is False


def test_val_at_on_dict():
    assert val_at({"a": 1}, "a", None) == 1


def test_val_at_on_dict_miss_returns_default():
    assert val_at({"a": 1}, "b", "default") == "default"


def test_val_at_on_list_index():
    assert val_at([10, 20, 30], 1, "nope") == 20


def test_val_at_on_list_out_of_bounds():
    assert val_at([10, 20, 30], 99, "nope") == "nope"


def test_val_at_unsupported_type_raises():
    class NoGetItem:
        pass
    with pytest.raises(IllegalArgumentException):
        val_at(NoGetItem(), "k", None)


def test_rt_get_keyword_still_works():
    """Keyword.invoke1 / invoke2 route through rt::get, which now goes through
    ILookup dispatch. Verify Keyword-as-key into dict still works."""
    from clojure._core import keyword, invoke1, invoke2
    d = {keyword("a"): 1}
    assert invoke1(keyword("a"), d) == 1
    assert invoke2(keyword("b"), d, "default") == "default"
