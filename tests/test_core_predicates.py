"""Tests for the Phase 1b gap-fill ports: predicates, constructors,
symbol/keyword utilities, `vary-meta`, `if-not`, and `compare`."""

import pytest
from clojure._core import eval_string, keyword, IllegalArgumentException


def _ev(src):
    return eval_string(src)


# --- nil? / false? / true? / boolean? / not / some? / any? ---

def test_nil_predicate():
    assert _ev("(nil? nil)") is True
    assert _ev("(nil? 0)") is False
    assert _ev("(nil? false)") is False
    assert _ev("(nil? [])") is False


def test_some_and_any():
    assert _ev("(some? nil)") is False
    assert _ev("(some? 0)") is True
    assert _ev("(any? nil)") is True
    assert _ev("(any? false)") is True


# --- Collection constructors ---

def test_vector_variadic():
    v = _ev("(vector 1 2 3)")
    assert list(v) == [1, 2, 3]


def test_vec_from_seq():
    v = _ev("(vec (seq [10 20 30]))")
    assert list(v) == [10, 20, 30]


def test_hash_map_variadic():
    m = _ev("(hash-map :a 1 :b 2)")
    assert m[keyword("a")] == 1
    assert m[keyword("b")] == 2


def test_hash_map_odd_raises():
    with pytest.raises(IllegalArgumentException):
        _ev("(hash-map :a 1 :b)")


def test_hash_set_dedupes():
    s = _ev("(hash-set 1 2 2 3)")
    assert set(s) == {1, 2, 3}


# --- gensym / find-keyword ---

def test_gensym_distinct():
    a = _ev("(gensym)")
    b = _ev("(gensym)")
    assert a != b
    assert _ev("(symbol? (gensym))") is True


def test_gensym_prefix():
    s = _ev('(gensym "prefix-")')
    assert str(s).startswith("prefix-")


def test_find_keyword_returns_nil_for_unseen():
    assert _ev('(find-keyword "never-interned-zzz-1")') is None


def test_find_keyword_returns_interned():
    _ev(":seen-kw-for-find")
    kw = _ev('(find-keyword "seen-kw-for-find")')
    assert kw == keyword("seen-kw-for-find")


# --- list* ---

def test_list_star_all_arities():
    assert list(_ev("(list* [1 2])")) == [1, 2]
    assert list(_ev("(list* 0 [1 2])")) == [0, 1, 2]
    assert list(_ev("(list* 0 1 [2 3])")) == [0, 1, 2, 3]
    assert list(_ev("(list* 0 1 2 [3 4])")) == [0, 1, 2, 3, 4]
    assert list(_ev("(list* 0 1 2 3 [4 5])")) == [0, 1, 2, 3, 4, 5]


# --- vary-meta ---

def test_vary_meta_extends_metadata():
    r = _ev("(meta (vary-meta ^{:a 1} {:b 2} assoc :c 3))")
    assert r[keyword("a")] == 1
    assert r[keyword("c")] == 3


# --- if-not ---

def test_if_not_both_branches():
    assert _ev("(if-not false :yes :no)") == keyword("yes")
    assert _ev("(if-not true :yes :no)") == keyword("no")


def test_if_not_else_omitted():
    assert _ev("(if-not true :yes)") is None
    assert _ev("(if-not false :yes)") == keyword("yes")


# --- compare ---

def test_compare_ints():
    assert _ev("(compare 1 2)") == -1
    assert _ev("(compare 2 1)") == 1
    assert _ev("(compare 3 3)") == 0


def test_compare_strings():
    assert _ev('(compare "a" "b")') == -1
    assert _ev('(compare "b" "a")') == 1


def test_compare_nil_sorts_first():
    assert _ev("(compare nil 5)") == -1
    assert _ev("(compare 5 nil)") == 1
    assert _ev("(compare nil nil)") == 0


def test_compare_incomparable_raises():
    with pytest.raises(IllegalArgumentException):
        _ev('(compare 1 "x")')
