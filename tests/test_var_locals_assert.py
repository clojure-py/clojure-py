"""Tests for var-set / with-local-vars, binding-conveyor-fn, assert,
and :pre / :post conditions in the fn macro."""

import pytest
from clojure._core import eval_string, AssertionError


def _ev(s):
    return eval_string(s)


# --- with-local-vars + var-set + var-get ---


def test_with_local_vars_basic():
    assert _ev("(with-local-vars [x 10 y 20] (+ (var-get x) (var-get y)))") == 30


def test_with_local_vars_var_set():
    assert _ev("(with-local-vars [x 10] (var-set x 99) (var-get x))") == 99


def test_with_local_vars_nested():
    # Inner var-set only affects inner scope.
    result = _ev("""
      (with-local-vars [x 1]
        (let* [outer (var-get x)]
          (with-local-vars [x 2]
            (var-set x 3)
            [outer (var-get x)])))
    """)
    assert list(result) == [1, 3]


def test_with_local_vars_returns_body_value():
    assert _ev("(with-local-vars [x 5] (var-set x 6) (var-set x 7) (var-get x))") == 7


def test_var_set_outside_with_local_vars_errors():
    from clojure._core import IllegalStateException
    with pytest.raises(IllegalStateException):
        _ev("(var-set (clojure.lang.RT/var-create) 1)")


# --- assert ---


def test_assert_true_returns_nil():
    assert _ev("(assert true)") is None


def test_assert_truthy_value():
    assert _ev("(assert 1)") is None
    assert _ev("(assert \"yes\")") is None


def test_assert_false_throws():
    with pytest.raises(AssertionError) as e:
        _ev("(assert false)")
    assert "Assert failed" in str(e.value)


def test_assert_nil_throws():
    with pytest.raises(AssertionError):
        _ev("(assert nil)")


def test_assert_with_message():
    with pytest.raises(AssertionError) as e:
        _ev("(assert (= 1 2) \"should be equal\")")
    msg = str(e.value)
    assert "should be equal" in msg
    assert "(= 1 2)" in msg


def test_assert_expr_in_message():
    with pytest.raises(AssertionError) as e:
        _ev("(assert (pos? -5))")
    assert "(pos? -5)" in str(e.value)


# --- :pre conditions on fn ---


def test_pre_single_condition():
    _ev("(def --f1 (fn [x] {:pre [(pos? x)]} (* x 2)))")
    assert _ev("(--f1 5)") == 10
    with pytest.raises(AssertionError):
        _ev("(--f1 -1)")


def test_pre_multiple_conditions():
    _ev("(def --f2 (fn [x y] {:pre [(pos? x) (pos? y)]} (+ x y)))")
    assert _ev("(--f2 2 3)") == 5
    with pytest.raises(AssertionError):
        _ev("(--f2 2 -1)")
    with pytest.raises(AssertionError):
        _ev("(--f2 -1 2)")


# --- :post conditions on fn ---


def test_post_single_condition():
    _ev("(def --f3 (fn [x] {:post [(number? %)]} (+ x 1)))")
    assert _ev("(--f3 5)") == 6


def test_post_rejects_bad_result():
    _ev("(def --f4 (fn [x] {:post [(string? %)]} (+ x 1)))")
    with pytest.raises(AssertionError):
        _ev("(--f4 5)")


def test_post_multiple_conditions():
    _ev("(def --f5 (fn [x] {:post [(number? %) (pos? %)]} (+ x 10)))")
    assert _ev("(--f5 5)") == 15
    with pytest.raises(AssertionError):
        _ev("(--f5 -100)")  # (pos? -90) fails


# --- :pre and :post together ---


def test_pre_and_post_together():
    _ev("(def --f6 (fn [x] {:pre [(pos? x)] :post [(> % x)]} (* x 2)))")
    assert _ev("(--f6 5)") == 10
    with pytest.raises(AssertionError):
        _ev("(--f6 -1)")  # :pre fires
    # :post: (> (* 0 2) 0) = (> 0 0) = false, but pre also catches (pos? 0).
    # Design a body where :post specifically fails:
    _ev("(def --f6-bad (fn [x] {:post [(> % 100)]} x))")
    with pytest.raises(AssertionError):
        _ev("(--f6-bad 5)")


def test_pre_post_defn_form():
    # Defn paths through fn, so the same conditions apply.
    _ev("(defn --f7 [x] {:pre [(number? x)]} (inc x))")
    assert _ev("(--f7 10)") == 11
    with pytest.raises(AssertionError):
        _ev("(--f7 \"nope\")")


def test_fn_without_conditions_still_works():
    # A fn whose first body form is a plain map (no :pre/:post) must NOT be
    # treated as a conditions map.
    _ev("(def --f8 (fn [x] {:a 1 :b 2} (* x x)))")
    # Currently our map-as-first-form is ambiguous; vanilla requires a body
    # AFTER the map for it to be conditions. We document: if the map has no
    # :pre/:post, it's just a value expression (the previous value) and the
    # next form is the actual return. Our implementation follows this.
    assert _ev("(--f8 4)") == 16


# --- binding-conveyor-fn ---


def test_binding_conveyor_fn_captures_frame():
    _ev("(def ^:dynamic *conv-var* :initial)")
    _ev("(def --conv-captured (atom nil))")
    # Capture the frame INSIDE a binding, then invoke the conveyor fn
    # OUTSIDE the binding — the bound value should still be visible.
    _ev("(def --conv-probe (binding [*conv-var* :conveyed] "
        "                    (binding-conveyor-fn "
        "                      (fn* [] (reset! --conv-captured *conv-var*)))))")
    _ev("(--conv-probe)")
    assert _ev("@--conv-captured") == _ev(":conveyed")


def test_binding_conveyor_fn_with_args():
    _ev("(def ^:dynamic *multiplier* 1)")
    _ev("(def --mult-fn (binding [*multiplier* 10] "
        "                 (binding-conveyor-fn (fn [x] (* x *multiplier*)))))")
    assert _ev("(--mult-fn 7)") == 70


def test_clone_reset_binding_frame_roundtrip():
    _ev("(def ^:dynamic *rb* :outer)")
    # Capture a frame with :inner, then reset to it and check.
    _ev("(def --captured-frame "
        "  (binding [*rb* :inner] "
        "    (clojure.lang.RT/clone-thread-binding-frame)))")
    _ev("(def --result (atom nil))")
    _ev("(clojure.lang.RT/reset-thread-binding-frame --captured-frame)")
    _ev("(reset! --result *rb*)")
    # Don't leave frame installed — pop back to an empty stack.
    _ev("(clojure.lang.RT/reset-thread-binding-frame "
        "  (binding [] (clojure.lang.RT/clone-thread-binding-frame)))")
    assert _ev("@--result") == _ev(":inner")
