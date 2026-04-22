"""IFn built-in fallback for arbitrary Python callables."""

from clojure._core import invoke1, invoke2, invoke_variadic, IFn
import functools


def test_lambda_as_ifn():
    f = lambda x: x + 1
    assert invoke1(f, 10) == 11


def test_def_function_as_ifn():
    def add(a, b):
        return a + b
    assert invoke2(add, 3, 4) == 7


def test_builtin_as_ifn():
    assert invoke2(max, 3, 9) == 9


def test_partial_as_ifn():
    inc = functools.partial(lambda a, b: a + b, 1)
    assert invoke1(inc, 41) == 42


def test_bound_method_as_ifn():
    class C:
        def greet(self, name):
            return f"hi {name}"
    c = C()
    assert invoke1(c.greet, "world") == "hi world"


def test_type_as_ifn():
    assert invoke1(int, "42") == 42


def test_variadic_any_callable():
    assert invoke_variadic(lambda *a: sum(a), 1, 2, 3, 4) == 10


def test_repeat_call_uses_cache_not_fallback(monkeypatch):
    """After the first call on a lambda's type, the cache should contain an entry —
    a second call should NOT trigger the fallback (because the type was cached)."""
    # We observe this indirectly: temporarily swap in a fallback that records calls
    # AFTER priming the cache with a first call.
    f = lambda x: x * 2
    # First call warms the cache via the built-in fallback.
    assert invoke1(f, 5) == 10

    # Now swap in a counting fallback and check it isn't consulted for the same type.
    calls = []
    original_fb = IFn.fallback

    def counting_fb(p, m, t):
        calls.append(t)
        if original_fb is not None:
            original_fb(p, m, t)

    IFn.set_fallback(counting_fb)
    try:
        assert invoke1(f, 6) == 12
        assert invoke1(f, 7) == 14
        assert len(calls) == 0, f"fallback was unexpectedly consulted: {calls}"
    finally:
        IFn.set_fallback(original_fb)
