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


def test_push_thread_bindings_rejects_non_dynamic_var():
    """JVM `Var.pushThreadBindings` raises IllegalStateException when any
    supplied var lacks `:dynamic true`. Mirrors Var.java:326-327."""
    v = _dynv("b1", "x", 1, dynamic=False)
    with pytest.raises(IllegalStateException, match="dynamically bind non-dynamic"):
        push_thread_bindings({v: 99})


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


def test_dynamic_var_call_uses_current_binding():
    """Calling a dynamic var via IFn (v() / v(a) / v(a, b) / variadic) must
    consult the binding stack, matching `@v` / `(deref v)`. JVM Clojure's
    Var.invoke delegates through the thread-binding frame, not the raw root."""
    from clojure._core import invoke1, invoke2, invoke_variadic

    # 0-arity __call__
    v0 = _dynv("b9.0", "f", lambda: "root")
    push_thread_bindings({v0: lambda: "bound"})
    try:
        assert v0() == "bound"
    finally:
        pop_thread_bindings()

    # 1-arity via __call__ and invoke1
    v1 = _dynv("b9.1", "inc", lambda x: x + 1)
    push_thread_bindings({v1: lambda x: x - 1})
    try:
        assert v1(10) == 9
        assert invoke1(v1, 10) == 9
    finally:
        pop_thread_bindings()

    # 2-arity via __call__ and invoke2
    v2 = _dynv("b9.2", "add", lambda a, b: a + b)
    push_thread_bindings({v2: lambda a, b: a * b})
    try:
        assert v2(3, 4) == 12
        assert invoke2(v2, 3, 4) == 12
    finally:
        pop_thread_bindings()

    # variadic
    vv = _dynv("b9.3", "sum", lambda *a: sum(a))
    push_thread_bindings({vv: lambda *a: sum(a) * 10})
    try:
        assert invoke_variadic(vv, 1, 2, 3) == 60
    finally:
        pop_thread_bindings()
