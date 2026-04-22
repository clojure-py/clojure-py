"""bound-fn* — convey current binding frame across threads."""

import threading
import sys
import types
from clojure._core import (
    Var,
    symbol,
    push_thread_bindings,
    pop_thread_bindings,
    bound_fn_star,
    BoundFn,
)


def _dynv(ns_name: str, sym_name: str, root):
    m = types.ModuleType(ns_name)
    sys.modules[ns_name] = m
    v = Var(m, symbol(sym_name))
    v.bind_root(root)
    v.set_dynamic(True)
    return v


def test_bound_fn_captures_current_frame():
    v = _dynv("bf.1", "x", 1)
    push_thread_bindings({v: 42})
    try:
        f = bound_fn_star(lambda: v.deref())
    finally:
        pop_thread_bindings()
    # Parent frame is gone; calling f still sees the captured binding.
    assert v.deref() == 1
    assert f() == 42


def test_bound_fn_isinstance_BoundFn():
    f = bound_fn_star(lambda: None)
    assert isinstance(f, BoundFn)


def test_bound_fn_conveys_across_thread():
    v = _dynv("bf.2", "x", 1)
    push_thread_bindings({v: 99})
    try:
        snap = bound_fn_star(lambda: v.deref())
    finally:
        pop_thread_bindings()

    result = []
    t = threading.Thread(target=lambda: result.append(snap()))
    t.start()
    t.join()
    assert result == [99]


def test_bound_fn_with_args():
    v = _dynv("bf.3", "offset", 100)
    push_thread_bindings({v: 7})
    try:
        snap = bound_fn_star(lambda a, b: v.deref() + a + b)
    finally:
        pop_thread_bindings()
    assert snap(10, 20) == 37


def test_bound_fn_nested_bindings_captured_correctly():
    """Only the top frame is captured — consistent with Clojure's bound-fn semantics."""
    v1 = _dynv("bf.4a", "x", 0)
    v2 = _dynv("bf.4b", "y", 0)
    push_thread_bindings({v1: 1})
    try:
        push_thread_bindings({v2: 2})  # top frame now has both v1 (inherited) and v2
        try:
            snap = bound_fn_star(lambda: (v1.deref(), v2.deref()))
        finally:
            pop_thread_bindings()
    finally:
        pop_thread_bindings()
    assert snap() == (1, 2)  # captured frame had v1=1 (inherited) and v2=2


def test_bound_fn_pops_frame_after_call():
    """After BoundFn returns, the stack length must match what it was before."""
    from clojure._core import bound_fn_star
    f = bound_fn_star(lambda: None)
    # We can't introspect the stack from Python easily, but we can verify indirectly:
    # call f twice and each time the top frame is our snapshot, not cumulative.
    v = _dynv("bf.5", "x", 99)
    # No binding active — v.deref() is root.
    assert v.deref() == 99

    push_thread_bindings({v: 123})
    try:
        g = bound_fn_star(lambda: v.deref())
    finally:
        pop_thread_bindings()

    assert v.deref() == 99
    assert g() == 123
    # After g() returns, outer-visible state is still root:
    assert v.deref() == 99
    assert g() == 123  # repeat — must still see snapshot, not accumulate


def test_bound_fn_error_path_still_pops():
    v = _dynv("bf.6", "x", 0)
    push_thread_bindings({v: 1})
    try:
        def raiser(): raise RuntimeError("boom")
        f = bound_fn_star(raiser)
    finally:
        pop_thread_bindings()

    # Before the erroring call, outer scope sees root.
    assert v.deref() == 0

    import pytest
    with pytest.raises(RuntimeError, match="boom"):
        f()

    # After the error, the stack is still clean — v sees root, not the snapshot.
    assert v.deref() == 0
