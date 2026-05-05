"""Tests for core.clj batch 36: the reduce-redef family (JVM 6913-7111).

What this batch lands:
  - Inst protocol + inst-ms / inst-ms* / inst?
  - uuid? / random-uuid (Python uuid module instead of java.util.UUID)
  - reduce, redefined to dispatch on IReduce / IReduceInit and fall
    through to clojure.core.protocols/coll-reduce. Replaces the
    bootstrap reduce1.
  - clojure.core.protocols/IKVReduce extension over nil / Object /
    clojure.lang.IKVReduce — the ABC version is the fast path,
    Object's impl iterates entries via reduce.
  - reduce-kv, completing, transduce, into, mapv, filterv
  - slurp / spit (Python open() instead of clojure.java.io)

Adaptations from JVM:
  - Skipped stream-reduce! / stream-seq! / stream-transduce! /
    stream-into! (java.util.stream.BaseStream has no Python analog).
  - Skipped (load "instant") + Timestamp protocol extension (JVM-only
    java.sql.Timestamp).
  - Inst extends over py.datetime/datetime instead of java.util.Date /
    java.time.Instant. .timestamp() returns seconds; multiply by 1000.
  - uuid? / random-uuid use Python's uuid module instead of
    java.util.UUID.
  - slurp / spit use Python open() with mode/encoding instead of
    clojure.java.io/reader|writer.

Protocol-machinery fix shaken out by this batch:
  find-impl-for-class now walks impls in JVM-faithful order
  (super-chain + pref): direct hit, then __mro__ excluding `object`,
  then virtual bases excluding `object`, then `object` last. Previous
  order let an Object extension always win over a more-specific
  virtual-base extension (like IKVReduce on PersistentVector).
"""

import datetime as _dt
import os as _os
import uuid as _uuid

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


# --- reduce ------------------------------------------------------

def test_reduce_with_init():
    assert E("(reduce + 0 [1 2 3 4])") == 10

def test_reduce_no_init():
    assert E("(reduce + [1 2 3 4])") == 10

def test_reduce_empty_no_init_calls_f():
    """No-init reduce on an empty coll calls (f)."""
    out = E("(reduce (fn ([] :empty) ([a b] [a b])) [])")
    assert out == K("empty")

def test_reduce_empty_with_init_returns_init():
    assert E("(reduce + 99 [])") == 99

def test_reduce_nil():
    assert E("(reduce + 99 nil)") == 99
    out = E("(reduce (fn ([] :empty) ([a b] b)) nil)")
    assert out == K("empty")

def test_reduce_single_no_init():
    """Single-item coll with no init returns the item, doesn't call f."""
    assert E("(reduce (fn [_ _] :should-not-fire) [42])") == 42

def test_reduce_honors_reduced():
    out = E("""
      (reduce (fn [acc x] (if (> x 3) (reduced acc) (+ acc x)))
              0 [1 2 3 4 5])""")
    assert out == 6  # 1+2+3 = 6, then x=4 triggers reduced

def test_reduce_dispatches_to_ireduce():
    """PersistentVector implements IReduce — verify the fast path
    works by reducing a large vector."""
    assert E("(reduce + 0 (vec (range 100)))") == 4950


# --- reduce-kv --------------------------------------------------

def test_reduce_kv_vector():
    """reduce-kv on a vector iterates (index, value) pairs."""
    out = E("(reduce-kv (fn [acc i v] (conj acc [i v])) [] [10 20 30])")
    out_list = list(out)
    assert [list(x) for x in out_list] == [[0, 10], [1, 20], [2, 30]]

def test_reduce_kv_map():
    """reduce-kv on a map iterates (k, v) pairs."""
    out = E("(reduce-kv (fn [acc k v] (assoc acc v k)) {} (sorted-map :a 1 :b 2))")
    assert dict(out) == {1: K("a"), 2: K("b")}

def test_reduce_kv_nil():
    assert E("(reduce-kv (fn [_ _ _] :nope) :init nil)") == K("init")

def test_reduce_kv_empty():
    assert E("(reduce-kv (fn [_ _ _] :nope) :init [])") == K("init")
    assert E("(reduce-kv (fn [_ _ _] :nope) :init {})") == K("init")


# --- completing -------------------------------------------------

def test_completing_no_finalizer():
    """No finalizer → identity. Arity-1 just returns its arg."""
    f = E("(completing +)")
    assert f() == 0
    assert f(5, 3) == 8
    assert f(42) == 42

def test_completing_with_finalizer():
    f = E("(completing + str)")
    assert f(5, 3) == 8
    assert f(42) == "42"


# --- transduce ---------------------------------------------------

def test_transduce_with_init():
    assert E("(transduce (map inc) + 0 [1 2 3])") == 9

def test_transduce_no_init_uses_f():
    """No-init transduce calls (f) for the seed."""
    assert E("(transduce (map inc) + [1 2 3])") == 9

def test_transduce_filter():
    assert E("(transduce (filter even?) + 0 [1 2 3 4 5 6])") == 12

def test_transduce_composed():
    assert E("""
      (transduce (comp (filter odd?) (map (fn [x] (* x x))))
                 + 0 [1 2 3 4 5])""") == 35  # 1+9+25

def test_transduce_finalizes_via_completing():
    """A reducing fn with a separate finalizer; transduce calls (f ret)."""
    out = E("""
      (transduce (map inc)
                 (completing + str)
                 0 [1 2 3])""")
    assert out == "9"


# --- into --------------------------------------------------------

def test_into_zero_args():
    assert list(E("(into)")) == []

def test_into_identity():
    assert list(E("(into [1 2 3])")) == [1, 2, 3]

def test_into_basic():
    out = E("(into [1] [2 3 4])")
    assert list(out) == [1, 2, 3, 4]

def test_into_empty_to():
    assert dict(E("(into {} [[:a 1] [:b 2]])")) == {K("a"): 1, K("b"): 2}

def test_into_set_dedupes():
    out = E("(into #{} [1 2 2 3])")
    assert sorted(out) == [1, 2, 3]

def test_into_with_xform():
    """3-arity into uses the transducer path."""
    assert list(E("(into [] (map inc) [1 2 3])")) == [2, 3, 4]

def test_into_xform_filter():
    assert list(E("(into [] (filter even?) [1 2 3 4 5 6])")) == [2, 4, 6]

def test_into_preserves_meta():
    """into copies meta from the destination collection."""
    out = E("""
      (let [src (with-meta [] {:tag :important})]
        (meta (into src [1 2 3])))""")
    assert dict(out)[K("tag")] == K("important")


# --- mapv --------------------------------------------------------

def test_mapv_single():
    out = E("(mapv inc [1 2 3])")
    assert list(out) == [2, 3, 4]

def test_mapv_multi_2():
    out = E("(mapv + [1 2 3] [10 20 30])")
    assert list(out) == [11, 22, 33]

def test_mapv_multi_3():
    out = E("(mapv + [1 2 3] [10 20 30] [100 200 300])")
    assert list(out) == [111, 222, 333]

def test_mapv_multi_4():
    out = E("(mapv + [1 2] [10 20] [100 200] [1000 2000])")
    assert list(out) == [1111, 2222]

def test_mapv_uneven_truncates():
    out = E("(mapv + [1 2 3 4] [10 20])")
    assert list(out) == [11, 22]

def test_mapv_returns_vector():
    """Result is always a vector (not a lazy seq)."""
    out = E("(mapv inc [1 2 3])")
    assert E(f"(vector? '{out})") or hasattr(out, "nth")


# --- filterv -----------------------------------------------------

def test_filterv_basic():
    out = E("(filterv even? [1 2 3 4 5 6])")
    assert list(out) == [2, 4, 6]

def test_filterv_empty_input():
    assert list(E("(filterv pos? [])")) == []

def test_filterv_no_match():
    assert list(E("(filterv neg? [1 2 3])")) == []

def test_filterv_returns_vector():
    out = E("(filterv even? [1 2 3 4])")
    assert hasattr(out, "nth")


# --- Inst protocol -----------------------------------------------

def test_inst_pred_true_for_datetime():
    out = E("(inst? (py.datetime/datetime 2020 1 1))")
    assert out is True

def test_inst_pred_false_for_other():
    assert E("(inst? 42)") is False
    assert E('(inst? "hello")') is False
    assert E("(inst? nil)") is False

def test_inst_ms_epoch():
    """1970-01-01 UTC → 0 ms."""
    utc = "(.-utc py.datetime/timezone)"
    out = E(f"(inst-ms (py.datetime/datetime 1970 1 1 0 0 0 0 {utc}))")
    assert out == 0

def test_inst_ms_known_date():
    """2020-01-01 UTC → 1577836800000 ms."""
    utc = "(.-utc py.datetime/timezone)"
    out = E(f"(inst-ms (py.datetime/datetime 2020 1 1 0 0 0 0 {utc}))")
    assert out == 1577836800000


# --- uuid --------------------------------------------------------

def test_random_uuid_returns_uuid():
    u = E("(random-uuid)")
    assert isinstance(u, _uuid.UUID)

def test_random_uuid_distinct():
    """Each call returns a fresh UUID."""
    u1 = E("(random-uuid)")
    u2 = E("(random-uuid)")
    assert u1 != u2

def test_uuid_pred_true():
    assert E("(uuid? (random-uuid))") is True

def test_uuid_pred_false():
    assert E("(uuid? 42)") is False
    assert E("(uuid? nil)") is False
    assert E('(uuid? "abc")') is False

def test_random_uuid_is_v4():
    """random-uuid produces type-4 UUIDs."""
    u = E("(random-uuid)")
    assert u.version == 4


# --- slurp / spit -----------------------------------------------

def test_spit_then_slurp_string(tmp_path):
    p = tmp_path / "tcb36.txt"
    E(f'(spit "{p}" "hello world")')
    assert E(f'(slurp "{p}")') == "hello world"

def test_spit_overwrites_by_default(tmp_path):
    p = tmp_path / "tcb36b.txt"
    E(f'(spit "{p}" "first")')
    E(f'(spit "{p}" "second")')
    assert E(f'(slurp "{p}")') == "second"

def test_spit_append_mode(tmp_path):
    p = tmp_path / "tcb36c.txt"
    E(f'(spit "{p}" "first")')
    E(f'(spit "{p}" "-second" :append true)')
    assert E(f'(slurp "{p}")') == "first-second"

def test_slurp_file_like():
    """slurp accepts any object with a .read method."""
    import io
    Compiler.eval(read_string('(def -tcb-buf nil)'))
    from clojure.lang import Var, Namespace
    buf = io.StringIO("hello from buffer")
    Var.intern(Compiler.current_ns(), Symbol.intern("-tcb-buf"), buf)
    out = E("(slurp -tcb-buf)")
    assert out == "hello from buffer"

def test_spit_file_like():
    """spit accepts any object with a .write method."""
    import io
    Compiler.eval(read_string('(def -tcb-out nil)'))
    from clojure.lang import Var
    buf = io.StringIO()
    Var.intern(Compiler.current_ns(), Symbol.intern("-tcb-out"), buf)
    E('(spit -tcb-out "writing!")')
    assert buf.getvalue() == "writing!"


# --- find-impl-for-class ordering fix ---------------------------

def test_protocol_dispatch_prefers_specific_over_object():
    """A protocol extended over both Object and a more-specific virtual
    base must dispatch to the more-specific one."""
    E("(defprotocol -TCB-Spec (sp [x]))")
    E("(extend-type Object -TCB-Spec (sp [x] :object))")
    E("(extend-type Number -TCB-Spec (sp [x] :number))")
    # Number is a virtual base of int via numbers.Number ABC.
    assert E("(sp 5)") == K("number")
    # str isn't covered by Number → falls through to Object.
    assert E('(sp "x")') == K("object")


# --- IKVReduce ABC fast path ------------------------------------

def test_kv_reduce_uses_abc_fast_path():
    """PersistentArrayMap registers as IKVReduce; reduce-kv should hit
    that fast path. Verify by checking iteration order / correctness
    for a sorted map (whose .kv_reduce yields entries in sorted order)."""
    out = E("""
      (reduce-kv (fn [acc k v] (conj acc [k v]))
                 []
                 (sorted-map :z 1 :a 2 :m 3))""")
    keys_order = [list(pair)[0] for pair in out]
    assert keys_order == [K("a"), K("m"), K("z")]
