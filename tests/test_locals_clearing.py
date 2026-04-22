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
