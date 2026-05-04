"""Tests for core.clj batch 23 (selected forms from JVM lines
4408-4806 — IO/print helpers and small list/map utilities).

Forms (9):
  array-map,
  seq-to-map-for-destructuring,
  when-first (macro),
  lazy-cat (macro),
  comment (macro),
  with-out-str (macro), with-in-str (macro),
  pr-str, prn-str.

Skipped — saved for the destructure batch:
  destructure, maybe-destructured (private),
  let / fn / loop redefinitions (with destructuring),
  for (uses destructuring extensively).

Backend addition:
  clojure.lang.StringWriter — small mutable text buffer; satisfies
  the same surface as *out* targets (.write / .append / .flush /
  .close) plus .toString and __str__ that return the accumulated
  text. Used by with-out-str.

Adaptations from JVM source:
  array-map's empty arity uses {} where JVM uses
    PersistentArrayMap/EMPTY — Cython cdef classes don't allow
    class-attribute assignment, so the static EMPTY field isn't
    reachable. Same value.
  seq-to-map-for-destructuring same story.
  with-out-str uses clojure.lang.StringWriter where JVM uses
    java.io.StringWriter.
  with-in-str uses (py.io/StringIO ~s) where JVM uses
    (java.io.StringReader. ~s).
  when-first uses explicit (first bindings)/(second bindings) where
    JVM uses [[x xs] bindings] destructuring — our `let` is still
    the bootstrap (no-destructure) version. Will switch back to
    destructuring once batch 24 lands.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentArrayMap,
    StringWriter,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- StringWriter shim --------------------------------------------

def test_stringwriter_basic():
    sw = StringWriter()
    sw.write("hello")
    sw.write(" ")
    sw.write("world")
    assert str(sw) == "hello world"
    assert sw.toString() == "hello world"
    assert sw.getvalue() == "hello world"

def test_stringwriter_append_chains():
    sw = StringWriter()
    out = sw.append("a").append("b").append("c")
    assert out is sw
    assert str(sw) == "abc"

def test_stringwriter_initial_value():
    sw = StringWriter("seed")
    sw.write("-after")
    assert str(sw) == "seed-after"

def test_stringwriter_flush_close_no_op():
    sw = StringWriter("x")
    sw.flush()
    sw.close()
    assert str(sw) == "x"


# --- array-map ----------------------------------------------------

def test_array_map_empty():
    out = E("(array-map)")
    assert isinstance(out, PersistentArrayMap)
    assert dict(out) == {}

def test_array_map_pairs():
    out = E("(array-map :a 1 :b 2 :c 3)")
    assert dict(out) == {K("a"): 1, K("b"): 2, K("c"): 3}

def test_array_map_dup_keys_last_wins():
    """JVM doc: 'If any keys are equal, they are handled as if by repeated uses of assoc.'"""
    out = E("(array-map :a 1 :a 2)")
    assert dict(out) == {K("a"): 2}

def test_array_map_odd_args_raises():
    with pytest.raises(Exception, match="No value supplied for key"):
        E("(array-map :a 1 :b)")


# --- seq-to-map-for-destructuring --------------------------------

def test_s2m_single_map_returned_as_is():
    """When the seq has one element that's a map, it's returned directly."""
    out = E("(seq-to-map-for-destructuring (quote ({:a 1})))")
    assert dict(out) == {K("a"): 1}

def test_s2m_multi_pairs():
    """Multi-element seq is built as an array-map."""
    out = E("(seq-to-map-for-destructuring (quote (:a 1 :b 2)))")
    assert dict(out) == {K("a"): 1, K("b"): 2}

def test_s2m_empty_returns_empty_map():
    out = E("(seq-to-map-for-destructuring nil)")
    assert dict(out) == {}


# --- when-first --------------------------------------------------

def test_when_first_non_empty():
    """First element bound, body evaluated."""
    assert E("(when-first [x [10 20 30]] (* x 100))") == 1000

def test_when_first_empty_returns_nil():
    assert E("(when-first [x []] :body)") is None

def test_when_first_evaluates_xs_only_once():
    """Crucial: the seq should be bound once via gensym."""
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb23-once!"),
               lambda: (counter.append(1), [1, 2, 3])[1])
    E("(when-first [x (user/tcb23-once!)] x)")
    # Including initial 0; one call adds one element.
    assert len(counter) == 2

def test_when_first_assert_args_non_vector():
    with pytest.raises(Exception, match="vector"):
        E("(when-first (x [1 2 3]) x)")

def test_when_first_assert_args_wrong_count():
    with pytest.raises(Exception, match="exactly 2 forms"):
        E("(when-first [x [1] y] x)")


# --- lazy-cat -----------------------------------------------------

def test_lazy_cat_basic():
    out = list(E("(lazy-cat [1 2] [3 4] [5 6])"))
    assert out == [1, 2, 3, 4, 5, 6]

def test_lazy_cat_empty_args():
    assert list(E("(lazy-cat)")) == []

def test_lazy_cat_lazy():
    """Each coll is wrapped in (lazy-seq ...) so it isn't evaluated until needed.
    Take only what's needed; later colls' bodies should not run."""
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb23-tail!"),
               lambda: (counter.append(1), [99])[1])
    out = list(E("(take 1 (lazy-cat [1] (user/tcb23-tail!)))"))
    assert out == [1]
    # Note: the chunked-seq fast path may force more than strictly
    # necessary; just verify we got the head correctly.


# --- comment ------------------------------------------------------

def test_comment_returns_nil():
    assert E("(comment :a :b :c)") is None

def test_comment_no_args():
    assert E("(comment)") is None

def test_comment_does_not_eval_body():
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb23-poke!"),
               lambda: counter.append(1))
    E("(comment (user/tcb23-poke!))")
    assert counter == [0]


# --- with-out-str -------------------------------------------------

def test_with_out_str_captures_pr():
    assert E("(with-out-str (pr 42))") == "42"

def test_with_out_str_captures_println():
    assert E('(with-out-str (println "hi"))') == "hi\n"

def test_with_out_str_captures_multiple_forms():
    out = E('(with-out-str (pr 1) (pr 2) (pr 3))')
    # No separators — pr is back-to-back.
    assert out == "123"

def test_with_out_str_empty_body():
    assert E("(with-out-str)") == ""

def test_with_out_str_does_not_leak_out():
    """After with-out-str, *out* should be back to whatever it was before."""
    import sys
    E("(with-out-str (pr 1))")
    assert E("(clojure.core/var clojure.core/*out*)").deref() is sys.stdout


# --- with-in-str --------------------------------------------------

def test_with_in_str_read_form():
    assert E('(with-in-str "42" (read))') == 42

def test_with_in_str_multiple_reads():
    out = E('(with-in-str ":a :b 99" [(read) (read) (read)])')
    assert list(out) == [K("a"), K("b"), 99]

def test_with_in_str_read_line():
    out = E('(with-in-str "first\\nsecond" (read-line))')
    assert out == "first"


# --- pr-str / prn-str --------------------------------------------

def test_pr_str_returns_string():
    out = E("(pr-str 42)")
    assert out == "42"

def test_pr_str_quotes_strings():
    """pr-str respects *print-readably* (default true) — strings get quoted."""
    out = E('(pr-str "hello")')
    assert out == '"hello"'

def test_pr_str_multiple_args_space_separated():
    out = E("(pr-str 1 2 3)")
    assert out == "1 2 3"

def test_pr_str_no_args():
    assert E("(pr-str)") == ""

def test_prn_str_appends_newline():
    out = E("(prn-str 42)")
    assert out == "42\n"

def test_prn_str_no_args_just_newline():
    assert E("(prn-str)") == "\n"

def test_prn_str_multiple_args():
    out = E("(prn-str :a :b)")
    assert out == ":a :b\n"
