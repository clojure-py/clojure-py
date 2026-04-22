"""extend-via-metadata dispatch path (opt-in per protocol)."""

import pytest
from clojure._core import Greeter, greet, IFn, invoke1, IllegalArgumentException


def test_greeter_protocol_opts_in_to_metadata():
    assert Greeter.via_metadata is True


def test_meta_dispatch_hit():
    class Mock:
        pass

    m = Mock()
    m.__clj_meta__ = {"greet": lambda self: "hi"}
    assert greet(m) == "hi"


def test_meta_dispatch_miss_raises():
    class NoMeta:
        pass

    with pytest.raises(IllegalArgumentException):
        greet(NoMeta())


def test_meta_key_missing_raises():
    class OtherMeta:
        pass

    o = OtherMeta()
    o.__clj_meta__ = {"other_method": lambda self: "x"}  # wrong key
    with pytest.raises(IllegalArgumentException):
        greet(o)


def test_meta_disabled_when_not_opted_in():
    """IFn has via_metadata=False, so __clj_meta__ is ignored."""
    class X:
        pass

    x = X()
    x.__clj_meta__ = {"invoke1": lambda s, a: "nope"}
    with pytest.raises(IllegalArgumentException):
        invoke1(x, 1)


def test_direct_extension_still_works_alongside_metadata():
    """extend_type-registered impls take precedence; metadata only fires on cache miss."""
    class E:
        pass

    Greeter.extend_type(E, {"greet": lambda self: "extended"})
    e = E()
    e.__clj_meta__ = {"greet": lambda self: "meta"}  # should NOT be consulted
    assert greet(e) == "extended"
