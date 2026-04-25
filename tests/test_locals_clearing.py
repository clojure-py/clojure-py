"""Locals-clearing liveness pass.

The key property: after a local's last use in the enclosing scope, the
`frame.locals[slot]` no longer retains a reference to the value. Tests
use `weakref` to observe GC-reachability after eval returns, and embed
observability via callbacks inside compiled code.
"""

import gc
import weakref
from clojure._core import eval_string


class Tracked:
    """A plain object we can weakref. GC'd when all refs drop."""
    __slots__ = ("__weakref__", "label")
    def __init__(self, label):
        self.label = label
    def __repr__(self):
        return f"Tracked({self.label})"


def test_local_cleared_after_last_use():
    # Register a Tracked object as a Var so compiled code can reach it.
    # Then: (let [x <tracked>] (identity x)) — after let exits and return
    # value (None from `identity` wrapper) falls out, x's slot should be
    # cleared inside the let body so the only remaining ref is the Var's
    # root, which we then remove.
    t = Tracked("a")
    ref = weakref.ref(t)
    # Bind it as a Var so reader can resolve a symbol to it.
    eval_string("(def target_value nil)")
    from clojure._core import find_ns, symbol
    user_ns = find_ns(symbol("clojure.user"))
    user_ns.target_value.bind_root(t)
    del t

    # Build a let that binds x and does nothing with it in a way that GC
    # would free it, IF the frame slot wasn't clearing.
    eval_string("(let [x target_value] (str x))")
    user_ns.target_value.bind_root(None)
    gc.collect()
    assert ref() is None, "Tracked object should be GC'd after Var root is cleared"


def test_captured_local_not_cleared_mid_body():
    # If an inner fn* captures a local, the outer scope must not mid-body
    # clear it before the fn is constructed. We verify by executing the
    # captured fn later and checking the captured value.
    f = eval_string(
        "(let [x 42] "
        "  (let [g (fn [] x)] "
        "    (vector (g) (g))))"
    )
    # Expect [42 42]
    assert list(f) == [42, 42]


def test_if_branches_both_access_local():
    # `x` used on both branches. After the let ends the scope-end clear
    # catches whichever branch didn't emit its own clear.
    assert eval_string("(let [x 7] (if true x x))") == 7
    assert eval_string("(let [x 7] (if false x x))") == 7


def test_loop_slot_survives_back_edge():
    # Loop slot `i` is read/written on every iteration; liveness must not
    # prematurely clear it (otherwise back-edge reads None).
    assert eval_string(
        "(loop [i 0 acc 0] "
        "  (if (= i 5) acc "
        "    (recur (+ i 1) (+ acc i))))"
    ) == 0 + 1 + 2 + 3 + 4


# ---------------------------------------------------------------------------
# Closure capture / clearing — adapted from
# clojure/test/clojure/test_clojure/clearing.clj.
#
# Vanilla uses `getDeclaredFields` + `setAccessible` to peek at closed-over
# fields of a JVM Fn class and assert they are nil after the `:once` fn is
# invoked. We have no JVM reflection — but the underlying property (a
# captured value should be GC-able after the closure that holds it is
# released) is observable via `weakref`. We don't have a `:once` meta-tag
# specialization, so we test the general case: drop the closure itself and
# verify the captured value gets GC'd.
# ---------------------------------------------------------------------------

def _bind(name, value):
    eval_string(f"(def {name} nil)")
    from clojure._core import find_ns, symbol
    ns = find_ns(symbol("clojure.user"))
    getattr(ns, name).bind_root(value)


def _unbind(name):
    from clojure._core import find_ns, symbol
    ns = find_ns(symbol("clojure.user"))
    getattr(ns, name).bind_root(None)


def test_dropped_closure_releases_captured_value():
    """If a closure captures `x` and we then drop the only reference to the
    closure, the captured value must be GC-able."""
    t = Tracked("captured")
    ref = weakref.ref(t)
    _bind("captured_target", t)
    del t

    # Build a closure that captures the local then drop our handle.
    f = eval_string("(let [x captured_target] (fn [] x))")
    assert f() is not None  # the captured value is reachable through f
    _unbind("captured_target")
    del f
    gc.collect()
    assert ref() is None


def test_nested_closure_releases_captured_value():
    """Nested closures: outer captures inner captures `x`. Drop both."""
    t = Tracked("nested")
    ref = weakref.ref(t)
    _bind("nested_target", t)
    del t

    outer = eval_string(
        "(let [x nested_target] "
        "  (let [inner (fn [] x)] "
        "    (fn [] (inner))))"
    )
    assert outer() is not None
    _unbind("nested_target")
    del outer
    gc.collect()
    assert ref() is None


def test_conditional_closure_capture():
    """A conditional inside the closure body that references `x` — we must
    not over-clear `x` before the closure runs."""
    t = Tracked("cond")
    ref = weakref.ref(t)
    _bind("cond_target", t)
    del t

    f = eval_string("(let [x cond_target] (fn [] (if true x nil)))")
    assert f() is not None
    _unbind("cond_target")
    del f
    gc.collect()
    assert ref() is None


def test_loop_inside_closure_does_not_break_capture():
    """A `(loop [] x)` inside the closure body — capture must still work."""
    t = Tracked("loop")
    ref = weakref.ref(t)
    _bind("loop_target", t)
    del t

    f = eval_string("(let [x loop_target] (fn [] (loop [] x)))")
    assert f() is not None
    _unbind("loop_target")
    del f
    gc.collect()
    assert ref() is None


def test_long_seq_capture_clears_eagerly():
    """CLJ-2145 repro adapted: a closure that consumes a seq once should not
    pin the seq's head — confirming consumption frees memory.
    Vanilla used 1e9 elements to exhaust heap; we use a smaller sentinel."""
    head = Tracked("head")
    ref = weakref.ref(head)
    _bind("seq_head", head)
    del head

    # Build a closure that walks a seq containing the tracked object.
    eval_string(
        "(let [x [seq_head]] "
        "  ((fn [] (doseq [_ x] _))))"
    )
    _unbind("seq_head")
    gc.collect()
    assert ref() is None
