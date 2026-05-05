"""Tests for core.clj batch 40: predicate combinators + threading +
with-redefs (JVM 7592-7806).

Forms ported:
  every-pred, some-fn, assert-valid-fdecl, with-redefs-fn,
  with-redefs, cond->, cond->>, as->, some->, some->>.

Adaptations from JVM:
  - .bindRoot / .getRawRoot on Var → snake-case .bind_root /
    .get_raw_root in with-redefs-fn (no behavioral change).
  - realized? was already redefined in batch 37 with broader logic
    (handles both clojure.lang.IPending and
    concurrent.futures.Future), so we skip the JVM redef of it here.
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


# --- every-pred -----------------------------------------------

def test_every_pred_single_pred_all_match():
    assert E("((every-pred pos?) 1 2 3)") is True

def test_every_pred_single_pred_one_misses():
    assert E("((every-pred pos?) 1 -1 3)") is False

def test_every_pred_zero_args_returns_true():
    """(f) with no args returns true — vacuous truth."""
    assert E("((every-pred pos?))") is True

def test_every_pred_two_preds():
    assert E("((every-pred pos? even?) 2 4 6)") is True
    assert E("((every-pred pos? even?) 2 4 5)") is False

def test_every_pred_three_preds():
    assert E("((every-pred pos? even? #(< % 100)) 2 4 6)") is True

def test_every_pred_four_plus_preds():
    """4+ preds dispatch to the variadic spn arity."""
    out = E("((every-pred pos? int? even? #(< % 1000)) 2 4 6 8)")
    assert out is True
    out = E("((every-pred pos? int? even? #(< % 1000)) 2 4 6 1001)")
    assert out is False

def test_every_pred_short_circuits():
    """Once one arg fails, remaining args / preds aren't evaluated."""
    counter = [0]
    import clojure.lang as cl
    def bump(x):
        counter[0] += 1
        return True
    cl.Var.intern(Compiler.current_ns(),
                  cl.Symbol.intern("-tcb40-bump"),
                  bump)
    E("((every-pred pos? -tcb40-bump) -1 2 3)")
    # First pred (pos?) sees -1 → false. -tcb40-bump shouldn't fire.
    assert counter[0] == 0


# --- some-fn ---------------------------------------------------

def test_some_fn_single_pred_match():
    """some-fn returns the truthy value, not always boolean."""
    out = E("((some-fn :a) {:a 42})")
    assert out == 42

def test_some_fn_single_pred_no_match():
    """No match → nil from `(some)` or last falsy value."""
    out = E("((some-fn :missing) {:a 42})")
    assert out is None

def test_some_fn_zero_args_returns_nil():
    assert E("((some-fn pos?))") is None

def test_some_fn_two_preds():
    out = E("((some-fn neg? zero?) 1 2 -3)")
    assert out is True

def test_some_fn_three_preds():
    """Returns first truthy value found scanning args × preds."""
    out = E("((some-fn :a :b :c) {:b 99})")
    assert out == 99

def test_some_fn_four_plus_preds():
    out = E("((some-fn neg? zero? #(> % 1000) #(= % :marker)) 1 2 1500 4)")
    assert out is True


# --- with-redefs / with-redefs-fn -----------------------------

def test_with_redefs_replaces_root():
    E("(def -tcb40-target (fn [x] [:orig x]))")
    out = E("(with-redefs [-tcb40-target (fn [x] [:mocked x])] (-tcb40-target 5))")
    assert list(out) == [K("mocked"), 5]

def test_with_redefs_restores_after_body():
    E("(def -tcb40-target2 (fn [] :orig))")
    E("(with-redefs [-tcb40-target2 (fn [] :mock)] :unused)")
    assert E("(-tcb40-target2)") == K("orig")

def test_with_redefs_restores_on_exception():
    """Even if the body throws, the original root is restored."""
    E("(def -tcb40-target3 (fn [] :orig))")
    with pytest.raises(Exception):
        E("""
          (with-redefs [-tcb40-target3 (fn [] :mock)]
            (throw (RuntimeException. "boom")))""")
    assert E("(-tcb40-target3)") == K("orig")

def test_with_redefs_multiple_vars():
    E("(def -tcb40-a (fn [] :a-orig))")
    E("(def -tcb40-b (fn [] :b-orig))")
    out = E("""
      (with-redefs [-tcb40-a (fn [] :a-mock)
                    -tcb40-b (fn [] :b-mock)]
        [(-tcb40-a) (-tcb40-b)])""")
    assert list(out) == [K("a-mock"), K("b-mock")]
    # Both restored.
    assert E("(-tcb40-a)") == K("a-orig")
    assert E("(-tcb40-b)") == K("b-orig")

def test_with_redefs_fn_explicit():
    """with-redefs is a macro over with-redefs-fn — exercise the fn directly."""
    E("(def -tcb40-c (fn [] :orig))")
    out = E("""
      (with-redefs-fn {(var -tcb40-c) (fn [] :explicit)}
        (fn [] (-tcb40-c)))""")
    assert out == K("explicit")
    assert E("(-tcb40-c)") == K("orig")


# --- cond-> ----------------------------------------------------

def test_cond_arrow_threads_when_true():
    """Both clauses fire."""
    assert E("(cond-> 1 true inc true (* 2))") == 4

def test_cond_arrow_skips_false_clauses():
    assert E("(cond-> 5 false inc true inc)") == 6

def test_cond_arrow_no_clauses_returns_expr():
    assert E("(cond-> 42)") == 42

def test_cond_arrow_doesnt_short_circuit():
    """Unlike cond, cond-> does NOT short-circuit after first true."""
    out = E("(cond-> 1 true inc true inc true inc)")
    assert out == 4


# --- cond->> ---------------------------------------------------

def test_cond_arrow_arrow_threads_last():
    """cond->> threads expr as last arg of each step."""
    out = E("(cond->> [1 2 3] true (map inc) true reverse)")
    assert list(out) == [4, 3, 2]

def test_cond_arrow_arrow_skip():
    out = E("(cond->> [1 2 3] false (map inc) true reverse)")
    assert list(out) == [3, 2, 1]


# --- as-> ------------------------------------------------------

def test_as_arrow_basic():
    assert E("(as-> 5 v (* v 2) (+ v 1) (str v))") == "11"

def test_as_arrow_no_forms_returns_expr():
    assert E("(as-> 42 v)") == 42

def test_as_arrow_explicit_position():
    """The point of as-> is putting the threading var anywhere in the form."""
    out = E("(as-> [1 2 3] coll (map inc coll) (reduce + coll))")
    assert out == 9


# --- some-> ----------------------------------------------------

def test_some_arrow_threads_when_non_nil():
    out = E("(some-> {:a {:b 42}} :a :b)")
    assert out == 42

def test_some_arrow_short_circuits_on_nil():
    """some-> bails the moment any step returns nil."""
    out = E("(some-> {:a nil} :a :b :c)")
    assert out is None

def test_some_arrow_initial_nil():
    assert E("(some-> nil :a :b)") is None

def test_some_arrow_no_forms():
    assert E("(some-> 42)") == 42

def test_some_arrow_with_falsy_non_nil():
    """some-> bails on nil, but NOT on false."""
    out = E("(some-> false (vector :tagged))")
    assert list(out) == [False, K("tagged")]


# --- some->> ---------------------------------------------------

def test_some_arrow_arrow_threads_last_when_non_nil():
    out = E("(some->> [1 2 3] (map inc) (filter odd?) first)")
    assert out == 3

def test_some_arrow_arrow_short_circuits():
    out = E("(some->> nil (map inc))")
    assert out is None


# --- assert-valid-fdecl ---------------------------------------

def test_assert_valid_fdecl_accepts_well_formed():
    """A valid fdecl is a list of (args body) lists.

    assert-valid-fdecl is private; call via the var directly."""
    fn = E("(deref (var clojure.core/assert-valid-fdecl))")
    # Should not raise.
    from clojure.lang import read_string as rs
    fn(rs("(([x] x) ([x y] (+ x y)))"))

def test_assert_valid_fdecl_throws_on_empty():
    fn = E("(deref (var clojure.core/assert-valid-fdecl))")
    from clojure.lang import read_string as rs
    with pytest.raises(Exception, match="missing"):
        fn(rs("()"))

def test_assert_valid_fdecl_throws_on_bad_args():
    """Non-vector arg list is rejected."""
    fn = E("(deref (var clojure.core/assert-valid-fdecl))")
    from clojure.lang import read_string as rs
    with pytest.raises(Exception, match="should be a vector"):
        fn(rs("(((not-a-vector) :body))"))
