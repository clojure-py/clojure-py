"""Tests for core.clj batch 31 (selected from JVM 5621-5808):
hierarchy machinery + small utilities.

Forms (12):
  isa?, parents, ancestors, descendants,
  derive, underive,
  distinct?,
  iterator-seq,
  format, printf,
  sequential? (out-of-JVM-order from line 6310),
  flatten (out-of-JVM-order from line 7288).

Skipped — saved for follow-up batches:
  resultset-seq    — needs java.sql.ResultSet support.
  enumeration-seq  — Python uses iterators (use iterator-seq).
  with-loading-context / ns / gen-class — class-loader machinery
                     and the heavyweight ns macro.

Adaptations from JVM source:
  isa? uses py.__builtins__/issubclass where JVM uses
       (.isAssignableFrom parent child).
  iterator-seq calls RT/chunk_iterator_seq (snake_case; JVM has
       chunkIteratorSeq).
  format uses RT/format which delegates to Python's `%` operator.
       Most Java format specifiers (%s, %d, %f) work; %n / %b
       differ.
  sequential? and flatten are pulled forward from later in JVM
       core.clj so underive (which calls flatten) is callable
       immediately.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name, ns=None):
    return Keyword.intern(ns, name)


# --- isa? ---------------------------------------------------------

def test_isa_equal_passthrough():
    assert E("(isa? :a :a)") is True
    assert E("(isa? 42 42)") is True

def test_isa_class_inheritance():
    """JVM's isAssignableFrom → Python's issubclass."""
    class Animal: pass
    class Dog(Animal): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-Animal"), Animal)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-Dog"), Dog)
    assert E("(isa? user/tcb31-Dog user/tcb31-Animal)") is True
    assert E("(isa? user/tcb31-Animal user/tcb31-Dog)") is False

def test_isa_keyword_via_derive():
    """Establish a parent/child via derive, then isa? sees it."""
    E("(derive ::tcb31-X ::tcb31-Y)")
    assert E("(isa? ::tcb31-X ::tcb31-Y)") is True
    assert E("(isa? ::tcb31-Y ::tcb31-X)") is False

def test_isa_vector_positional():
    """isa? on vectors compares positionally."""
    class A: pass
    class B(A): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-iA"), A)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-iB"), B)
    assert E("(isa? [user/tcb31-iB user/tcb31-iB] [user/tcb31-iA user/tcb31-iA])") is True
    # Length mismatch → false
    assert E("(isa? [user/tcb31-iB] [user/tcb31-iA user/tcb31-iA])") is False


# --- parents / ancestors / descendants ----------------------------

def test_parents_class_returns_immediate_supers():
    """parents on a class returns the immediate supers (= bases)."""
    class A: pass
    class B(A): pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-pA"), A)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-pB"), B)
    out = E("(parents user/tcb31-pB)")
    assert set(out) == {A}

def test_parents_keyword_via_derive():
    E("(derive ::tcb31-child ::tcb31-parent)")
    out = E("(parents ::tcb31-child)")
    assert set(out) == {K("tcb31-parent", "user")}

def test_ancestors_keyword_chain():
    E("(derive ::tcb31-a1 ::tcb31-mid)")
    E("(derive ::tcb31-mid ::tcb31-top)")
    out = E("(ancestors ::tcb31-a1)")
    assert set(out) == {K("tcb31-mid", "user"), K("tcb31-top", "user")}

def test_descendants_keyword_chain():
    E("(derive ::tcb31-d-leaf ::tcb31-d-mid)")
    E("(derive ::tcb31-d-mid ::tcb31-d-root)")
    out = E("(descendants ::tcb31-d-root)")
    assert set(out) == {K("tcb31-d-mid", "user"), K("tcb31-d-leaf", "user")}

def test_descendants_class_throws():
    """JVM forbids descendants of a class — too expensive to enumerate."""
    class A: pass
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb31-dC"), A)
    with pytest.raises(Exception, match="Can't get descendants"):
        E("(descendants user/tcb31-dC)")


# --- derive / underive --------------------------------------------

def test_derive_creates_relationship():
    E("(derive ::tcb31-dog ::tcb31-mammal)")
    assert E("(isa? ::tcb31-dog ::tcb31-mammal)") is True

def test_derive_chains():
    E("(derive ::tcb31-c1 ::tcb31-c2)")
    E("(derive ::tcb31-c2 ::tcb31-c3)")
    assert E("(isa? ::tcb31-c1 ::tcb31-c3)") is True

def test_derive_assert_parent_namespaced():
    """derive requires parent to be namespaced (asserts on plain :foo)."""
    with pytest.raises(AssertionError):
        E("(derive :tcb31-bad-child :tcb31-bad-parent)")

def test_derive_returns_nil():
    """1-arg form returns nil (mutates global hierarchy)."""
    assert E("(derive ::tcb31-rd-c ::tcb31-rd-p)") is None

def test_underive_removes_relationship():
    E("(derive ::tcb31-u-c ::tcb31-u-p)")
    assert E("(isa? ::tcb31-u-c ::tcb31-u-p)") is True
    E("(underive ::tcb31-u-c ::tcb31-u-p)")
    assert E("(isa? ::tcb31-u-c ::tcb31-u-p)") is False

def test_underive_preserves_other_relationships():
    """Underiving one parent doesn't affect other derives."""
    E("(derive ::tcb31-mp-x ::tcb31-mp-A)")
    E("(derive ::tcb31-mp-x ::tcb31-mp-B)")
    E("(underive ::tcb31-mp-x ::tcb31-mp-A)")
    assert E("(isa? ::tcb31-mp-x ::tcb31-mp-A)") is False
    assert E("(isa? ::tcb31-mp-x ::tcb31-mp-B)") is True


# --- distinct? ----------------------------------------------------

def test_distinct_pred_one_arg_true():
    assert E("(distinct? 42)") is True

def test_distinct_pred_two_args():
    assert E("(distinct? 1 2)") is True
    assert E("(distinct? 1 1)") is False

def test_distinct_pred_variadic_unique():
    assert E("(distinct? :a :b :c :d)") is True

def test_distinct_pred_variadic_dup():
    assert E("(distinct? :a :b :c :a)") is False

def test_distinct_pred_value_equality_not_identity():
    """Two equal collections aren't distinct?."""
    assert E("(distinct? [1 2 3] [1 2 3])") is False


# --- iterator-seq -------------------------------------------------

def test_iterator_seq_from_list():
    out = list(E("(iterator-seq (clojure.lang.RT/iter [10 20 30]))"))
    assert out == [10, 20, 30]

def test_iterator_seq_empty():
    """Empty iterator → nil (chunk_iterator_seq returns nil for empty)."""
    out = E("(iterator-seq (clojure.lang.RT/iter []))")
    assert out is None


# --- format / printf ---------------------------------------------

def test_format_simple_string():
    assert E('(format "hello %s" "world")') == "hello world"

def test_format_int():
    assert E('(format "%d/%d" 22 7)') == "22/7"

def test_format_float():
    """Default float formatting — exact width/precision is implementation-
    detail; just verify the substitution happens."""
    out = E('(format "%.2f" 3.14159)')
    assert out == "3.14"

def test_format_no_args():
    assert E('(format "no substitutions")') == "no substitutions"

def test_format_multiple_types():
    out = E('(format "%s = %d (%.1f%%)" "score" 95 99.5)')
    assert out == "score = 95 (99.5%)"

def test_printf_writes_to_out():
    """printf is print + format; verify it uses *out*."""
    import io
    from clojure.lang import Namespace, PersistentArrayMap
    core_ns = Namespace.find(Symbol.intern("clojure.core"))
    out_var = core_ns.find_interned_var(Symbol.intern("*out*"))
    buf = io.StringIO()
    Var.push_thread_bindings(PersistentArrayMap.create(out_var, buf))
    try:
        E('(printf "%d items" 5)')
    finally:
        Var.pop_thread_bindings()
    assert buf.getvalue() == "5 items"


# --- sequential? / flatten ---------------------------------------

def test_sequential_pred_true_for_lists_and_vectors():
    assert E("(sequential? [1 2 3])") is True
    assert E("(sequential? (list 1 2 3))") is True
    assert E("(sequential? (range 5))") is True

def test_sequential_pred_false_for_unordered():
    assert E("(sequential? {:a 1})") is False
    assert E("(sequential? #{1 2 3})") is False

def test_sequential_pred_false_for_string():
    """Python str isn't sequential? in our world (it's a Python builtin,
    not a Clojure Sequential)."""
    assert E('(sequential? "abc")') is False

def test_sequential_pred_false_for_nil_and_atoms():
    assert E("(sequential? nil)") is False
    assert E("(sequential? 42)") is False

def test_flatten_nested_collections():
    out = list(E("(flatten [[1 2] [3 [4 5]] 6])"))
    assert out == [1, 2, 3, 4, 5, 6]

def test_flatten_empty():
    assert list(E("(flatten [])")) == []

def test_flatten_nil():
    assert list(E("(flatten nil)")) == []

def test_flatten_no_nesting():
    assert list(E("(flatten [1 2 3])")) == [1, 2, 3]

def test_flatten_deep_nesting():
    out = list(E("(flatten [[[1] [2]] [[3] [[4]]]])"))
    assert out == [1, 2, 3, 4]

def test_flatten_preserves_non_sequential():
    """Maps and sets aren't sequential? — they pass through as units."""
    out = list(E("(flatten [[1 2] {:a 1} [3]])"))
    # Three elements: 1, 2, the map, 3 (map is non-sequential, kept whole)
    assert len(out) == 4
    assert out[0] == 1
    assert out[1] == 2
    assert out[3] == 3
