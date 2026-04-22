"""Dispatch — MRO walk + epoch-based promotion invalidation."""

from clojure._core import IFn, invoke1


class Parent: pass
class Child(Parent): pass


def test_mro_walk_finds_parent_impl():
    IFn.extend_type(Parent, {"invoke1": lambda self, x: ("parent", x)})
    assert invoke1(Child(), 42) == ("parent", 42)


def test_mro_hit_promoted_to_exact_and_invalidated_on_reextend():
    # Fresh pair so we don't inherit state from the prior test.
    class P: pass
    class C(P): pass

    IFn.extend_type(P, {"invoke1": lambda self, x: ("v1", x)})
    # First call: MRO walk finds P, promotes to C's cache entry.
    assert invoke1(C(), 1) == ("v1", 1)

    # Re-extend P with a new impl; epoch bumps; C's promoted entry becomes stale.
    IFn.extend_type(P, {"invoke1": lambda self, x: ("v2", x)})

    # Next dispatch on C sees stale epoch at exact_key → falls through to MRO
    # walk → finds new P table → re-promotes (now with new epoch).
    assert invoke1(C(), 2) == ("v2", 2)

    # A third call confirms the newly promoted entry is now current (no re-walk needed).
    assert invoke1(C(), 3) == ("v2", 3)


def test_exact_type_impl_overrides_mro():
    class Q: pass
    class R(Q): pass

    IFn.extend_type(Q, {"invoke1": lambda self, x: ("Q-impl", x)})
    IFn.extend_type(R, {"invoke1": lambda self, x: ("R-impl", x)})
    assert invoke1(R(), "hi") == ("R-impl", "hi")
    assert invoke1(Q(), "hi") == ("Q-impl", "hi")
