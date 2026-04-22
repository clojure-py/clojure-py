import pytest
from clojure._core import (
    IFn,
    invoke1,
    invoke_variadic,
    Protocol,
    ProtocolMethod,
    symbol,
    IllegalArgumentException,
)


def test_ifn_protocol_object_exists():
    assert isinstance(IFn, Protocol)
    assert IFn.name == symbol("clojure.core", "IFn")
    assert IFn.via_metadata is False


def test_protocol_methods_are_objects():
    assert isinstance(invoke1, ProtocolMethod)
    assert invoke1.key == "invoke1"
    assert invoke1.protocol is IFn


def test_invoke_variadic_registered():
    assert isinstance(invoke_variadic, ProtocolMethod)
    assert invoke_variadic.key == "invoke_variadic"
    assert invoke_variadic.protocol is IFn


def test_dispatch_on_empty_raises():
    class Foo:
        pass

    with pytest.raises(IllegalArgumentException, match="No implementation"):
        invoke1(Foo(), 42)


def test_dispatch_error_mentions_class():
    class MyWeirdType:
        pass

    with pytest.raises(IllegalArgumentException, match="MyWeirdType"):
        invoke1(MyWeirdType(), "arg")
