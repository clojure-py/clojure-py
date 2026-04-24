"""Tests for the partition family, forcing machinery (dorun/doall/doseq),
and utility macros (when-first/while/memoize/trampoline/condp)."""

import pytest
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


def _vec(src):
    return list(eval_string(f"(vec {src})"))


def _vecs(src):
    """For results containing nested lazy seqs — walk each partition."""
    return [list(p) for p in eval_string(f"(vec {src})")]


# --- partition / partition-all / partition-by ---

def test_partition_even_split():
    assert _vecs("(partition 2 [1 2 3 4 5 6])") == [[1, 2], [3, 4], [5, 6]]


def test_partition_drops_short_tail():
    # partition with no pad drops trailing short partition.
    assert _vecs("(partition 2 [1 2 3 4 5])") == [[1, 2], [3, 4]]


def test_partition_with_step():
    # Overlapping: step=2, n=3 → [1 2 3] [3 4 5] [5 6 7]
    assert _vecs("(partition 3 2 [1 2 3 4 5 6 7])") == [[1, 2, 3], [3, 4, 5], [5, 6, 7]]


def test_partition_with_pad():
    # Pad completes trailing partition.
    assert _vecs("(partition 3 2 [:a :b] [1 2 3 4 5])") == [
        [1, 2, 3], [3, 4, 5], [5, keyword("a"), keyword("b")],
    ]


def test_partition_all_keeps_short_tail():
    assert _vecs("(partition-all 2 [1 2 3 4 5])") == [[1, 2], [3, 4], [5]]


def test_partition_by_runs():
    assert _vecs("(partition-by even? [1 1 2 2 3 1])") == [[1, 1], [2, 2], [3, 1]]


def test_partition_by_all_same():
    assert _vecs("(partition-by identity [1 1 1])") == [[1, 1, 1]]


def test_partition_lazy_on_infinite():
    # Only forces enough of the infinite range to fill 3 partitions.
    assert _vecs("(take 3 (partition 2 (range)))") == [[0, 1], [2, 3], [4, 5]]


# --- dorun / doall / doseq ---

def test_dorun_forces_side_effects():
    r = _ev(
        "(let* [a (atom 0)]"
        "  (dorun (map (fn [_] (swap! a inc)) [1 2 3 4 5]))"
        "  (deref a))"
    )
    assert r == 5


def test_dorun_returns_nil():
    assert _ev("(dorun [1 2 3])") is None


def test_doall_returns_head_and_forces():
    r = _ev(
        "(let* [a (atom 0)]"
        "  (let* [s (doall (map (fn [x] (swap! a inc) x) [10 20 30]))]"
        "    [(deref a) (vec s)]))"
    )
    counter = r[0]
    assert counter == 3
    assert list(r[1]) == [10, 20, 30]


def test_doseq_executes_body_per_elem():
    r = _ev(
        "(let* [a (atom [])]"
        "  (doseq [x [1 2 3]] (swap! a conj (* x 10)))"
        "  (deref a))"
    )
    assert list(r) == [10, 20, 30]


def test_doseq_returns_nil():
    assert _ev("(doseq [x [1 2]] x)") is None


# --- when-first / while ---

def test_when_first_non_empty():
    assert _ev("(when-first [x [10 20 30]] x)") == 10


def test_when_first_empty():
    assert _ev("(when-first [x []] :body)") is None


def test_while_loop_runs_until_false():
    r = _ev(
        "(let* [a (atom 3) log (atom [])]"
        "  (while (pos? (deref a))"
        "    (swap! log conj (deref a))"
        "    (swap! a dec))"
        "  (deref log))"
    )
    assert list(r) == [3, 2, 1]


# --- memoize ---

def test_memoize_caches_result():
    r = _ev(
        "(let* [calls (atom 0)"
        "       f (memoize (fn [x] (swap! calls inc) (* x 2)))]"
        "  (f 5) (f 5) (f 5)"
        "  (deref calls))"
    )
    assert r == 1  # Only called once.


def test_memoize_different_args():
    r = _ev(
        "(let* [calls (atom 0)"
        "       f (memoize (fn [x] (swap! calls inc) (* x 2)))]"
        "  (f 5) (f 6) (f 5) (f 6)"
        "  (deref calls))"
    )
    assert r == 2  # Called once per unique arg.


# --- trampoline ---

def test_trampoline_avoids_stack_overflow():
    # 10000 mutually-recursive calls; would blow the stack without trampoline.
    r = _ev(
        "(trampoline"
        "  (fn rec [n] (if (zero? n) :done (fn [] (rec (dec n)))))"
        "  10000)"
    )
    assert r == keyword("done")


def test_trampoline_non_fn_return():
    # If f returns a non-fn immediately, that's returned.
    assert _ev("(trampoline (fn [] 42))") == 42


def test_trampoline_with_args():
    # trampoline with initial args packs into a thunk.
    assert _ev("(trampoline (fn [x] (* x 2)) 5)") == 10


# --- condp ---

def test_condp_match():
    assert _ev("(condp = 5 1 :one 5 :five :default)") == keyword("five")


def test_condp_default():
    assert _ev("(condp = 99 1 :one 5 :five :default)") == keyword("default")


def test_condp_no_default_raises():
    from clojure._core import IllegalArgumentException
    with pytest.raises(IllegalArgumentException):
        _ev("(condp = 99 1 :one 5 :five)")


def test_condp_arrow_form():
    # `:>>` arrow passes the pred result to a fn.
    r = _ev("(condp + 3 10 :>> (fn [n] [:matched n]) :default)")
    assert list(r) == [keyword("matched"), 13]


# --- ifn? / fn? distinction ---

def test_fn_pred_on_real_fn():
    assert _ev("(fn? (fn [] 1))") is True
    assert _ev("(fn? inc)") is True


def test_fn_pred_rejects_pseudo_fns():
    # Keywords, maps, sets all implement IFn but aren't fn?.
    assert _ev("(fn? :x)") is False
    assert _ev("(fn? {:a 1})") is False
    assert _ev("(fn? #{1 2})") is False


def test_ifn_pred_accepts_pseudo_fns():
    assert _ev("(ifn? :x)") is True
    assert _ev("(ifn? {:a 1})") is True
    assert _ev("(ifn? #{1 2})") is True
    assert _ev("(ifn? (fn [] 1))") is True


# --- Named fn self-recursion (the compiler fix) ---

def test_named_fn_self_ref_same_level():
    # (fn name [args] (name ...)) — self-reference inside the body.
    r = _ev(
        "(let* [f (fn sum [n] (if (zero? n) 0 (+ n (sum (dec n)))))]"
        "  (f 10))"
    )
    assert r == 55  # 1+2+...+10


def test_named_fn_self_ref_nested():
    # Inner fn references outer fn's self-name.
    r = _ev(
        "(let* [outer (fn outer [xs]"
        "               (let* [inner (fn inner [n]"
        "                              (if (zero? n) outer (inner (dec n))))]"
        "                 (fn? (inner 3))))]"
        "  (outer :irrelevant))"
    )
    assert r is True
