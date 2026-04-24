"""Tests for Clojure-level `defprotocol`, `extend-type`, `extend-protocol`,
and `satisfies?`.

Runtime-created protocols participate in the same dispatch machinery as
Rust-defined ones — exact PyType lookup → MRO walk → optional
`__clj_meta__` → optional fallback.
"""

import sys
import pytest
from clojure._core import eval_string, Protocol, ProtocolMethod


def _ev(s):
    return eval_string(s)


def _inject(name, obj):
    _ev("(def %s nil)" % name)
    sys.modules["clojure.user"].__dict__[name].bind_root(obj)


# --- defprotocol creates a Protocol + method Vars ---


def test_defprotocol_creates_protocol_instance():
    _ev("(defprotocol PA (foo [x]))")
    p = _ev("PA")
    assert isinstance(p, Protocol)


def test_defprotocol_creates_method_dispatcher():
    _ev("(defprotocol PB (bar [x]))")
    m = _ev("bar")
    assert isinstance(m, ProtocolMethod)


def test_defprotocol_multiple_methods():
    _ev("(defprotocol PC (m1 [x]) (m2 [x y]) (m3 [x y z]))")
    assert isinstance(_ev("m1"), ProtocolMethod)
    assert isinstance(_ev("m2"), ProtocolMethod)
    assert isinstance(_ev("m3"), ProtocolMethod)


def test_defprotocol_with_docstring():
    _ev('(defprotocol PD "A protocol." (m [x]))')
    assert isinstance(_ev("PD"), Protocol)


# --- extend-type registers impls on a type ---


def test_extend_type_registers_impl():
    _ev("(defprotocol PE (describe [x]))")

    class Foo:
        pass
    _inject("--Foo", Foo)

    _ev('(extend-type --Foo PE (describe [f] "a foo"))')
    f = Foo()
    _inject("--f", f)
    assert _ev("(describe --f)") == "a foo"


def test_extend_type_multiple_methods():
    _ev("(defprotocol PF (a [x]) (b [x y]))")

    class Bar:
        def __init__(self, v):
            self.v = v
    _inject("--Bar", Bar)

    _ev("""
        (extend-type --Bar
          PF
          (a [x] (clojure.lang.RT/getattr x "v" 0))
          (b [x y] (+ (clojure.lang.RT/getattr x "v" 0) y)))
    """)
    b = Bar(10)
    _inject("--b", b)
    assert _ev("(a --b)") == 10
    assert _ev("(b --b 5)") == 15


def test_extend_type_multiple_protocols():
    _ev("(defprotocol PG (g1 [x]))")
    _ev("(defprotocol PH (h1 [x]))")

    class Zap:
        pass
    _inject("--Zap", Zap)

    _ev("""
        (extend-type --Zap
          PG (g1 [_] :g)
          PH (h1 [_] :h))
    """)
    z = Zap()
    _inject("--z", z)
    assert _ev("(g1 --z)") == _ev(":g")
    assert _ev("(h1 --z)") == _ev(":h")


# --- extend-protocol: one protocol, many types ---


def test_extend_protocol_multiple_types():
    _ev("(defprotocol PI (shape-name [x]))")

    class A:
        pass
    class B:
        pass
    _inject("--A", A)
    _inject("--B", B)

    _ev("""
        (extend-protocol PI
          --A (shape-name [_] "a")
          --B (shape-name [_] "b"))
    """)
    _inject("--a-inst", A())
    _inject("--b-inst", B())
    assert _ev("(shape-name --a-inst)") == "a"
    assert _ev("(shape-name --b-inst)") == "b"


# --- satisfies? ---


def test_satisfies_true_when_extended():
    _ev("(defprotocol PJ (mj [x]))")

    class C:
        pass
    _inject("--C", C)
    _ev("(extend-type --C PJ (mj [_] 1))")
    _inject("--c-inst", C())
    assert _ev("(satisfies? PJ --c-inst)") is True


def test_satisfies_false_when_not_extended():
    _ev("(defprotocol PK (mk [x]))")

    class D:
        pass
    _inject("--D", D)
    _inject("--d-inst", D())
    assert _ev("(satisfies? PK --d-inst)") is False


def test_satisfies_via_mro():
    _ev("(defprotocol PL (ml [x]))")

    class Parent:
        pass
    class Child(Parent):
        pass
    _inject("--Parent", Parent)
    _inject("--Child", Child)

    _ev("(extend-type --Parent PL (ml [_] :parent))")
    _inject("--child-inst", Child())
    # Child inherits via MRO.
    assert _ev("(satisfies? PL --child-inst)") is True
    assert _ev("(ml --child-inst)") == _ev(":parent")


# --- Extension on a Rust pyclass ---


def test_extend_rust_pyclass():
    from clojure._core import Atom
    _ev("(defprotocol PM (me [x]))")
    _inject("--Atom", Atom)
    _ev('(extend-type --Atom PM (me [_] "from atom"))')
    assert _ev('(me (atom 42))') == "from atom"


# --- Re-extension bumps epoch + is picked up ---


def test_re_extend_replaces_impl():
    _ev("(defprotocol PN (nn [x]))")

    class E:
        pass
    _inject("--E", E)
    _inject("--e-inst", E())

    _ev("(extend-type --E PN (nn [_] :v1))")
    assert _ev("(nn --e-inst)") == _ev(":v1")

    _ev("(extend-type --E PN (nn [_] :v2))")
    assert _ev("(nn --e-inst)") == _ev(":v2")


# --- No implementation raises ---


def test_no_impl_raises():
    _ev("(defprotocol PO (oo [x]))")

    class F:
        pass
    _inject("--F", F)
    _inject("--f-inst", F())
    with pytest.raises(Exception):
        _ev("(oo --f-inst)")


# --- deftype ---


def test_deftype_basic():
    _ev("(defprotocol PP (p-method [x]))")
    _ev("(deftype Pair [a b] PP (p-method [this] (clojure.lang.RT/getattr this \"a\" nil)))")
    inst = _ev("(->Pair 10 20)")
    assert inst.a == 10
    assert inst.b == 20
    assert _ev("(p-method (->Pair 5 nil))") == 5


def test_deftype_method_uses_field_via_getattr():
    _ev("(defprotocol PQ (q-sum [x]))")
    _ev("""
        (deftype Summable [x y]
          PQ (q-sum [this]
              (+ (clojure.lang.RT/getattr this "x" 0)
                 (clojure.lang.RT/getattr this "y" 0))))
    """)
    assert _ev("(q-sum (->Summable 3 4))") == 7


def test_deftype_multiple_protocols():
    _ev("(defprotocol PR (r-a [x]))")
    _ev("(defprotocol PS (s-b [x]))")
    _ev("""
        (deftype TwoFer []
          PR (r-a [_] :a)
          PS (s-b [_] :b))
    """)
    inst = _ev("(->TwoFer)")
    _inject("--tf-inst", inst)
    assert _ev("(r-a --tf-inst)") == _ev(":a")
    assert _ev("(s-b --tf-inst)") == _ev(":b")


def test_deftype_instances_distinct():
    _ev("(deftype Holder [v])")
    i1 = _ev("(->Holder 1)")
    i2 = _ev("(->Holder 2)")
    assert i1 is not i2
    assert i1.v == 1
    assert i2.v == 2


# --- defrecord ---


def test_defrecord_positional_constructor():
    _ev("(defrecord Point [x y])")
    p = _ev("(->Point 3 4)")
    assert p.x == 3
    assert p.y == 4


def test_defrecord_keyword_access():
    _ev("(defrecord PointA [a b])")
    assert _ev("(:a (->PointA 11 22))") == 11
    assert _ev("(:b (->PointA 11 22))") == 22


def test_defrecord_get_works():
    _ev("(defrecord PointB [p q])")
    assert _ev("(get (->PointB 1 2) :p)") == 1
    assert _ev("(get (->PointB 1 2) :missing :d)") == _ev(":d")


def test_defrecord_map_constructor():
    _ev("(defrecord PointC [x y])")
    p = _ev("(map->PointC {:x 100 :y 200})")
    assert p.x == 100
    assert p.y == 200


def test_defrecord_with_protocol():
    _ev("(defprotocol PT (t-label [x]))")
    _ev('(defrecord Labeled [n] PT (t-label [this] (clojure.lang.RT/str-concat "lbl:" (str (:n this)))))')
    assert _ev("(t-label (->Labeled 7))") == "lbl:7"


def test_defrecord_field_access_in_method_via_keyword():
    _ev("(defprotocol PU (u-sum [x]))")
    _ev("(defrecord Vec2 [x y] PU (u-sum [this] (+ (:x this) (:y this))))")
    assert _ev("(u-sum (->Vec2 3 4))") == 7


# --- reify ---


def test_reify_single_protocol():
    _ev("(defprotocol PV (v-sing [x]))")
    assert _ev('(v-sing (reify PV (v-sing [_] "from-reify")))') == "from-reify"


def test_reify_multiple_protocols():
    _ev("(defprotocol PW (w-a [x]))")
    _ev("(defprotocol PX (x-b [x]))")
    result = _ev("""
        (let [r (reify
                  PW (w-a [_] 1)
                  PX (x-b [_] 2))]
          [(w-a r) (x-b r)])
    """)
    assert list(result) == [1, 2]


def test_reify_closes_over_outer_scope():
    _ev("(defprotocol PY (y-val [x]))")
    assert _ev("(let [secret 99 r (reify PY (y-val [_] secret))] (y-val r))") == 99


def test_reify_each_call_fresh_class():
    _ev("(defprotocol PZ (z-m [x]))")
    r1 = _ev("(reify PZ (z-m [_] 1))")
    r2 = _ev("(reify PZ (z-m [_] 2))")
    # Different anonymous classes.
    assert type(r1) is not type(r2)
