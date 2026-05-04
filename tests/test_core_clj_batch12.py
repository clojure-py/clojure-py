"""Tests for core.clj batch 12 (lines 3069-3351, pure-data forms):

merge, merge-with,
comparator, sort, sort-by,
dorun, doall,
nthnext, nthrest, partition,
eval,
doseq (macro), dotimes (macro re-def with assert-args)

The IO/concurrency forms from this same JVM range — line-seq, await,
await1, await-for — live in test_core_clj_batch12_io.py alongside the
java.io.BufferedReader / java.util.concurrent.CountDownLatch /
java.util.concurrent.TimeUnit shims they depend on.
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    PersistentVector, PersistentArrayMap, PersistentList, ISeq,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- merge ---------------------------------------------------------

def test_merge_two_maps():
    m = E("(clojure.core/merge {:a 1} {:b 2})")
    assert dict(m) == {K("a"): 1, K("b"): 2}

def test_merge_overrides_left_to_right():
    m = E("(clojure.core/merge {:a 1 :b 2} {:b 99 :c 3})")
    assert dict(m) == {K("a"): 1, K("b"): 99, K("c"): 3}

def test_merge_nil_passthrough():
    m = E("(clojure.core/merge nil {:a 1})")
    assert dict(m) == {K("a"): 1}

def test_merge_no_args_is_nil():
    assert E("(clojure.core/merge)") is None

def test_merge_all_nil_is_nil():
    assert E("(clojure.core/merge nil nil nil)") is None

def test_merge_three_maps():
    m = E("(clojure.core/merge {:a 1} {:b 2} {:c 3})")
    assert dict(m) == {K("a"): 1, K("b"): 2, K("c"): 3}


# --- merge-with ----------------------------------------------------

def test_merge_with_combines_dupes():
    m = E("(clojure.core/merge-with clojure.core/+ {:a 1 :b 2} {:b 3 :c 4})")
    assert dict(m) == {K("a"): 1, K("b"): 5, K("c"): 4}

def test_merge_with_keeps_unique_keys():
    m = E("(clojure.core/merge-with clojure.core/+ {:a 1} {:b 2})")
    assert dict(m) == {K("a"): 1, K("b"): 2}

def test_merge_with_three_maps_uses_left_acc():
    """When key recurs in N maps, fn is applied (N-1) times left-to-right."""
    m = E("(clojure.core/merge-with clojure.core/+ {:a 1} {:a 2} {:a 3})")
    assert dict(m) == {K("a"): 6}

def test_merge_with_nil_treated_as_empty():
    m = E("(clojure.core/merge-with clojure.core/+ nil {:a 1})")
    assert dict(m) == {K("a"): 1}

def test_merge_with_no_maps_is_nil():
    assert E("(clojure.core/merge-with clojure.core/+)") is None


# --- comparator ----------------------------------------------------

def test_comparator_lt_returns_neg_one():
    assert E("((clojure.core/comparator clojure.core/<) 1 2)") == -1

def test_comparator_gt_returns_one():
    assert E("((clojure.core/comparator clojure.core/<) 2 1)") == 1

def test_comparator_eq_returns_zero():
    assert E("((clojure.core/comparator clojure.core/<) 2 2)") == 0


# --- sort ----------------------------------------------------------

def test_sort_default():
    assert list(E("(clojure.core/sort [3 1 4 1 5 9 2 6])")) == [1, 1, 2, 3, 4, 5, 6, 9]

def test_sort_strings():
    assert list(E('(clojure.core/sort ["banana" "apple" "cherry"])')) == ["apple", "banana", "cherry"]

def test_sort_with_gt_predicate():
    """Pass `>` directly — Arrays/sort shim must handle bool returns."""
    assert list(E("(clojure.core/sort clojure.core/> [3 1 4 1 5 9 2 6])")) == [9, 6, 5, 4, 3, 2, 1, 1]

def test_sort_with_lt_predicate():
    assert list(E("(clojure.core/sort clojure.core/< [3 1 4])")) == [1, 3, 4]

def test_sort_with_int_comparator():
    """A 3-way int-returning comparator works alongside the bool path."""
    # comparator that sorts even before odd, otherwise natural order
    cmp = ("(fn* [a b] (clojure.core/cond "
           "(clojure.core/and (clojure.core/even? a) (clojure.core/odd? b)) -1 "
           "(clojure.core/and (clojure.core/odd? a) (clojure.core/even? b)) 1 "
           ":else (clojure.core/compare a b)))")
    out = list(E(f"(clojure.core/sort {cmp} [3 1 4 1 5 9 2 6])"))
    # all evens first, then odds, each group sorted
    evens = [x for x in out if x % 2 == 0]
    odds = [x for x in out if x % 2 == 1]
    assert evens == sorted(evens)
    assert odds == sorted(odds)
    assert all(x % 2 == 0 for x in out[:len(evens)])

def test_sort_empty_returns_empty_list():
    out = E("(clojure.core/sort [])")
    assert list(out) == []

def test_sort_stable():
    """JVM Arrays/sort is stable; Python's Timsort is too."""
    pairs = [[1, "a"], [2, "x"], [1, "b"], [2, "y"], [1, "c"]]
    src = "[" + " ".join(f'[{p[0]} \"{p[1]}\"]' for p in pairs) + "]"
    out = list(E(f"(clojure.core/sort-by clojure.core/first {src})"))
    # all 1's come before all 2's, original relative order preserved within
    keys_for_1 = [p[1] for p in out if p[0] == 1]
    keys_for_2 = [p[1] for p in out if p[0] == 2]
    assert keys_for_1 == ["a", "b", "c"]
    assert keys_for_2 == ["x", "y"]


# --- sort-by -------------------------------------------------------

def test_sort_by_first():
    out = list(E("(clojure.core/sort-by clojure.core/first [[3 :c] [1 :a] [2 :b]])"))
    assert [list(p) for p in out] == [[1, K("a")], [2, K("b")], [3, K("c")]]

def test_sort_by_count_with_gt():
    out = list(E('(clojure.core/sort-by clojure.core/count clojure.core/> ["abc" "a" "ab"])'))
    assert out == ["abc", "ab", "a"]

def test_sort_by_with_compare():
    out = list(E("(clojure.core/sort-by clojure.core/- [3 1 2])"))
    # negation flips the order: original 3 1 2 → keys -3 -1 -2 → sorted -3 -2 -1 → 3 2 1
    assert out == [3, 2, 1]


# --- dorun ---------------------------------------------------------

def test_dorun_returns_nil():
    assert E("(clojure.core/dorun [1 2 3])") is None

def test_dorun_forces_lazy_seq():
    """dorun walks the seq, triggering side effects."""
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb12-dorun-poke!"),
               lambda x: (counter.append(x), x)[1])
    E("(clojure.core/dorun (clojure.core/map user/tcb12-dorun-poke! [10 20 30]))")
    assert counter == [10, 20, 30]

def test_dorun_n_only_walks_n():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb12-dorun2-poke!"),
               lambda x: (counter.append(x), x)[1])
    E("(clojure.core/dorun 2 (clojure.core/map user/tcb12-dorun2-poke! [10 20 30 40 50]))")
    # chunked seqs may realize a whole chunk at once, so we only assert at-least
    assert counter[:2] == [10, 20]

def test_dorun_nil():
    assert E("(clojure.core/dorun nil)") is None


# --- doall ---------------------------------------------------------

def test_doall_returns_seq():
    out = list(E("(clojure.core/doall (clojure.core/range 4))"))
    assert out == [0, 1, 2, 3]

def test_doall_is_eager():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb12-doall-poke!"),
               lambda x: (counter.append(x), x + 100)[1])
    out = list(E("(clojure.core/doall (clojure.core/map user/tcb12-doall-poke! [1 2 3]))"))
    assert counter == [1, 2, 3]
    assert out == [101, 102, 103]

def test_doall_n_returns_seq():
    out = list(E("(clojure.core/doall 2 (clojure.core/range 5))"))
    # doall returns the WHOLE seq, regardless of n; n only controls how much is forced
    assert out == [0, 1, 2, 3, 4]


# --- nthnext -------------------------------------------------------

def test_nthnext_basic():
    assert list(E("(clojure.core/nthnext [1 2 3 4 5] 2)")) == [3, 4, 5]

def test_nthnext_zero_returns_seq():
    assert list(E("(clojure.core/nthnext [1 2 3] 0)")) == [1, 2, 3]

def test_nthnext_past_end_returns_nil():
    assert E("(clojure.core/nthnext [1 2 3] 10)") is None

def test_nthnext_uses_idrop_fast_path_on_vector():
    out = E("(clojure.core/nthnext [1 2 3 4 5] 2)")
    assert list(out) == [3, 4, 5]

def test_nthnext_on_lazy_seq():
    out = E("(clojure.core/nthnext (clojure.core/range 6) 3)")
    assert list(out) == [3, 4, 5]

def test_nthnext_empty():
    assert E("(clojure.core/nthnext nil 2)") is None


# --- nthrest -------------------------------------------------------

def test_nthrest_basic():
    assert list(E("(clojure.core/nthrest [1 2 3 4 5] 2)")) == [3, 4, 5]

def test_nthrest_zero_returns_coll():
    out = E("(clojure.core/nthrest [1 2 3] 0)")
    assert list(out) == [1, 2, 3]

def test_nthrest_past_end_is_empty():
    out = E("(clojure.core/nthrest [1 2 3] 10)")
    assert list(out) == []

def test_nthrest_on_lazy():
    out = E("(clojure.core/nthrest (clojure.core/range 5) 2)")
    assert list(out) == [2, 3, 4]


# --- partition -----------------------------------------------------

def test_partition_n():
    out = list(E("(clojure.core/partition 2 [1 2 3 4 5])"))
    assert [list(p) for p in out] == [[1, 2], [3, 4]]

def test_partition_n_step():
    out = list(E("(clojure.core/partition 2 1 [1 2 3 4])"))
    assert [list(p) for p in out] == [[1, 2], [2, 3], [3, 4]]

def test_partition_n_step_pad():
    out = list(E("(clojure.core/partition 3 3 [:x :y] [1 2 3 4 5])"))
    pads = [K("x"), K("y")]
    assert [list(p) for p in out] == [[1, 2, 3], [4, 5, pads[0]]]

def test_partition_pad_short():
    """If pad is shorter than needed, last group has < n items."""
    out = list(E("(clojure.core/partition 3 3 [:x] [1 2 3 4])"))
    assert [list(p) for p in out] == [[1, 2, 3], [4, K("x")]]

def test_partition_step_larger_than_n():
    out = list(E("(clojure.core/partition 2 4 [1 2 3 4 5 6 7 8 9])"))
    assert [list(p) for p in out] == [[1, 2], [5, 6], [9]] or \
           [list(p) for p in out] == [[1, 2], [5, 6]]
    # JVM yields [(1 2) (5 6)] (drops trailing partial); confirm
    assert [list(p) for p in out] == [[1, 2], [5, 6]]

def test_partition_empty():
    assert list(E("(clojure.core/partition 2 [])")) == []


# --- eval ----------------------------------------------------------

def test_eval_basic():
    assert E("(clojure.core/eval (clojure.core/list (quote clojure.core/+) 1 2 3))") == 6

def test_eval_literal():
    assert E("(clojure.core/eval 42)") == 42

def test_eval_symbol_resolves_in_caller_ns():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb12-evalv"), 99)
    assert E("(clojure.core/eval (quote user/tcb12-evalv))") == 99


# --- doseq (macro) -------------------------------------------------

def _doseq_capture():
    """Install a `user/out!` var that appends its arg to a list, return list."""
    out = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb12-out!"),
               lambda x: (out.append(x), x)[1])
    return out

def test_doseq_single_binding():
    out = _doseq_capture()
    E("(clojure.core/doseq [x [1 2 3]] (user/tcb12-out! x))")
    assert out == [1, 2, 3]

def test_doseq_returns_nil():
    out = _doseq_capture()
    assert E("(clojure.core/doseq [x [1 2 3]] (user/tcb12-out! x))") is None

def test_doseq_two_bindings_cartesian():
    out = _doseq_capture()
    E("(clojure.core/doseq [a [1 2] b [:x :y]] (user/tcb12-out! [a b]))")
    assert [list(p) for p in out] == [[1, K("x")], [1, K("y")], [2, K("x")], [2, K("y")]]

def test_doseq_when_filter():
    out = _doseq_capture()
    E("(clojure.core/doseq [x [1 2 3 4 5] :when (clojure.core/even? x)] (user/tcb12-out! x))")
    assert out == [2, 4]

def test_doseq_while_short_circuits():
    out = _doseq_capture()
    E("(clojure.core/doseq [x [1 2 3 -1 4] :while (clojure.core/pos? x)] (user/tcb12-out! x))")
    assert out == [1, 2, 3]

def test_doseq_let_binding():
    out = _doseq_capture()
    E("(clojure.core/doseq [x [1 2 3] :let [y (clojure.core/* x x)]] (user/tcb12-out! [x y]))")
    assert [list(p) for p in out] == [[1, 1], [2, 4], [3, 9]]

def test_doseq_empty_coll_does_nothing():
    out = _doseq_capture()
    E("(clojure.core/doseq [x []] (user/tcb12-out! x))")
    assert out == []

def test_doseq_assert_args_non_vector():
    with pytest.raises(Exception, match="vector for its binding"):
        E("(clojure.core/doseq (x [1 2]) x)")

def test_doseq_assert_args_odd_count():
    with pytest.raises(Exception, match="even number of forms"):
        E("(clojure.core/doseq [x [1 2] y] x)")


# --- dotimes (re-defed) --------------------------------------------

def test_dotimes_redef_basic():
    out = _doseq_capture()
    E("(clojure.core/dotimes [i 5] (user/tcb12-out! i))")
    assert out == [0, 1, 2, 3, 4]

def test_dotimes_zero_runs_nothing():
    out = _doseq_capture()
    E("(clojure.core/dotimes [i 0] (user/tcb12-out! i))")
    assert out == []

def test_dotimes_assert_args_non_vector():
    with pytest.raises(Exception, match="vector for its binding"):
        E("(clojure.core/dotimes (i 5) i)")

def test_dotimes_assert_args_wrong_arity():
    with pytest.raises(Exception, match="exactly 2 forms"):
        E("(clojure.core/dotimes [i 5 6] i)")
