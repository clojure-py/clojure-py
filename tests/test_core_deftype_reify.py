"""Tests for reify + deftype (sub-batch B of core_deftype.clj).

Both are implemented as Clojure macros that emit a call to the
-build-type runtime helper, which calls Python's `type(name, bases,
attrs)` to materialize a class. Methods are attached as both class
attributes (so `(.method inst)` works through Python's bound-method
mechanism) AND as protocol impls (so `(proto-method inst)` goes
through extend's dispatch cache).

Notes on adaptations:
  - definterface skipped (Python uses ABCs).
  - defrecord skipped — needs IPersistentMap implementation, value
    equality, hash; coming in a follow-up slice.
  - JVM type hints / primitive args / mutable-field flags accepted but
    ignored (Python is fully dynamic).
  - deftype auto-binds field names inside method bodies via a wrapping
    `let [f (.-f this) ...]`. JVM does this via compiler magic; we do
    it via macroexpansion.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword, Symbol,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


_proto_counter = [0]


def fresh_proto():
    _proto_counter[0] += 1
    return f"TCB37_P{_proto_counter[0]}"


# --- reify basics ----------------------------------------------

def test_reify_implements_protocol():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(def r-inst (reify {name} (m [this] :hello)))")
    assert E("(m r-inst)") == K("hello")

def test_reify_satisfies_pred():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(def r-inst (reify {name} (m [this] 1)))")
    assert E(f"(satisfies? {name} r-inst)") is True

def test_reify_multiple_methods():
    name = fresh_proto()
    E(f"(defprotocol {name} (m1 [x]) (m2 [x y]))")
    E(f"(def r-inst (reify {name} (m1 [this] :a) (m2 [this y] [:b y])))")
    assert E("(m1 r-inst)") == K("a")
    assert list(E("(m2 r-inst 7)")) == [K("b"), 7]

def test_reify_multiple_protocols():
    n1 = fresh_proto()
    n2 = fresh_proto()
    E(f"(defprotocol {n1} (p1 [x]))")
    E(f"(defprotocol {n2} (p2 [x]))")
    E(f"""(def r-inst
            (reify {n1} (p1 [this] :one)
                   {n2} (p2 [this] :two)))""")
    assert E("(p1 r-inst)") == K("one")
    assert E("(p2 r-inst)") == K("two")


# --- reify closure capture -------------------------------------

def test_reify_captures_outer_let():
    name = fresh_proto()
    E(f"(defprotocol {name} (val [x]))")
    out = E(f"""
      (let [n 42]
        (val (reify {name} (val [this] n))))""")
    assert out == 42

def test_reify_independent_closures():
    """Two reify forms in different scopes capture different env."""
    name = fresh_proto()
    E(f"(defprotocol {name} (val [x]))")
    E(f"(def make-r (fn [n] (reify {name} (val [this] n))))")
    E("(def r1 (make-r 100))")
    E("(def r2 (make-r 200))")
    assert E("(val r1)") == 100
    assert E("(val r2)") == 200


# --- reify .method interop -------------------------------------

def test_reify_method_callable_via_dot():
    """The method is also a Python class attr — `.method inst` works."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(def r-inst (reify {name} (m [this] :direct)))")
    assert E("(.m r-inst)") == K("direct")


# --- reify produces fresh classes ------------------------------

def test_reify_fresh_class_each_call():
    """Each reify form produces a distinct class, so two reify
    expressions in the same scope don't share the same class."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(def r1 (reify {name} (m [this] :a)))")
    E(f"(def r2 (reify {name} (m [this] :b)))")
    assert E("(identical? (class r1) (class r2))") is False


# --- deftype basics --------------------------------------------

def test_deftype_creates_class_with_fields():
    E("(deftype TCBPoint [x y])")
    E("(def p (TCBPoint. 3 4))")
    assert E("(.-x p)") == 3
    assert E("(.-y p)") == 4

def test_deftype_factory_fn():
    """deftype emits a ->Name positional factory."""
    E("(deftype TCBVec [a b c])")
    E("(def v (->TCBVec 1 2 3))")
    assert E("(.-a v)") == 1
    assert E("(.-b v)") == 2
    assert E("(.-c v)") == 3

def test_deftype_class_name():
    E("(deftype TCBNamed [a])")
    assert E("(.-__name__ TCBNamed)") == "TCBNamed"

def test_deftype_no_fields():
    E("(deftype TCBEmpty [])")
    E("(def e (TCBEmpty.))")
    assert E("(.-__name__ (class e))") == "TCBEmpty"


# --- deftype with protocol impls -------------------------------

def test_deftype_implements_protocol():
    name = fresh_proto()
    E(f"(defprotocol {name} (sum [v]))")
    E(f"(deftype TCBPoint2 [x y] {name} (sum [this] (+ x y)))")
    E("(def p (TCBPoint2. 3 4))")
    assert E("(sum p)") == 7

def test_deftype_field_names_in_body():
    """Field names used directly in method body — auto-bound to
    instance attrs via the let-wrapper."""
    name = fresh_proto()
    E(f"(defprotocol {name} (info [v]))")
    E(f"(deftype TCBData [a b] {name} (info [this] [a b (+ a b)]))")
    out = list(E("(info (TCBData. 10 20))"))
    assert out == [10, 20, 30]

def test_deftype_method_callable_via_dot():
    name = fresh_proto()
    E(f"(defprotocol {name} (label [v]))")
    E(f"(deftype TCBLab [n] {name} (label [this] [:lab n]))")
    out = list(E("(.label (TCBLab. 99))"))
    assert out == [K("lab"), 99]

def test_deftype_multi_arity_method():
    name = fresh_proto()
    E(f"(defprotocol {name} (op [v] [v y]))")
    E(f"""(deftype TCBOp [base]
            {name}
            (op ([this] base)
                ([this y] (+ base y))))""")
    E("(def o (TCBOp. 100))")
    assert E("(op o)") == 100
    assert E("(op o 5)") == 105


# --- deftype + multiple protocols ------------------------------

def test_deftype_multiple_protocols():
    n1 = fresh_proto()
    n2 = fresh_proto()
    E(f"(defprotocol {n1} (a [v]))")
    E(f"(defprotocol {n2} (b [v]))")
    E(f"""(deftype TCBMulti [x]
            {n1} (a [this] [:a x])
            {n2} (b [this] [:b x]))""")
    E("(def m (TCBMulti. 7))")
    assert list(E("(a m)")) == [K("a"), 7]
    assert list(E("(b m)")) == [K("b"), 7]


# --- deftype + extend later ------------------------------------

def test_deftype_extend_after_definition():
    """A type defined via deftype can be extended later."""
    name = fresh_proto()
    E("(deftype TCBLater [v])")
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type TCBLater {name} (m [this] [:later (.-v this)]))")
    out = list(E("(m (TCBLater. 5))"))
    assert out == [K("later"), 5]


# --- field auto-binding doesn't leak ---------------------------

def test_deftype_field_let_does_not_leak():
    """Fields are bound via a wrapping `let`, so binding `x` outside
    the method shouldn't conflict — the inner let shadows.

    Specifically: a let outside the deftype that uses `x` works fine
    (the deftype's `x` only exists inside its method bodies)."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [v]))")
    E(f"(deftype TCBField [x] {name} (m [this] x))")
    out = E(f"""
      (let [x 999]
        (m (TCBField. 10)))""")
    # The deftype method sees its OWN x (the field), not the outer let's x.
    assert out == 10


# --- deftype with no protocols ---------------------------------

def test_deftype_struct_only():
    """A deftype without any protocols is just a struct."""
    E("(deftype TCBStruct [a b])")
    E("(def s (TCBStruct. :hello :world))")
    assert E("(.-a s)") == K("hello")
    assert E("(.-b s)") == K("world")
