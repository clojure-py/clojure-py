"""Tests for the protocols section of core_deftype.clj (JVM 508-919 of
core_deftype.clj — sub-batch A; reify/deftype/defrecord come later).

What's covered here:
  defprotocol, extend, extend-type, extend-protocol,
  satisfies?, extends?, extenders,
  find-protocol-impl, find-protocol-method,
  -reset-methods,
  :extend-via-meta opt.

Implementation notes worth calling out:
  - The protocol var holds a regular Clojure map. extend uses
    alter-var-root + assoc-in [:impls cls] to install the impl map.
  - Each method gets its own per-class dispatch cache (a Python dict
    stashed in the method var's meta). -reset-methods clears them on
    impls change.
  - Class hierarchy walk: cls.__mro__ first (concrete inheritance),
    then a scan over registered impl classes via isa? (handles
    virtual bases / Python ABCs like numbers.Number).
  - nil dispatch works because (class nil) is None, used as the
    direct-hit key.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword, Symbol, Namespace, Var,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# Each test gets a fresh protocol name so we don't pollute the namespace
# across tests.
_proto_counter = [0]


def fresh_proto():
    _proto_counter[0] += 1
    return f"TCB36_P{_proto_counter[0]}"


# --- defprotocol basics -----------------------------------------

def test_defprotocol_creates_var_with_map():
    name = fresh_proto()
    E(f"(defprotocol {name} (m1 [x]))")
    val = E(name)
    assert val[K("name")] == Symbol.intern(name)
    assert K("m1") in dict(val[K("sigs")])

def test_defprotocol_with_docstring():
    name = fresh_proto()
    E(f'(defprotocol {name} "the doc" (m [x]))')
    val = E(name)
    assert val[K("doc")] == "the doc"

def test_defprotocol_method_doc():
    name = fresh_proto()
    E(f'(defprotocol {name} (m [x] "method doc"))')
    val = E(name)
    sig = val[K("sigs")][K("m")]
    assert sig[K("doc")] == "method doc"

def test_defprotocol_arglists():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x] [x y] [x y z]))")
    val = E(name)
    arglists = val[K("sigs")][K("m")][K("arglists")]
    assert len(list(arglists)) == 3


# --- extend / extend-type ---------------------------------------

def test_extend_type_dispatches_on_class():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] [:int x]))")
    out = E("(m 7)")
    assert list(out) == [K("int"), 7]

def test_extend_type_multiple_methods():
    name = fresh_proto()
    E(f"(defprotocol {name} (a [x]) (b [x]))")
    E(f"(extend-type py.__builtins__/int {name} (a [x] :a) (b [x] :b))")
    assert E("(a 1)") == K("a")
    assert E("(b 1)") == K("b")

def test_extend_type_multiple_protocols():
    n1 = fresh_proto()
    n2 = fresh_proto()
    E(f"(defprotocol {n1} (p1 [x]))")
    E(f"(defprotocol {n2} (p2 [x]))")
    E(f"(extend-type py.__builtins__/int {n1} (p1 [x] :one) {n2} (p2 [x] :two))")
    assert E("(p1 1)") == K("one")
    assert E("(p2 1)") == K("two")

def test_extend_function_takes_method_map():
    """The underlying extend fn takes alternating (proto, method-map)."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend py.__builtins__/str {name} {{:m (fn [s] (str \"S:\" s))}})")
    assert E('(m "hi")') == "S:hi"

def test_extend_protocol_multiple_types():
    name = fresh_proto()
    E(f"(defprotocol {name} (g [x]))")
    E(f"""(extend-protocol {name}
            py.__builtins__/int (g [x] :i)
            py.__builtins__/str (g [x] :s))""")
    assert E("(g 1)") == K("i")
    assert E('(g "x")') == K("s")


# --- multi-arity dispatch --------------------------------------

def test_extend_multi_arity_method():
    name = fresh_proto()
    E(f"(defprotocol {name} (op [x] [x y] [x y z]))")
    E(f"""(extend-type py.__builtins__/int {name}
            (op ([x] x)
                ([x y] (+ x y))
                ([x y z] (+ x y z))))""")
    assert E("(op 5)") == 5
    assert E("(op 5 6)") == 11
    assert E("(op 1 2 3)") == 6


# --- nil dispatch ----------------------------------------------

def test_extend_nil():
    name = fresh_proto()
    E(f"(defprotocol {name} (n [x]))")
    E(f"(extend-type nil {name} (n [_] :nil-impl))")
    assert E("(n nil)") == K("nil-impl")

def test_dispatch_distinct_for_nil_and_other():
    name = fresh_proto()
    E(f"(defprotocol {name} (n [x]))")
    E(f"(extend-type nil {name} (n [_] :nilcase))")
    E(f"(extend-type py.__builtins__/int {name} (n [_] :intcase))")
    assert E("(n nil)") == K("nilcase")
    assert E("(n 7)") == K("intcase")


# --- virtual bases (Python ABCs) -------------------------------

def test_extend_abc_dispatches_to_concrete():
    """Extending Number ABC should match int and float instances."""
    name = fresh_proto()
    E(f"(defprotocol {name} (dbl [x]))")
    E(f"(extend-type Number {name} (dbl [x] (* 2 x)))")
    assert E("(dbl 3)") == 6
    assert E("(dbl 2.5)") == 5.0

def test_extends_pred_for_abc():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type Number {name} (m [x] x))")
    assert E(f"(extends? {name} py.__builtins__/int)") is True
    assert E(f"(extends? {name} py.__builtins__/float)") is True
    assert E(f"(extends? {name} py.__builtins__/str)") is False


# --- satisfies? / extenders ------------------------------------

def test_satisfies_true_after_extend():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] x))")
    assert E(f"(satisfies? {name} 5)") is True
    assert E(f'(satisfies? {name} "x")') is False
    assert E(f"(satisfies? {name} nil)") is False

def test_satisfies_for_subclass_via_abc():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type Number {name} (m [x] x))")
    assert E(f"(satisfies? {name} 1.5)") is True
    assert E(f"(satisfies? {name} 5)") is True

def test_extenders_lists_extended_classes():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] x))")
    E(f"(extend-type py.__builtins__/str {name} (m [x] x))")
    out = list(E(f"(extenders {name})"))
    assert int in out
    assert str in out


# --- find-protocol-impl / find-protocol-method -----------------

def test_find_protocol_impl():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]) (n [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] :a) (n [x] :b))")
    out = E(f"(find-protocol-impl {name} 5)")
    assert dict(out) == {K("m"): out[K("m")], K("n"): out[K("n")]}

def test_find_protocol_method():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] (* x 10)))")
    f = E(f"(find-protocol-method {name} :m 5)")
    assert callable(f)
    assert f(5) == 50

def test_find_protocol_impl_nil_when_not_extended():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    assert E(f"(find-protocol-impl {name} 5)") is None


# --- dispatch error --------------------------------------------

def test_dispatch_error_for_unextended_class():
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    with pytest.raises(Exception, match="No implementation"):
        E("(m 5)")


# --- cache invalidation ----------------------------------------

def test_redefining_impl_visible_after_extend():
    """Extending a class twice — the second extend wins immediately."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] :first))")
    assert E("(m 1)") == K("first")
    E(f"(extend-type py.__builtins__/int {name} (m [x] :second))")
    assert E("(m 1)") == K("second")

def test_late_extend_visible_after_failed_call():
    """A class that didn't implement the protocol gets extended later;
    subsequent calls succeed (no negative caching)."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    with pytest.raises(Exception):
        E("(m 5)")
    E(f"(extend-type py.__builtins__/int {name} (m [x] :now))")
    assert E("(m 5)") == K("now")


# --- :extend-via-metadata --------------------------------------
#
# Per JVM contract, the metadata key is namespace-qualified
# `:proto-ns/method-name`. These tests run from the user namespace, so
# the key looks like `:user/method-name`.

def test_extend_via_metadata_basic():
    name = fresh_proto()
    E(f"(defprotocol {name} :extend-via-metadata true (m [x]))")
    out = E(f"(m (with-meta {{}} {{:user/m (fn [_] :from-meta)}}))")
    assert out == K("from-meta")

def test_extend_via_metadata_overrides_class():
    """When both class-based extend AND meta-fn are present, meta wins.

    Matches JVM: :extend-via-metadata is a per-instance escape hatch
    that overrides the protocol's regular dispatch."""
    name = fresh_proto()
    E(f"(defprotocol {name} :extend-via-metadata true (m [x]))")
    E(f"(extend-type clojure.lang.IPersistentMap {name} (m [x] :class-impl))")
    out = E(f"(m (with-meta {{}} {{:user/m (fn [_] :meta-impl)}}))")
    assert out == K("meta-impl")

def test_extend_via_metadata_falls_through_when_no_meta():
    """When :extend-via-metadata is on but x has no relevant metadata,
    fall through to class-based dispatch."""
    name = fresh_proto()
    E(f"(defprotocol {name} :extend-via-metadata true (m [x]))")
    E(f"(extend-type clojure.lang.IPersistentMap {name} (m [x] :class-impl))")
    assert E(f"(m {{}})") == K("class-impl")

def test_extend_via_metadata_default_off():
    """Without :extend-via-metadata true, metadata-based dispatch is ignored."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    with pytest.raises(Exception, match="No implementation"):
        E(f"(m (with-meta {{}} {{:user/m (fn [_] :ignored)}}))")

def test_extend_via_metadata_uses_qualified_key():
    """The unqualified key is ignored — only the namespace-qualified
    `:proto-ns/method-name` key triggers meta-dispatch."""
    name = fresh_proto()
    E(f"(defprotocol {name} :extend-via-metadata true (m [x]))")
    with pytest.raises(Exception, match="No implementation"):
        E(f"(m (with-meta {{}} {{:m (fn [_] :unqualified-ignored)}}))")


# --- defprotocol redefinition ----------------------------------

def test_defprotocol_redefinition_clears_impls():
    """Redefining a protocol resets its :impls map."""
    name = fresh_proto()
    E(f"(defprotocol {name} (m [x]))")
    E(f"(extend-type py.__builtins__/int {name} (m [x] :one))")
    assert E("(m 5)") == K("one")
    # Redefine the protocol with the same name
    E(f"(defprotocol {name} (m [x]))")
    with pytest.raises(Exception, match="No implementation"):
        E("(m 5)")
