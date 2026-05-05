"""Tests for core.clj batch 35: case + case* (JVM 6678-6853).

Compiler addition:
  _compile_case_star is a new special-form handler. It wraps each
  `then` and the `default` in (fn* [] body) thunks, then emits a
  call to a runtime helper that:
    1. Computes the bucket key from the value (raw int for :int
       mode, Util.hash otherwise) optionally shifted/masked.
    2. Looks up the bucket in a runtime-built dict.
    3. Skips the post-switch equality check for buckets in
       skip-check (those buckets' thens dispatch internally via a
       condp built by merge-hash-collisions).
    4. Otherwise compares value to the test using = (or `is` for
       :hash-identity mode).

  We don't honor switch-type (:compact / :sparse) — that's a JVM
  tableswitch-vs-lookupswitch optimization, irrelevant when the
  Python target is dict.get (already O(1)).

Macro additions in core.clj:
  shift-mask, max-mask-bits, max-switch-table-size,
  case-int32-min, case-int32-max  (Python ints have no MIN/MAX,
                                   so we hardcode 32-bit signed
                                   bounds — same range as JVM uses
                                   to gate :int dispatch),
  maybe-min-hash, case-map, fits-table?, prep-ints,
  merge-hash-collisions, prep-hashes, case macro.
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


# --- :ints mode -------------------------------------------------

def test_case_int_hit_first():
    assert E("(case 1 1 :one 2 :two 3 :three :default)") == K("one")

def test_case_int_hit_middle():
    assert E("(case 2 1 :one 2 :two 3 :three :default)") == K("two")

def test_case_int_hit_last():
    assert E("(case 3 1 :one 2 :two 3 :three :default)") == K("three")

def test_case_int_default():
    assert E("(case 99 1 :one 2 :two 3 :three :default)") == K("default")

def test_case_int_negatives():
    assert E("(case -5 -10 :neg10 -5 :neg5 0 :zero :default)") == K("neg5")

def test_case_int_sparse():
    """Sparse range — forces shift/mask path."""
    out = E("(case 1000000 1 :one 1000000 :million 999999 :almost :default)")
    assert out == K("million")


# --- :identity mode (all keywords) ------------------------------

def test_case_keyword_hit():
    assert E("(case :b :a 1 :b 2 :c 3 :default)") == 2

def test_case_keyword_default():
    assert E("(case :z :a 1 :b 2 :c 3 :default)") == K("default")

def test_case_keyword_with_namespace():
    out = E("(case :foo/bar :foo/bar :ok :other 1 :default)")
    assert out == K("ok")


# --- :hashes mode (mixed types) ---------------------------------

def test_case_string_hit():
    assert E('(case "hi" "hi" 1 "bye" 2 :default)') == 1

def test_case_string_default():
    assert E('(case "nope" "hi" 1 "bye" 2 :default)') == K("default")

def test_case_symbol_hit():
    """Symbols as test constants — must be quoted in result for the macro
    to see them as literals."""
    out = E("(case 'foo foo :got-foo bar :got-bar :default)")
    assert out == K("got-foo")

def test_case_mixed_types():
    """Mix of strings/numbers triggers :hashes mode."""
    assert E('(case 7 "a" 1 7 :seven "b" 2 :default)') == K("seven")
    assert E('(case "a" "a" 1 7 :seven "b" 2 :default)') == 1


# --- grouped test constants -------------------------------------

def test_case_grouped_first():
    assert E("(case 2 (1 2 3) :small (4 5 6) :mid :default)") == K("small")

def test_case_grouped_middle():
    assert E("(case 5 (1 2 3) :small (4 5 6) :mid (7 8 9) :big :default)") == K("mid")

def test_case_grouped_keywords():
    out = E("(case :y (:x :y :z) :late (:a :b :c) :early :default)")
    assert out == K("late")

def test_case_grouped_miss():
    assert E("(case 99 (1 2 3) :small (4 5 6) :mid :default)") == K("default")


# --- default behavior --------------------------------------------

def test_case_no_default_throws():
    with pytest.raises(Exception, match="No matching clause"):
        E("(case 99 1 :one 2 :two)")

def test_case_no_default_throws_includes_value():
    with pytest.raises(Exception, match="123"):
        E("(case 123 1 :one 2 :two)")

def test_case_default_only():
    """Single trailing default expression with no clauses."""
    assert E("(case 99 :only-default)") == K("only-default")

def test_case_default_only_evaluated():
    """The single-default form still evaluates the expression."""
    assert E("(case (+ 1 2) :hardcoded)") == K("hardcoded")


# --- duplicate detection -----------------------------------------

def test_case_duplicate_test_throws():
    with pytest.raises(Exception, match="Duplicate case test"):
        E("(case 1 1 :a 1 :b)")

def test_case_duplicate_in_group_throws():
    with pytest.raises(Exception, match="Duplicate case test"):
        E("(case 1 (1 2) :first (3 1) :second :default)")


# --- expression evaluation semantics ----------------------------

def test_case_expr_evaluated_once():
    """The dispatch expression is evaluated exactly once even if many
    clauses present."""
    counter = [0]
    import clojure.lang as cl
    cl.Var.intern(Compiler.current_ns(),
                  cl.Symbol.intern("tcb35-bump!"),
                  lambda: (counter.append(1), len(counter))[1])
    E("(case (user/tcb35-bump!) 1 :one 2 :two 3 :three :default)")
    assert sum(counter) == 1

def test_case_then_lazy():
    """Only the matching `then` (or default) is evaluated."""
    side = []
    import clojure.lang as cl
    cl.Var.intern(Compiler.current_ns(),
                  cl.Symbol.intern("tcb35-side!"),
                  lambda x: (side.append(x), x)[1])
    E("(case 2 1 (user/tcb35-side! :a) 2 (user/tcb35-side! :b) 3 (user/tcb35-side! :c) :d)")
    assert side == [K("b")]


# --- closure capture -------------------------------------------

def test_case_then_captures_outer_let():
    """The then-thunk must close over outer locals (the `factor` here)."""
    out = E("""
      (let [factor 100]
        (case 2 1 (* 1 factor) 2 (* 2 factor) :default))""")
    assert out == 200

def test_case_default_captures_outer_let():
    out = E("""
      (let [base 1000]
        (case 99 1 :one 2 :two (+ base 1)))""")
    assert out == 1001


# --- hash-collision handling ------------------------------------

def test_case_handles_hash_collisions():
    """Construct two strings whose Util.hash collides (or fall back to
    naive picks). The compiler will route through merge-hash-collisions
    if any hashes match. We just verify behavior is correct regardless."""
    # Try a bunch of strings; the macro picks :hashes mode for non-int /
    # non-all-keyword tests. With a few strings it'll typically not
    # collide, but the path is exercised.
    out = E('(case "Aa" "Aa" :a "BB" :b "Cc" :c :default)')
    assert out == K("a")
    out = E('(case "BB" "Aa" :a "BB" :b "Cc" :c :default)')
    assert out == K("b")
    out = E('(case "nope" "Aa" :a "BB" :b "Cc" :c :default)')
    assert out == K("default")


# --- nested case ------------------------------------------------

def test_case_nested():
    out = E("""
      (case 1
        1 (case :y :x :outer1-x :y :outer1-y :default)
        2 :outer2
        :default)""")
    assert out == K("outer1-y")
