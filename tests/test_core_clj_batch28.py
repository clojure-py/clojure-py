"""Tests for core.clj batch 28 (selected from JVM 5005-5300):
miscellaneous utilities.

Forms (13):
  xml-seq, special-symbol?, var?, subs,
  max-key, min-key,
  distinct, replace,
  dosync (macro), repeatedly,
  hash, interpose, empty.

Skipped — saved for focused follow-up batches:
  file-seq                            — needs java.io.File or
                                        Python pathlib equivalent.
  with-precision                      — JVM MathContext / RoundingMode
                                        story; rarely used.
  mk-bound-fn / subseq / rsubseq     — need Sorted protocol surface
                                        on TreeMap/TreeSet.
  add-classpath                      — DEPRECATED in JVM 1.1.
  mix-collection-hash /
  hash-ordered-coll /
  hash-unordered-coll                 — Murmur3 helpers.
  definline                           — uses (eval (list `fn …)) at
                                        macro-expansion time.

Backend additions:
  Compiler.specials = SPECIAL_FORMS — JVM API name alias used by
                                       special-symbol?.
  JAVA_METHOD_FALLBACKS["substring"] — forwards to Python str slicing
                                        when the receiver lacks
                                        .substring (so Python str
                                        works with `(.substring s a b)`).

One adaptation from JVM source:
  special-symbol? uses Compiler/specials (slash form for static
  field) rather than (. Compiler specials). Our compiler treats
  `(. Cls name)` as a zero-arg method call.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentVector,
    PersistentArrayMap,
    PersistentHashSet,
    PersistentList,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- xml-seq ------------------------------------------------------

def test_xml_seq_walks_elements():
    out = list(E('(xml-seq {:tag :a :content [{:tag :b :content ["text"]}]})'))
    # Three nodes: outer :a, inner :b, leaf "text"
    assert len(out) == 3
    assert out[0] == E('{:tag :a :content [{:tag :b :content ["text"]}]}')
    assert out[2] == "text"

def test_xml_seq_leaf_string_only():
    out = list(E('(xml-seq "just text")'))
    assert out == ["just text"]


# --- special-symbol? ---------------------------------------------

def test_special_symbol_for_special_forms():
    assert E("(special-symbol? 'if)") is True
    assert E("(special-symbol? 'do)") is True
    assert E("(special-symbol? 'fn*)") is True
    assert E("(special-symbol? 'let*)") is True
    assert E("(special-symbol? 'try)") is True
    assert E("(special-symbol? 'catch)") is True

def test_special_symbol_false_for_normal_symbols():
    assert E("(special-symbol? 'foo)") is False
    assert E("(special-symbol? '+)") is False
    assert E("(special-symbol? 'when)") is False  # macro, not special


# --- var? --------------------------------------------------------

def test_var_pred_true_for_var():
    assert E("(var? (var +))") is True

def test_var_pred_false_for_value():
    assert E("(var? 42)") is False
    assert E("(var? :keyword)") is False
    assert E("(var? nil)") is False
    assert E("(var? +)") is False


# --- subs --------------------------------------------------------

def test_subs_two_arg():
    assert E('(subs "hello world" 6)') == "world"

def test_subs_three_arg():
    assert E('(subs "hello world" 0 5)') == "hello"

def test_subs_empty_substring():
    assert E('(subs "abc" 1 1)') == ""

def test_subs_full_string():
    assert E('(subs "abc" 0 3)') == "abc"

def test_subs_negative_index_python_semantics():
    """Python str slicing accepts negative indices; we inherit that.
    JVM's String.substring would throw — slight semantic deviation."""
    assert E('(subs "abc" 0 -1)') == "ab"


# --- max-key / min-key -------------------------------------------

def test_max_key_one_arg():
    out = E("(max-key count [1 2 3])")
    assert list(out) == [1, 2, 3]

def test_max_key_two_args():
    out = E("(max-key count [1] [1 2])")
    assert list(out) == [1, 2]

def test_max_key_variadic():
    out = E("(max-key count [1] [1 2 3] [1 2])")
    assert list(out) == [1, 2, 3]

def test_max_key_ties_picks_last():
    """JVM doc: 'If there are multiple such xs, the last one is returned.'"""
    out = E("(max-key count [1 2] [3 4] [5 6])")
    assert list(out) == [5, 6]

def test_min_key_basic():
    out = E("(min-key count [1 2 3] [1] [1 2])")
    assert list(out) == [1]


# --- distinct ----------------------------------------------------

def test_distinct_basic():
    assert list(E("(distinct [1 2 3 1 2 4 3])")) == [1, 2, 3, 4]

def test_distinct_preserves_order():
    """First occurrence wins."""
    assert list(E("(distinct [3 1 2 1 3 2])")) == [3, 1, 2]

def test_distinct_empty():
    assert list(E("(distinct [])")) == []

def test_distinct_transducer():
    assert list(E("(sequence (distinct) [1 2 3 1 2])")) == [1, 2, 3]

def test_distinct_lazy():
    assert list(E("(take 3 (distinct (cycle [1 2 3 1 2 3])))")) == [1, 2, 3]


# --- replace -----------------------------------------------------

def test_replace_in_vector_returns_vector():
    """JVM doc: vector input → vector output (stable indexing)."""
    out = E("(replace {1 :a 2 :b} [1 2 3 1])")
    assert isinstance(out, PersistentVector)
    assert list(out) == [K("a"), K("b"), 3, K("a")]

def test_replace_in_seq_returns_seq():
    out = list(E("(replace {1 :a 2 :b} (list 1 2 3 1))"))
    assert out == [K("a"), K("b"), 3, K("a")]

def test_replace_no_match_passes_through():
    out = list(E("(replace {99 :nope} [1 2 3])"))
    assert out == [1, 2, 3]

def test_replace_transducer():
    out = list(E("(sequence (replace {1 :a 2 :b}) [1 2 3])"))
    assert out == [K("a"), K("b"), 3]


# --- dosync ------------------------------------------------------

def test_dosync_returns_body_value():
    assert E("(dosync 42)") == 42

def test_dosync_with_ref():
    """dosync wraps a real STM transaction — ref-set works."""
    out = E("""(let [r (ref 0)]
                 (dosync (ref-set r 99))
                 @r)""")
    assert out == 99


# --- repeatedly --------------------------------------------------

def test_repeatedly_n_arg():
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb28-tick!"),
               lambda: counter.append(1) or len(counter) - 1)
    out = list(E("(repeatedly 5 user/tcb28-tick!)"))
    assert out == [1, 2, 3, 4, 5]

def test_repeatedly_lazy_infinite():
    """No-arg form is infinite — take-from-it should still terminate."""
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb28-bump!"),
               lambda: counter.append(1) or len(counter) - 1)
    out = list(E("(take 3 (repeatedly user/tcb28-bump!))"))
    assert out == [1, 2, 3]


# --- hash --------------------------------------------------------

def test_hash_int():
    """hash returns an int."""
    assert isinstance(E("(hash 42)"), int)

def test_hash_consistent_with_equality():
    """Equal values have equal hashes."""
    assert E("(= (hash 42) (hash 42))")
    assert E("(= (hash :a) (hash :a))")
    assert E("(= (hash [1 2 3]) (hash [1 2 3]))")
    assert E('(= (hash "abc") (hash "abc"))')

def test_hash_nil_is_zero():
    assert E("(hash nil)") == 0


# --- interpose ---------------------------------------------------

def test_interpose_basic():
    out = list(E("(interpose :sep [1 2 3])"))
    assert out == [1, K("sep"), 2, K("sep"), 3]

def test_interpose_empty():
    assert list(E("(interpose :sep [])")) == []

def test_interpose_singleton():
    """No separator if there's only one element."""
    assert list(E("(interpose :sep [42])")) == [42]

def test_interpose_transducer():
    out = list(E("(sequence (interpose :sep) [1 2 3])"))
    assert out == [1, K("sep"), 2, K("sep"), 3]

def test_interpose_with_strings():
    out = list(E('(interpose ", " ["a" "b" "c"])'))
    assert out == ["a", ", ", "b", ", ", "c"]


# --- empty -------------------------------------------------------

def test_empty_vector():
    out = E("(empty [1 2 3])")
    assert isinstance(out, PersistentVector)
    assert list(out) == []

def test_empty_map():
    out = E("(empty {:a 1 :b 2})")
    assert dict(out) == {}

def test_empty_set():
    out = E("(empty #{1 2 3})")
    assert isinstance(out, PersistentHashSet)
    assert set(out) == set()

def test_empty_list():
    out = E("(empty (list 1 2 3))")
    assert list(out) == []

def test_empty_nil_returns_nil():
    """JVM: empty on a non-IPersistentCollection returns nil."""
    assert E("(empty nil)") is None

def test_empty_string_returns_nil():
    """String isn't IPersistentCollection in our world (Python str is just str)."""
    assert E('(empty "abc")') is None
