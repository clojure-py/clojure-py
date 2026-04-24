import pytest
from clojure._core import (
    IFn,
    invoke1,
    invoke_variadic,
    Protocol,
    ProtocolFn,
    symbol,
    IllegalArgumentException,
)


def test_ifn_protocol_object_exists():
    assert isinstance(IFn, Protocol)
    assert IFn.name == symbol("clojure.core", "IFn")
    assert IFn.via_metadata is False


def test_protocol_methods_are_objects():
    # Phase 3: method-level bindings switched from ProtocolMethod to
    # ProtocolFn. The Protocol object still holds the method_keys list.
    assert isinstance(invoke1, ProtocolFn)
    assert repr(invoke1) == "#<ProtocolFn IFn/invoke1>"


def test_invoke_variadic_registered():
    assert isinstance(invoke_variadic, ProtocolFn)
    assert repr(invoke_variadic) == "#<ProtocolFn IFn/invoke_variadic>"


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
