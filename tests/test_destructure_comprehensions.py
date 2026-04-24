"""Tests for destructuring (let/loop/fn), doseq full grammar, and `for`."""

import pytest
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


# --- let destructuring ---

def test_let_vec_destructure():
    r = _ev("(let [[a b c] [1 2 3]] [a b c])")
    assert list(r) == [1, 2, 3]


def test_let_vec_with_rest():
    r = _ev("(let [[a & bs] [1 2 3 4]] [a (vec bs)])")
    assert [r[0], list(r[1])] == [1, [2, 3, 4]]


def test_let_vec_with_as():
    r = _ev("(let [[a b :as all] [1 2 3]] [a b (vec all)])")
    assert [r[0], r[1], list(r[2])] == [1, 2, [1, 2, 3]]


def test_let_nested_vec():
    r = _ev("(let [[[x y] z] [[1 2] 3]] [x y z])")
    assert list(r) == [1, 2, 3]


def test_let_map_destructure():
    r = _ev("(let [{a :a b :b} {:a 1 :b 2}] [a b])")
    assert list(r) == [1, 2]


def test_let_map_keys():
    r = _ev("(let [{:keys [a b]} {:a 1 :b 2}] [a b])")
    assert list(r) == [1, 2]


def test_let_map_or_defaults():
    r = _ev("(let [{:keys [a b] :or {b 99}} {:a 1}] [a b])")
    assert list(r) == [1, 99]


def test_let_map_as():
    r = _ev("(let [{:keys [a] :as m} {:a 1 :b 2}] [a (get m :b)])")
    assert list(r) == [1, 2]


# --- fn destructuring ---

def test_fn_vec_arg():
    r = _ev("((fn [[a b]] [a b]) [1 2])")
    assert list(r) == [1, 2]


def test_fn_nested_vec_arg():
    r = _ev("((fn [[[a b] c]] [a b c]) [[1 2] 3])")
    assert list(r) == [1, 2, 3]


def test_fn_rest_arg_destructure():
    r = _ev("((fn [a & [b c]] [a b c]) 1 2 3)")
    assert list(r) == [1, 2, 3]


def test_fn_map_arg():
    r = _ev("((fn [{:keys [a b]}] [a b]) {:a 1 :b 2})")
    assert list(r) == [1, 2]


def test_fn_preserves_multiple_arities():
    r = _ev("((fn ([x] x) ([x y] (+ x y))) 10)")
    assert r == 10


def test_named_fn_with_destructure():
    # Named fn with destructured arg — verifies self-ref still works.
    r = _ev("((fn self [[a b]] (if (> a 0) (self [(- a 1) (inc b)]) b)) [3 0])")
    assert r == 3


# --- loop destructuring ---

def test_loop_destructure():
    r = _ev("(loop [[a b] [3 0]] (if (zero? a) b (recur [(dec a) (inc b)])))")
    assert r == 3


# --- doseq full grammar ---

def test_doseq_when_filter():
    r = _ev(
        "(let [a (atom [])]"
        "  (doseq [x [1 2 3 4 5] :when (even? x)] (swap! a conj x))"
        "  (deref a))"
    )
    assert list(r) == [2, 4]


def test_doseq_while_stops():
    r = _ev(
        "(let [a (atom [])]"
        "  (doseq [x [1 2 3 4 5] :while (< x 4)] (swap! a conj x))"
        "  (deref a))"
    )
    assert list(r) == [1, 2, 3]


def test_doseq_let_binding():
    r = _ev(
        "(let [a (atom [])]"
        "  (doseq [x [1 2 3] :let [sq (* x x)]] (swap! a conj sq))"
        "  (deref a))"
    )
    assert list(r) == [1, 4, 9]


def test_doseq_multiple_bindings_cartesian():
    r = _ev(
        "(let [a (atom [])]"
        "  (doseq [x [1 2] y [:a :b]] (swap! a conj [x y]))"
        "  (deref a))"
    )
    assert [list(v) for v in r] == [[1, keyword("a")], [1, keyword("b")],
                                    [2, keyword("a")], [2, keyword("b")]]


def test_doseq_destructures_mapentry():
    r = _ev(
        "(let [a (atom [])]"
        "  (doseq [[k v] {:a 1 :b 2}] (swap! a conj [(clojure.core/name k) v]))"
        "  (deref a))"
    )
    pairs = sorted([tuple(list(x)) for x in r])
    assert pairs == [("a", 1), ("b", 2)]


def test_doseq_combined_modifiers():
    r = _ev(
        "(let [a (atom [])]"
        "  (doseq [x [1 2 3] :let [sq (* x x)] y [:a :b] :when (not= x 2)]"
        "    (swap! a conj [x sq y]))"
        "  (deref a))"
    )
    got = [list(v) for v in r]
    assert got == [[1, 1, keyword("a")], [1, 1, keyword("b")],
                   [3, 9, keyword("a")], [3, 9, keyword("b")]]


# --- for comprehension ---

def test_for_simple():
    assert list(_ev("(vec (for [x [1 2 3]] (* x x)))")) == [1, 4, 9]


def test_for_cartesian():
    r = _ev("(vec (for [x [1 2] y [:a :b]] [x y]))")
    assert [list(p) for p in r] == [[1, keyword("a")], [1, keyword("b")],
                                    [2, keyword("a")], [2, keyword("b")]]


def test_for_when():
    assert list(_ev("(vec (for [x (range 10) :when (even? x)] x))")) == [0, 2, 4, 6, 8]


def test_for_while():
    assert list(_ev("(vec (for [x (range 10) :while (< x 4)] x))")) == [0, 1, 2, 3]


def test_for_let_modifier():
    assert list(_ev("(vec (for [x [1 2 3] :let [sq (* x x)]] sq))")) == [1, 4, 9]


def test_for_lazy_on_infinite():
    # for must be lazy — take-only-N of an infinite coll.
    assert list(_ev("(vec (take 5 (for [x (range)] (* x x))))")) == [0, 1, 4, 9, 16]


def test_for_inner_binding_sees_outer():
    # :when using both x and y — proves nested-group dispatch.
    r = _ev("(vec (for [x (range 3) y (range 3) :when (< x y)] [x y]))")
    assert [list(p) for p in r] == [[0, 1], [0, 2], [1, 2]]


def test_for_destructures_binding():
    r = _ev("(vec (for [[a b] [[1 2] [3 4] [5 6]]] (+ a b)))")
    assert list(r) == [3, 7, 11]


# --- regression: bootstrap-let still works in downstream code ---

def test_named_fn_self_ref_with_plain_args():
    # Ensures our fn-redef didn't break the self-ref path.
    r = _ev("((fn sum [n] (if (zero? n) 0 (+ n (sum (dec n))))) 10)")
    assert r == 55
