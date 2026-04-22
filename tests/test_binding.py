"""Dynamic binding stack — push/pop, dynamic deref, set!"""

import pytest
import sys
import types
from clojure._core import (
    Var,
    symbol,
    push_thread_bindings,
    pop_thread_bindings,
    IllegalStateException,
)


def _dynv(ns_name: str, sym_name: str, root, dynamic=True):
    m = types.ModuleType(ns_name)
    sys.modules[ns_name] = m
    v = Var(m, symbol(sym_name))
    v.bind_root(root)
    if dynamic:
        v.set_dynamic(True)
    return v


def test_non_dynamic_unaffected_by_binding():
    v = _dynv("b1", "x", 1, dynamic=False)
    push_thread_bindings({v: 99})
    try:
        assert v.deref() == 1
    finally:
        pop_thread_bindings()


def test_dynamic_binding_shadows_root():
    v = _dynv("b2", "x", 1)
    push_thread_bindings({v: 99})
    try:
        assert v.deref() == 99
    finally:
        pop_thread_bindings()
    # After pop, root is visible again.
    assert v.deref() == 1


def test_nested_bindings_inherit_outer():
    v = _dynv("b3", "x", 1)
    push_thread_bindings({v: 10})
    try:
        push_thread_bindings({})  # inner empty frame inherits v: 10
        try:
            assert v.deref() == 10
        finally:
            pop_thread_bindings()
    finally:
        pop_thread_bindings()


def test_inner_binding_shadows_outer():
    v = _dynv("b4", "x", 1)
    push_thread_bindings({v: 10})
    try:
        push_thread_bindings({v: 20})
        try:
            assert v.deref() == 20
        finally:
            pop_thread_bindings()
        # After inner pop: back to outer binding.
        assert v.deref() == 10
    finally:
        pop_thread_bindings()


def test_set_bang_in_binding():
    v = _dynv("b5", "x", 1)
    push_thread_bindings({v: 10})
    try:
        v.set_bang(20)
        assert v.deref() == 20
    finally:
        pop_thread_bindings()
    # Root unchanged by set!
    assert v.deref() == 1


def test_set_bang_no_frame_raises():
    v = _dynv("b6", "x", 1)
    with pytest.raises(IllegalStateException):
        v.set_bang(20)


def test_set_bang_var_not_in_frame_raises():
    v1 = _dynv("b7.a", "x", 1)
    v2 = _dynv("b7.b", "y", 2)
    push_thread_bindings({v1: 10})  # only v1 bound
    try:
        with pytest.raises(IllegalStateException):
            v2.set_bang(99)
    finally:
        pop_thread_bindings()


def test_multiple_vars_in_one_frame():
    v1 = _dynv("b8.a", "x", 1)
    v2 = _dynv("b8.b", "y", 2)
    push_thread_bindings({v1: 10, v2: 20})
    try:
        assert v1.deref() == 10
        assert v2.deref() == 20
    finally:
        pop_thread_bindings()


def test_dynamic_var_call_uses_root_only():
    """Known gap: calling a dynamic var via IFn (v() syntax) does NOT consult the
    binding stack — it uses the root directly. Documented in Task 29's spec.
    This test pins the current behavior so we notice when it changes (hopefully
    when a follow-on closes the gap)."""
    v = _dynv("b9", "f", lambda: "root-impl")
    push_thread_bindings({v: lambda: "bound-impl"})
    try:
        # deref() path: binding-aware → gets the bound lambda
        bound_fn = v.deref()
        assert bound_fn() == "bound-impl"
        # IFn call path: reads root only → gets the root lambda
        assert v() == "root-impl"
    finally:
        pop_thread_bindings()
