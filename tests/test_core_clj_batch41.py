"""Tests for core.clj batch 41: transducer fns + Eduction (JVM 7808-7960).

Forms ported:
  preserving-reduced (private), cat, halt-when, dedupe, random-sample,
  Eduction (deftype), eduction, run!, iteration, plus print-method
  for Eduction.

Adaptations from JVM:
  - Iterable / .iterator → py.collections.abc/Iterable / __iter__.
    Python iter() and `for x in ...` both call __iter__ on the
    class.
  - clojure.lang.RT/iter → (py.__builtins__/iter coll). Python
    builtin iter() over any iterable.
  - Eduction is a deftype (not defrecord) — it doesn't need
    map-like behavior, just iterability + reducibility.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- cat -------------------------------------------------------

def test_cat_basic():
    out = E("(into [] cat [[1 2] [3 4] [5]])")
    assert list(out) == [1, 2, 3, 4, 5]

def test_cat_empty():
    out = E("(into [] cat [[] [] []])")
    assert list(out) == []

def test_cat_with_other_xforms():
    """cat composes with map / filter."""
    out = E("(into [] (comp cat (filter odd?)) [[1 2] [3 4] [5 6]])")
    assert list(out) == [1, 3, 5]

def test_cat_honors_reduced():
    """preserving-reduced should let inner reduces short-circuit
    without prematurely terminating the outer transduction."""
    out = E("""
      (transduce cat
                 (fn ([acc] acc)
                     ([acc x] (if (= x 3) (reduced acc) (conj acc x))))
                 []
                 [[1 2] [3 4] [5]])""")
    assert list(out) == [1, 2]


# --- halt-when -------------------------------------------------

def test_halt_when_no_halt_passes_through():
    out = E("(into [] (halt-when neg?) [1 2 3])")
    assert list(out) == [1, 2, 3]

def test_halt_when_halts_returning_input():
    """No retf → returns the triggering input."""
    out = E("(transduce (halt-when neg?) conj [1 2 -3 4])")
    assert out == -3

def test_halt_when_halts_with_retf():
    """retf called with (rf-completed-result, triggering-input)."""
    out = E("""
      (transduce (halt-when neg? (fn [acc x] [:halted-at x :so-far acc]))
                 conj [1 2 -3 4])""")
    assert list(out) == [K("halted-at"), -3, K("so-far"), [1, 2]]


# --- dedupe ----------------------------------------------------

def test_dedupe_consecutive_only():
    """Note: dedupe removes only CONSECUTIVE dups (1 1 2 1 stays 1 2 1)."""
    out = E("(dedupe [1 1 2 2 2 3 1 1])")
    assert list(out) == [1, 2, 3, 1]

def test_dedupe_empty():
    assert list(E("(dedupe [])")) == []

def test_dedupe_no_dups():
    assert list(E("(dedupe [1 2 3 4])")) == [1, 2, 3, 4]

def test_dedupe_all_same():
    assert list(E("(dedupe [:x :x :x :x])")) == [K("x")]

def test_dedupe_transducer():
    out = E("(into [] (dedupe) [1 1 2 2 3])")
    assert list(out) == [1, 2, 3]


# --- random-sample --------------------------------------------

def test_random_sample_prob_one_keeps_all():
    out = E("(random-sample 1.0 [1 2 3 4 5])")
    assert list(out) == [1, 2, 3, 4, 5]

def test_random_sample_prob_zero_keeps_none():
    out = E("(random-sample 0.0 [1 2 3 4 5])")
    assert list(out) == []

def test_random_sample_transducer_form():
    """1-arity: returns a transducer."""
    out = E("(into [] (random-sample 1.0) [1 2 3])")
    assert list(out) == [1, 2, 3]


# --- Eduction / eduction --------------------------------------

def test_eduction_reduces():
    """eduction's primary surface: pass to reduce."""
    out = E("(reduce + 0 (eduction (map inc) (filter odd?) [1 2 3 4 5]))")
    # [1 2 3 4 5] → map inc → [2 3 4 5 6] → filter odd? → [3 5] → sum 8.
    assert out == 8

def test_eduction_iterates():
    """eduction is iterable — seq works on it."""
    out = E("(seq (eduction (map inc) [1 2 3]))")
    assert list(out) == [2, 3, 4]

def test_eduction_isinstance_iterable():
    """Eduction registers with py.collections.abc/Iterable."""
    assert E("(instance? py.collections.abc/Iterable (eduction (map inc) [1]))") is True

def test_eduction_isinstance_ireduceinit():
    assert E("(instance? clojure.lang.IReduceInit (eduction (map inc) [1]))") is True

def test_eduction_isinstance_sequential():
    assert E("(instance? clojure.lang.Sequential (eduction (map inc) [1]))") is True

def test_eduction_composes_xforms_in_order():
    """xforms applied as if combined with comp — first xform is outermost."""
    out = E("""
      (into [] (eduction (map inc) (map #(* % 10)) [1 2 3]))""")
    # 1 → inc → 2 → *10 → 20
    assert list(out) == [20, 30, 40]

def test_eduction_each_call_replays():
    """Reducing twice replays the same xform application."""
    E("(def -tcb-ed (eduction (map inc) [1 2 3]))")
    a = E("(reduce + 0 -tcb-ed)")
    b = E("(reduce + 0 -tcb-ed)")
    assert a == b == 9

def test_eduction_print_method():
    """Eduction prints as a parenthesized sequence."""
    out = E("(pr-str (eduction (map inc) [1 2 3]))")
    assert out == "(2 3 4)"


# --- run! ------------------------------------------------------

def test_run_bang_calls_for_side_effects():
    out = E("""
      (let [a (atom 0)]
        (run! (fn [x] (swap! a + x)) [1 2 3 4 5])
        (deref a))""")
    assert out == 15

def test_run_bang_returns_nil():
    out = E("(run! identity [1 2 3])")
    assert out is None

def test_run_bang_empty():
    out = E("""
      (let [a (atom :unchanged)]
        (run! (fn [_] (reset! a :should-not-fire)) [])
        (deref a))""")
    assert out == K("unchanged")


# --- iteration ------------------------------------------------

def test_iteration_basic_seq():
    """Basic iteration: keep stepping while < 50, return values."""
    out = E("""
      (take 5 (iteration (fn [n] (when (< n 100) (+ n 10)))
                          :initk 0))""")
    assert list(out) == [10, 20, 30, 40, 50]

def test_iteration_terminates_when_somef_false():
    """iteration stops when (somef ret) is false."""
    out = E("""
      (into [] (iteration (fn [n] (if (< n 5) (inc n) :stop))
                       :somef int?
                       :initk 0))""")
    assert list(out) == [1, 2, 3, 4, 5]

def test_iteration_with_vf():
    """vf transforms ret before yielding."""
    out = E("""
      (into [] (iteration (fn [n] (when (< n 5) (inc n)))
                       :vf (fn [n] [:n n])
                       :initk 0))""")
    assert [list(p) for p in out] == [[K("n"), 1], [K("n"), 2], [K("n"), 3], [K("n"), 4], [K("n"), 5]]

def test_iteration_with_kf_continuation():
    """kf returns the next continuation token (or nil to stop). Here
    step takes a token n and returns {:n n :next (n+1 or nil)}."""
    out = E("""
      (into [] (iteration (fn [n] {:n n :next (when (< n 3) (inc n))})
                          :vf :n
                          :kf :next
                          :initk 1))""")
    # step 1 → {:n 1 :next 2}; vf=:n → 1; kf=:next → 2; recur with 2.
    # step 2 → {:n 2 :next 3}; vf → 2; kf → 3.
    # step 3 → {:n 3 :next nil}; vf → 3; kf → nil → done.
    assert list(out) == [1, 2, 3]

def test_iteration_reduces():
    """iteration's IReduceInit: reduce works directly."""
    out = E("""
      (reduce + 0 (iteration (fn [n] (when (< n 5) (inc n)))
                              :initk 0))""")
    assert out == 15  # 1+2+3+4+5
