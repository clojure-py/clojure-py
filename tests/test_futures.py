"""Futures, promises, pmap/pvalues/pcalls, with-open, extends?/extenders."""

import io
import sys
import threading
import time
import pytest
from clojure._core import eval_string, Future, Promise


def _ev(s):
    return eval_string(s)


def _inject(name, obj):
    _ev("(def %s nil)" % name)
    sys.modules["clojure.user"].__dict__[name].bind_root(obj)


# --- Future ---


def test_future_macro_returns_value():
    assert _ev("@(future (+ 1 2 3))") == 6


def test_future_call_with_zero_arg_fn():
    assert _ev("@(future-call (fn [] :answer))") == _ev(":answer")


def test_future_runs_on_separate_thread():
    main_tid = threading.get_ident()
    _ev("(def --tid (atom nil))")
    def grab():
        sys.modules["clojure.user"].__dict__["--tid"].deref()  # reach for the var
        sys.modules["clojure.user"].__dict__["--tid"]
    # Have the future record its own thread id.
    _inject("--get-tid", lambda: threading.get_ident())
    f_tid = _ev("@(future-call --get-tid)")
    assert f_tid != main_tid


def test_future_done_pred():
    f = _ev("(future 42)")
    _inject("--f", f)
    _ev("@--f")
    assert _ev("(future-done? --f)") is True


def test_future_pred():
    assert _ev("(future? (future 1))") is True
    assert _ev("(future? 1)") is False
    assert _ev("(future? (atom 1))") is False


def test_future_cancel_pending():
    # Future blocked on a never-delivered promise → cancelable.
    _ev("(def --p (promise))")
    _ev("(def --f (future @--p))")
    # Brief sleep so the worker has actually picked it up isn't required —
    # cancel works whether or not the worker has started, because our impl
    # marks the slot as Cancelled and any subsequent result is discarded.
    assert _ev("(future-cancel --f)") is True
    assert _ev("(future-cancelled? --f)") is True


def test_future_cancel_after_done_returns_false():
    f = _ev("(future 1)")
    _inject("--f2", f)
    _ev("@--f2")  # ensure done
    assert _ev("(future-cancel --f2)") is False


def test_deref_cancelled_future_raises():
    from clojure._core import IllegalStateException
    _ev("(def --p2 (promise))")
    _ev("(def --f3 (future @--p2))")
    _ev("(future-cancel --f3)")
    with pytest.raises(IllegalStateException):
        _ev("@--f3")


def test_future_propagates_exception():
    # An exception in the future body should re-raise on deref.
    with pytest.raises(Exception):
        _ev("@(future (/ 1 0))")


# --- Promise ---


def test_promise_deliver_deref():
    assert _ev("(let [p (promise)] (deliver p 42) @p)") == 42


def test_promise_deliver_idempotent():
    # Second deliver is a no-op and returns nil.
    result = _ev("(let [p (promise)] (deliver p 1) [(deliver p 2) @p])")
    assert list(result) == [None, 1]


def test_promise_deref_blocks_until_delivered():
    p = _ev("(promise)")
    _inject("--p3", p)

    def deliver_later():
        time.sleep(0.05)
        p.deliver(99)

    threading.Thread(target=deliver_later).start()
    # @--p3 blocks until the other thread calls deliver.
    assert _ev("@--p3") == 99


def test_realized_on_promise():
    assert _ev("(realized? (promise))") is False
    assert _ev("(let [p (promise)] (deliver p 1) (realized? p))") is True


def test_realized_on_future():
    f = _ev("(future 1)")
    _inject("--rf", f)
    _ev("@--rf")
    assert _ev("(realized? --rf)") is True


def test_realized_false_on_non_ipending():
    assert _ev("(realized? 42)") is False
    assert _ev('(realized? "hello")') is False


# --- pmap ---


def test_pmap_basic():
    assert list(_ev("(pmap inc (range 5))")) == [1, 2, 3, 4, 5]


def test_pmap_runs_in_parallel():
    def slow(x):
        time.sleep(0.05)
        return x * 10
    _inject("--slow", slow)
    start = time.time()
    result = list(_ev("(pmap --slow (range 10))"))
    elapsed = time.time() - start
    # 10 × 50ms sequential = 500ms; any real parallelism gets us comfortably
    # under that. Threshold is loose (0.45s) so a slow runner can't flake —
    # we're checking that pmap isn't *serial*, not benchmarking it.
    assert result == [0, 10, 20, 30, 40, 50, 60, 70, 80, 90]
    assert elapsed < 0.45, f"pmap took {elapsed*1000:.0f}ms — not parallel?"


def test_pmap_multi_collection():
    assert list(_ev("(pmap + [1 2 3] [10 20 30])")) == [11, 22, 33]


def test_pmap_lazy():
    # pmap should be lazy — it returns a lazy seq.
    s = _ev("(pmap inc (range 1000000))")
    # Just take a few — shouldn't realize the whole thing.
    assert list(_ev("(take 3 (pmap inc (range 1000000)))")) == [1, 2, 3]


# --- pvalues / pcalls ---


def test_pvalues():
    assert list(_ev("(pvalues 1 2 3 (+ 4 5))")) == [1, 2, 3, 9]


def test_pcalls():
    assert list(_ev("(pcalls (fn [] :a) (fn [] :b) (fn [] :c))")) == [
        _ev(":a"), _ev(":b"), _ev(":c")
    ]


# --- with-open ---


def test_with_open_calls_close_method():
    class C:
        def __init__(self):
            self.closed = False
        def close(self):
            self.closed = True
    c = C()
    _inject("--c-close", c)
    _ev("(with-open [x --c-close] :inside)")
    assert c.closed is True


def test_with_open_calls_exit_method():
    # StringIO supports __exit__.
    buf = io.StringIO()
    _inject("--buf", buf)
    _ev('(with-open [b --buf] (.write b "hi"))')
    assert buf.closed is True


def test_with_open_multiple_bindings():
    class C:
        def __init__(self, name):
            self.name = name
            self.closed_at = None
    closed_order = []
    a = C("a"); b = C("b"); c_ = C("c")
    a.close = lambda: closed_order.append("a")
    b.close = lambda: closed_order.append("b")
    c_.close = lambda: closed_order.append("c")
    _inject("--ma", a); _inject("--mb", b); _inject("--mc", c_)
    _ev("(with-open [aa --ma bb --mb cc --mc] :inside)")
    # Vanilla closes in REVERSE order (last opened, first closed).
    assert closed_order == ["c", "b", "a"]


def test_with_open_closes_on_exception():
    class C:
        def __init__(self):
            self.closed = False
        def close(self):
            self.closed = True
    c = C()
    _inject("--cc", c)
    with pytest.raises(Exception):
        _ev("(with-open [x --cc] (/ 1 0))")
    assert c.closed is True


# --- extends? / extenders ---


def test_extends_pred():
    # IDeref is implemented for Atom (a Rust pyclass).
    assert _ev("(extends? clojure._core/IDeref clojure._core/Atom)") is True
    # Plain int doesn't implement IDeref.
    assert _ev("(extends? clojure._core/IDeref 5)") is False


def test_extenders_returns_seq():
    types = list(_ev("(extenders clojure._core/IDeref)"))
    # Multiple types should be registered (Atom, Var, Ref, Agent, …).
    assert len(types) >= 4


# --- bean ---


def test_bean_basic_data_attrs():
    class Point:
        def __init__(self, x, y):
            self.x = x
            self.y = y
    _inject("--bp", Point(3, 4))
    assert _ev("(:x (bean --bp))") == 3
    assert _ev("(:y (bean --bp))") == 4


def test_bean_skips_callables():
    class WithMethod:
        def __init__(self, v):
            self.v = v
        def double(self):
            return self.v * 2
    _inject("--bwm", WithMethod(5))
    keys = {str(k) for k in _ev("(keys (bean --bwm))")}
    assert keys == {":v"}


def test_bean_skips_dunder_and_private():
    class Mixed:
        def __init__(self):
            self.public = 1
            self._private = 2
    _inject("--bmx", Mixed())
    keys = {str(k) for k in _ev("(keys (bean --bmx))")}
    assert keys == {":public"}
    assert "__class__" not in keys


def test_bean_includes_property_values():
    class Sized:
        def __init__(self, items):
            self._items = items
        @property
        def length(self):
            return len(self._items)
    _inject("--bsz", Sized([10, 20, 30]))
    assert _ev("(:length (bean --bsz))") == 3


def test_bean_get_with_default():
    class A:
        def __init__(self):
            self.foo = 42
    _inject("--ba", A())
    assert _ev("(get (bean --ba) :foo)") == 42
    assert _ev("(get (bean --ba) :missing :sentinel)") == _ev(":sentinel")


def test_bean_works_through_seq():
    class Q:
        def __init__(self):
            self.a = 1
            self.b = 2
            self.c = 3
    _inject("--bq", Q())
    pairs = list(_ev("(seq (bean --bq))"))
    # MapEntries iterable as 2-element seqs.
    decoded = {str(list(p)[0]): list(p)[1] for p in pairs}
    assert decoded == {":a": 1, ":b": 2, ":c": 3}


def test_bean_is_live_view():
    # Vanilla bean reflects the underlying object on every lookup. We
    # match that — a bean captured before mutation reflects the new state
    # afterwards.
    class Mutable:
        def __init__(self):
            self.n = 1
    m = Mutable()
    _inject("--bmu", m)
    captured = _ev("(bean --bmu)")
    m.n = 99
    _inject("--bmu-cap", captured)
    assert _ev("(:n --bmu-cap)") == 99
    m.n = 7
    assert _ev("(:n --bmu-cap)") == 7


def test_bean_keys_captured_at_creation():
    # Property NAMES are captured once at bean-creation; new attributes
    # added later are NOT exposed by the captured bean (matches vanilla:
    # the property descriptor list is a snapshot, the values are live).
    class Add:
        def __init__(self):
            self.a = 1
    obj = Add()
    _inject("--add", obj)
    captured = _ev("(bean --add)")
    obj.b = 2
    _inject("--add-cap", captured)
    keys = {str(k) for k in _ev("(keys --add-cap)")}
    assert keys == {":a"}, keys


def test_bean_count_lookup_via_seq():
    class Q:
        def __init__(self):
            self.x = 10
            self.y = 20
            self.z = 30
    _inject("--bcq", Q())
    assert _ev("(count (bean --bcq))") == 3
    pairs = list(_ev("(seq (bean --bcq))"))
    decoded = {str(list(p)[0]): list(p)[1] for p in pairs}
    assert decoded == {":x": 10, ":y": 20, ":z": 30}


def test_bean_invokable_as_fn():
    # Like a map, a bean can be invoked with a key.
    class K:
        def __init__(self):
            self.foo = 42
    _inject("--bk", K())
    assert _ev("((bean --bk) :foo)") == 42
    assert _ev("((bean --bk) :missing :default)") == _ev(":default")


def test_bean_equiv_to_persistent_map():
    # Same shape + values → equal to the corresponding hash-map.
    class P:
        def __init__(self):
            self.a = 1
            self.b = 2
    _inject("--bep", P())
    assert _ev("(= (bean --bep) {:a 1 :b 2})") is True
    assert _ev("(= (bean --bep) {:a 1 :b 99})") is False
