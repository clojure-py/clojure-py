"""Tests for src/clojure/core/protocols.clj — the small protocol-only
file in the clojure.core.protocols namespace. Defines CollReduce,
InternalReduce, IKVReduce, Datafiable, Navigable.

Adaptations from JVM:
  - JVM Iterator API → Python iter()/next() with sentinel.
  - Skipped extensions for clojure.lang.StringSeq, KeySeq, ValSeq,
    Iterable. The Object branch (which calls seq-reduce →
    internal-reduce) handles them via the existing IteratorSeq path.

What this batch unlocks:
  - The next batch (real `reduce` redefinition + `transduce` / `into` /
    `mapv` / `filterv`) builds on coll-reduce.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword, Namespace, Symbol,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


def CR(src):
    """Eval inside a (clojure.core.protocols/coll-reduce ...) call."""
    return E(f"(clojure.core.protocols/coll-reduce {src})")


# --- coll-reduce: nil ------------------------------------------

def test_coll_reduce_nil_no_init_calls_f():
    """(coll-reduce nil f) calls (f) with no args — same as (reduce f nil)."""
    out = E("(clojure.core.protocols/coll-reduce nil (fn ([] :empty) ([a b] [a b])))")
    assert out == K("empty")

def test_coll_reduce_nil_with_init_returns_init():
    out = CR("nil + 99")
    assert out == 99


# --- coll-reduce: vectors / lists ------------------------------

def test_coll_reduce_vector_with_init():
    assert CR("[1 2 3 4] + 0") == 10

def test_coll_reduce_vector_no_init():
    assert CR("[1 2 3 4] +") == 10

def test_coll_reduce_empty_vector_no_init():
    assert E("(clojure.core.protocols/coll-reduce [] (fn ([] :z) ([a b] (+ a b))))") == K("z")

def test_coll_reduce_list():
    assert CR("(quote (1 2 3)) + 0") == 6

def test_coll_reduce_lazy_seq():
    assert CR("(map inc [10 20 30]) + 0") == 63

def test_coll_reduce_range():
    assert CR("(range 5) + 0") == 10


# --- coll-reduce: maps and sets --------------------------------

def test_coll_reduce_map_iterates_entries():
    """Map iteration yields MapEntry pairs. Use clojure.core/val to
    extract the value (qualified to avoid name clashes with any
    protocol method also named `val` defined in earlier tests)."""
    out = E("""
      (clojure.core.protocols/coll-reduce
        (sorted-map :a 1 :b 2 :c 3)
        (fn [acc kv] (+ acc (clojure.core/val kv)))
        0)""")
    assert out == 6

def test_coll_reduce_set():
    """Sets aren't ordered but sum should still be 6."""
    out = E("""
      (clojure.core.protocols/coll-reduce
        #{1 2 3} + 0)""")
    assert out == 6


# --- coll-reduce: reduced short-circuits -----------------------

def test_coll_reduce_honors_reduced():
    out = E("""
      (clojure.core.protocols/coll-reduce
        [1 2 3 4 5]
        (fn [_ x] (if (> x 3) (reduced x) x))
        0)""")
    assert out == 4

def test_coll_reduce_reduced_unwrapped():
    out = E("""
      (clojure.core.protocols/coll-reduce
        [10 20 30]
        (fn [_ x] (reduced [:final x]))
        0)""")
    assert list(out) == [K("final"), 10]


# --- internal-reduce -------------------------------------------

def test_internal_reduce_nil():
    out = E("(clojure.core.protocols/internal-reduce nil + 99)")
    assert out == 99

def test_internal_reduce_chunked_seq():
    out = E("(clojure.core.protocols/internal-reduce (seq [1 2 3 4 5]) + 0)")
    assert out == 15

def test_internal_reduce_lazy_seq():
    out = E("(clojure.core.protocols/internal-reduce (map inc [1 2 3]) + 0)")
    assert out == 9


# --- iterator-reduce! ------------------------------------------

def test_iterator_reduce_basic():
    out = E("""
      (clojure.core.protocols/iterator-reduce!
        (py.__builtins__/iter [1 2 3 4])
        + 0)""")
    assert out == 10

def test_iterator_reduce_no_init_calls_f_on_empty():
    out = E("""
      (clojure.core.protocols/iterator-reduce!
        (py.__builtins__/iter [])
        (fn ([] :empty) ([a b] [a b])))""")
    assert out == K("empty")

def test_iterator_reduce_no_init_uses_first():
    out = E("""
      (clojure.core.protocols/iterator-reduce!
        (py.__builtins__/iter [10 20 30])
        +)""")
    assert out == 60

def test_iterator_reduce_honors_reduced():
    out = E("""
      (clojure.core.protocols/iterator-reduce!
        (py.__builtins__/iter [1 2 3 4 5])
        (fn [acc x] (if (> x 2) (reduced acc) (+ acc x)))
        0)""")
    assert out == 3

def test_iterator_reduce_consumes_iterator():
    """The iterator is mutated as we walk it; subsequent next() raises."""
    E("""
      (def -tcb-it (py.__builtins__/iter [1 2 3]))""")
    out = E("""
      (clojure.core.protocols/iterator-reduce! -tcb-it + 0)""")
    assert out == 6
    # Now the iterator is exhausted.
    with pytest.raises(Exception):
        E("(py.__builtins__/next -tcb-it)")


# --- Datafiable / Navigable -----------------------------------

def test_datafy_default_identity():
    assert E("(clojure.core.protocols/datafy 42)") == 42
    assert E('(clojure.core.protocols/datafy "hello")') == "hello"
    assert E("(clojure.core.protocols/datafy nil)") is None

def test_datafy_custom_via_meta():
    """Datafiable is :extend-via-metadata; meta-fn wins."""
    out = E("""
      (clojure.core.protocols/datafy
        (with-meta {:secret 42}
          {:clojure.core.protocols/datafy
           (fn [m] [:datafied (:secret m)])}))""")
    assert list(out) == [K("datafied"), 42]

def test_nav_default_identity_for_value():
    assert E("(clojure.core.protocols/nav [1 2 3] 0 :ignored-coll-key)") == K("ignored-coll-key")

def test_nav_custom_via_meta():
    """Navigable is :extend-via-metadata."""
    out = E("""
      (clojure.core.protocols/nav
        (with-meta [1 2 3]
          {:clojure.core.protocols/nav
           (fn [coll k v] [:navved k v])})
        0
        :the-val)""")
    assert list(out) == [K("navved"), 0, K("the-val")]


# --- IKVReduce protocol var exists ----------------------------

def test_kv_reduce_protocol_var_exists():
    """IKVReduce is declared but not extended in this file (extension
    happens in core.clj's reduce-redef batch). Protocol var should
    still be a defined map."""
    val = E("clojure.core.protocols/IKVReduce")
    assert val[K("name")] == Symbol.intern("IKVReduce")
    sigs = val[K("sigs")]
    assert K("kv-reduce") in dict(sigs)


# --- coll-reduce dispatches via class hierarchy ---------------

def test_coll_reduce_walks_class_hierarchy():
    """A Clojure-level extension over Number should match int."""
    E("""
      (defprotocol -TCB-CR-Test (cr-pass [x]))""")
    E("(extend-protocol -TCB-CR-Test Number (cr-pass [n] (* n 2)))")
    assert E("(cr-pass 5)") == 10
    assert E("(cr-pass 1.5)") == 3.0
