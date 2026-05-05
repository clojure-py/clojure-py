"""Tests for core.clj batch 42: readers + tap (JVM 7961-8160).

Forms ported:
  tagged-literal? / tagged-literal,
  reader-conditional? / reader-conditional,
  default-data-readers, *data-readers*, *default-data-reader-fn*,
  uri?,
  add-tap / remove-tap / tap>, plus the tapset / tapq / tap-loop
  defonces.

Adaptations from JVM:
  - clojure.lang.TaggedLiteral and clojure.lang.ReaderConditional
    already exist as Cython classes in lisp_reader.pxi (the reader
    produces them). We expose the tagged-literal / reader-conditional
    fns + their predicates.
  - JVM's classloader scan for data_readers.clj / data_readers.cljc
    is omitted. *data-readers* defaults to {}, default-data-readers
    is empty for now (we'll add 'uuid / 'inst entries when we port
    clojure.uuid / clojure.instant).
  - uri? checks for urllib.parse.ParseResult instead of java.net.URI.
    Closest Python analog — what urllib.parse.urlparse returns.
  - JVM ArrayBlockingQueue → queue.Queue with maxsize=1024.
    .offer(non-blocking, returns bool) → .put_nowait (raises
    queue.Full); we wrap in try/catch.
  - JVM Thread.setDaemon(true) → setattr daemon True. Thread
    constructor takes (group, target, name) positionally — we pass
    nil for group.
"""

import time as _time
import urllib.parse as _urlparse
import queue as _queue

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword, Symbol, Var, TaggedLiteral, ReaderConditional,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- tagged-literal / tagged-literal? --------------------------

def test_tagged_literal_constructor():
    tl = E("(tagged-literal (quote foo/bar) [1 2 3])")
    assert isinstance(tl, TaggedLiteral)
    assert tl.tag == Symbol.intern("foo", "bar")

def test_tagged_literal_pred_true():
    out = E("(tagged-literal? (tagged-literal (quote x) :y))")
    assert out is True

def test_tagged_literal_pred_false():
    assert E("(tagged-literal? 42)") is False
    assert E("(tagged-literal? nil)") is False
    assert E('(tagged-literal? "abc")') is False
    assert E("(tagged-literal? [1 2])") is False

def test_tagged_literal_lookup_tag():
    """Tag is accessible via :tag keyword."""
    out = E("(:tag (tagged-literal (quote my-tag) [10 20]))")
    assert out == Symbol.intern("my-tag")

def test_tagged_literal_lookup_form():
    """Form is accessible via :form keyword."""
    out = E("(:form (tagged-literal (quote my-tag) [:a :b]))")
    assert list(out) == [K("a"), K("b")]

def test_tagged_literal_equality():
    """Two tagged-literals with same tag+form are equal."""
    out = E("""
      (= (tagged-literal (quote foo) [1 2])
         (tagged-literal (quote foo) [1 2]))""")
    assert out is True

def test_tagged_literal_inequality():
    out = E("""
      (= (tagged-literal (quote foo) [1 2])
         (tagged-literal (quote bar) [1 2]))""")
    assert out is False


# --- reader-conditional / reader-conditional? ----------------

def test_reader_conditional_constructor():
    rc = E("(reader-conditional (quote (:default 1 :clj 2)) false)")
    assert isinstance(rc, ReaderConditional)
    assert rc.splicing is False

def test_reader_conditional_splicing():
    rc = E("(reader-conditional (quote (:default 1)) true)")
    assert rc.splicing is True

def test_reader_conditional_pred_true():
    out = E("(reader-conditional? (reader-conditional (quote ()) false))")
    assert out is True

def test_reader_conditional_pred_false():
    assert E("(reader-conditional? 42)") is False
    assert E("(reader-conditional? nil)") is False

def test_reader_conditional_lookup_form():
    out = E("(:form (reader-conditional (quote (:clj 1)) false))")
    assert list(out) == [K("clj"), 1]

def test_reader_conditional_lookup_splicing():
    assert E("(:splicing? (reader-conditional () false))") is False
    assert E("(:splicing? (reader-conditional () true))") is True

def test_reader_conditional_equality():
    out = E("""
      (= (reader-conditional (quote (:clj 1)) false)
         (reader-conditional (quote (:clj 1)) false))""")
    assert out is True

def test_reader_conditional_splicing_matters_for_eq():
    out = E("""
      (= (reader-conditional () true)
         (reader-conditional () false))""")
    assert out is False


# --- *data-readers* / *default-data-reader-fn* ---------------

def test_default_data_readers_is_map():
    """default-data-readers is an empty map for now (JVM has 'uuid /
    'inst entries that depend on clojure.uuid / clojure.instant)."""
    out = E("default-data-readers")
    assert hasattr(out, "count")
    assert out.count() == 0

def test_star_data_readers_default_empty():
    out = E("*data-readers*")
    assert hasattr(out, "count")
    assert out.count() == 0

def test_star_data_readers_dynamic():
    """*data-readers* is bindable per-thread."""
    out = E("""
      (binding [*data-readers* {(quote foo) :marker}]
        (get *data-readers* (quote foo)))""")
    assert out == K("marker")

def test_star_default_data_reader_fn_default_nil():
    assert E("*default-data-reader-fn*") is None

def test_star_default_data_reader_fn_dynamic():
    out = E("""
      (binding [*default-data-reader-fn* (fn [tag form] [:tagged tag form])]
        (*default-data-reader-fn* (quote myt) [1 2]))""")
    assert list(out) == [K("tagged"), Symbol.intern("myt"), [1, 2]]


# --- uri? ----------------------------------------------------

def test_uri_pred_true_for_parse_result():
    parsed = _urlparse.urlparse("https://example.com/path?q=1")
    Var.intern(Compiler.current_ns(),
               Symbol.intern("-tcb42-u1"),
               parsed)
    assert E("(uri? -tcb42-u1)") is True

def test_uri_pred_false_for_string():
    assert E('(uri? "https://example.com")') is False

def test_uri_pred_false_for_other():
    assert E("(uri? 42)") is False
    assert E("(uri? nil)") is False
    assert E("(uri? [:not :a :uri])") is False


# --- tap -----------------------------------------------------

def _wait_for_taps(ns_var_name, expected_count, timeout=0.5):
    """Spin up to `timeout` seconds for the tap loop to drain."""
    deadline = _time.monotonic() + timeout
    while _time.monotonic() < deadline:
        out = E(f"@{ns_var_name}")
        if out.count() >= expected_count:
            return out
        _time.sleep(0.005)
    return E(f"@{ns_var_name}")


def test_add_tap_and_send():
    """Register a tap, send values, verify the loop delivers them."""
    E("(def -tcb42-results (atom []))")
    E("""(def -tcb42-tap-fn
           (fn [x] (swap! -tcb42-results conj x)))""")
    E("(add-tap -tcb42-tap-fn)")
    try:
        E("(tap> :one)")
        E("(tap> :two)")
        E("(tap> 42)")
        out = _wait_for_taps("-tcb42-results", 3)
        assert list(out) == [K("one"), K("two"), 42]
    finally:
        E("(remove-tap -tcb42-tap-fn)")

def test_tap_passes_nil_through():
    """tap> wraps nil in a sentinel so the queue can pass it; the loop
    unwraps it back to nil before invoking taps."""
    E("(def -tcb42-results2 (atom []))")
    E("""(def -tcb42-tap-fn2
           (fn [x] (swap! -tcb42-results2 conj [:got x])))""")
    E("(add-tap -tcb42-tap-fn2)")
    try:
        E("(tap> nil)")
        out = _wait_for_taps("-tcb42-results2", 1)
        first = list(list(out)[0])
        assert first == [K("got"), None]
    finally:
        E("(remove-tap -tcb42-tap-fn2)")

def test_remove_tap_stops_delivery():
    """After remove-tap, the fn no longer receives values."""
    E("(def -tcb42-results3 (atom []))")
    E("""(def -tcb42-tap-fn3
           (fn [x] (swap! -tcb42-results3 conj x)))""")
    E("(add-tap -tcb42-tap-fn3)")
    E("(tap> :before-remove)")
    _wait_for_taps("-tcb42-results3", 1)
    E("(remove-tap -tcb42-tap-fn3)")
    E("(tap> :after-remove)")
    _time.sleep(0.05)
    out = list(E("@-tcb42-results3"))
    assert out == [K("before-remove")]

def test_tap_returns_true_on_success():
    """tap> returns true when there's room in the queue."""
    out = E("(tap> :anything)")
    assert out is True

def test_failing_tap_doesnt_break_others():
    """If one tap raises, others still run (the loop catches Throwable)."""
    E("(def -tcb42-results4 (atom []))")
    E("""(def -tcb42-bad-tap (fn [_] (throw (RuntimeException. "boom"))))""")
    E("""(def -tcb42-good-tap (fn [x] (swap! -tcb42-results4 conj x)))""")
    E("(add-tap -tcb42-bad-tap)")
    E("(add-tap -tcb42-good-tap)")
    try:
        E("(tap> :survives)")
        out = _wait_for_taps("-tcb42-results4", 1)
        assert list(out) == [K("survives")]
    finally:
        E("(remove-tap -tcb42-bad-tap)")
        E("(remove-tap -tcb42-good-tap)")
