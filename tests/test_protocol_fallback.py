"""Dispatch — fallback function path."""

import pytest
from clojure._core import IFn, invoke1, IllegalArgumentException


def test_fallback_registers_impl_on_miss():
    """A fallback that calls extend_type with an impl should succeed after retry."""
    def fb(protocol, method, target):
        protocol.extend_type(
            type(target),
            {method: lambda s, a: ("fallback", a)},
        )

    _original_fb = IFn.fallback
    IFn.set_fallback(fb)
    try:
        class X:
            pass

        assert invoke1(X(), 10) == ("fallback", 10)
        # Subsequent call: type is cached, fallback not consulted again.
        assert invoke1(X(), 11) == ("fallback", 11)
    finally:
        IFn.set_fallback(_original_fb)


def test_fallback_consulted_once_then_raises():
    """If fallback doesn't register anything, dispatch raises after one retry."""
    calls = []

    def fb(p, m, t):
        calls.append(1)
        # Deliberately do nothing — no impl gets registered.

    _original_fb = IFn.fallback
    IFn.set_fallback(fb)
    try:
        class Y:
            pass

        with pytest.raises(IllegalArgumentException):
            invoke1(Y(), 1)
        assert len(calls) == 1
    finally:
        IFn.set_fallback(_original_fb)


def test_fallback_can_register_different_type():
    """Fallback may register an impl for a completely unrelated type, but the original
    target still won't dispatch unless its own type gets an impl or an MRO-reachable one."""

    class OtherType:
        pass

    def fb(p, m, t):
        p.extend_type(OtherType, {m: lambda s, a: ("other", a)})

    _original_fb = IFn.fallback
    IFn.set_fallback(fb)
    try:
        class Z:
            pass

        with pytest.raises(IllegalArgumentException):
            invoke1(Z(), 1)
        # But OtherType now works (no fallback needed — directly extended):
        assert invoke1(OtherType(), 5) == ("other", 5)
    finally:
        IFn.set_fallback(_original_fb)


def test_fallback_slot_is_settable_and_clearable():
    # IFn ships with a built-in fallback (handles arbitrary Python callables);
    # the test checks the slot mechanics, not its initial value.
    original_fb = IFn.fallback
    fb = lambda p, m, t: None
    IFn.set_fallback(fb)
    try:
        assert IFn.fallback is fb
    finally:
        IFn.set_fallback(original_fb)
    assert IFn.fallback is original_fb
