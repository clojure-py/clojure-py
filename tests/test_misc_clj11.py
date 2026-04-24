"""Letfn, ex-info family, fnil, regex, parse-* and random-uuid."""

import uuid
import pytest
from clojure._core import eval_string, ExceptionInfo


def _ev(s):
    return eval_string(s)


# --- letfn ------------------------------------------------------------------


def test_letfn_simple():
    assert _ev("(letfn [(f [x] (* x 2))] (f 5))") == 10


def test_letfn_mutual_recursion():
    assert (
        _ev(
            "(letfn [(my-even? [n] (if (= n 0) true (my-odd? (- n 1))))"
            "        (my-odd?  [n] (if (= n 0) false (my-even? (- n 1))))]"
            "  (my-even? 6))"
        )
        is True
    )
    assert (
        _ev(
            "(letfn [(my-even? [n] (if (= n 0) true (my-odd? (- n 1))))"
            "        (my-odd?  [n] (if (= n 0) false (my-even? (- n 1))))]"
            "  (my-odd? 7))"
        )
        is True
    )


def test_letfn_multi_arity():
    assert (
        _ev("(letfn [(g ([x] (g x 10)) ([x y] (+ x y)))] (g 5))")
        == 15
    )


def test_letfn_body_can_use_other_fns():
    # squared composes through doubled; both are letfn-bound
    assert (
        _ev(
            "(letfn [(doubled [x] (* 2 x))"
            "        (sum-doubled [a b] (+ (doubled a) (doubled b)))]"
            "  (sum-doubled 3 4))"
        )
        == 14
    )


# --- ex-info ----------------------------------------------------------------


def test_ex_info_construct():
    e = _ev('(ex-info "boom" {:code 42})')
    assert isinstance(e, ExceptionInfo)
    assert e.args == ("boom",)


def test_ex_info_subclass_of_exception():
    assert issubclass(ExceptionInfo, Exception)


def test_ex_data_returns_map():
    assert _ev('(ex-data (ex-info "x" {:a 1 :b 2}))') == _ev("{:a 1 :b 2}")


def test_ex_data_nil_for_non_ex_info():
    assert _ev("(ex-data 42)") is None
    assert _ev('(ex-data "no data here")') is None


def test_ex_message():
    assert _ev('(ex-message (ex-info "hello" {}))') == "hello"


def test_ex_cause():
    inner = _ev('(ex-info "inner" {:n 1})')
    outer = _ev(f'(ex-info "outer" {{}} (ex-info "inner" {{:n 1}}))')
    assert _ev('(ex-cause (ex-info "outer" {} (ex-info "inner" {:n 1})))').args == ("inner",)


def test_ex_cause_nil_when_no_cause():
    assert _ev('(ex-cause (ex-info "x" {}))') is None


def test_ex_info_caught_by_python_exception():
    assert (
        _ev(
            '(try (throw (ex-info "boom" {:k 99}))'
            "  (catch builtins/Exception e (ex-data e)))"
        )
        == _ev("{:k 99}")
    )


def test_ex_info_caught_propagates_to_python():
    with pytest.raises(ExceptionInfo) as ei:
        _ev('(throw (ex-info "out" {:why :test}))')
    assert ei.value.data == _ev("{:why :test}")


# --- fnil -------------------------------------------------------------------


def test_fnil_one_replacement_nil():
    assert _ev("((fnil + 10) nil)") == 10


def test_fnil_one_replacement_non_nil():
    assert _ev("((fnil + 10) 5)") == 5


def test_fnil_two_replacements():
    assert _ev("((fnil + 10 20) nil nil)") == 30
    assert _ev("((fnil + 10 20) 1 nil)") == 21
    assert _ev("((fnil + 10 20) nil 2)") == 12


def test_fnil_three_replacements():
    assert _ev("((fnil + 10 20 30) nil nil nil)") == 60


def test_fnil_passes_extra_args_through():
    assert _ev("((fnil + 0) nil 1 2 3)") == 6


def test_fnil_inc_pattern():
    # Classic use case: increment a counter that may be nil.
    assert _ev("((fnil inc 0) nil)") == 1
    assert _ev("((fnil inc 0) 5)") == 6


# --- regex ------------------------------------------------------------------


def test_re_pattern_compiles():
    p = _ev('(re-pattern "a+")')
    import re
    assert isinstance(p, re.Pattern)


def test_re_pattern_idempotent():
    p1 = _ev('(re-pattern "a+")')
    # passing a Pattern through re-pattern should return as-is
    import sys
    sys.modules["clojure.user"].__dict__.setdefault
    _ev("(def --p1 nil)")
    sys.modules["clojure.user"].__dict__["--p1"].bind_root(p1)
    assert _ev("(re-pattern --p1)") is p1


def test_re_find_no_groups():
    assert _ev('(re-find (re-pattern "a+") "baaab")') == "aaa"


def test_re_find_with_groups():
    result = list(_ev('(re-find (re-pattern "([0-9]+)-([0-9]+)") "abc 12-34")'))
    assert result == ["12-34", "12", "34"]


def test_re_find_no_match():
    assert _ev('(re-find (re-pattern "z+") "abc")') is None


def test_re_matches_full_match():
    assert _ev('(re-matches (re-pattern "a+") "aaa")') == "aaa"


def test_re_matches_partial_returns_nil():
    assert _ev('(re-matches (re-pattern "a+") "baaa")') is None


def test_re_matches_with_groups():
    result = list(_ev('(re-matches (re-pattern "([0-9]+)-([0-9]+)") "12-34")'))
    assert result == ["12-34", "12", "34"]


def test_re_seq_finds_all():
    assert list(_ev('(re-seq (re-pattern "a+") "baaa b aa")')) == ["aaa", "aa"]


def test_re_seq_empty_returns_nil():
    assert _ev('(re-seq (re-pattern "z+") "abc")') is None


def test_re_seq_with_groups():
    decoded = [list(x) for x in _ev('(re-seq (re-pattern "([0-9]+)-([0-9]+)") "12-34 56-78")')]
    assert decoded == [["12-34", "12", "34"], ["56-78", "56", "78"]]


# --- parse-* ----------------------------------------------------------------


def test_parse_long_basic():
    assert _ev('(parse-long "42")') == 42


def test_parse_long_negative():
    assert _ev('(parse-long "-7")') == -7


def test_parse_long_invalid():
    assert _ev('(parse-long "abc")') is None


def test_parse_long_empty():
    assert _ev('(parse-long "")') is None


def test_parse_double_basic():
    assert _ev('(parse-double "3.14")') == 3.14


def test_parse_double_int_form():
    assert _ev('(parse-double "5")') == 5.0


def test_parse_double_invalid():
    assert _ev('(parse-double "xyz")') is None


def test_parse_boolean_true():
    assert _ev('(parse-boolean "true")') is True


def test_parse_boolean_false():
    assert _ev('(parse-boolean "false")') is False


def test_parse_boolean_invalid():
    assert _ev('(parse-boolean "TRUE")') is None
    assert _ev('(parse-boolean "yes")') is None


def test_parse_uuid_valid():
    s = "12345678-1234-5678-1234-567812345678"
    result = _ev(f'(parse-uuid "{s}")')
    assert isinstance(result, uuid.UUID)
    assert str(result) == s


def test_parse_uuid_invalid():
    assert _ev('(parse-uuid "not-a-uuid")') is None


def test_random_uuid_returns_uuid():
    u = _ev("(random-uuid)")
    assert isinstance(u, uuid.UUID)


def test_random_uuid_distinct():
    a = _ev("(random-uuid)")
    b = _ev("(random-uuid)")
    assert a != b
