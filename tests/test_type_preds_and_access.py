"""Tests for the chunk porting vanilla 1394-1588: type predicates,
identity utilities, collection access, symbol/keyword utilities."""

import pytest
from hypothesis import given, strategies as st, settings
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


# --- Type predicates ---

def test_integer_excludes_bool():
    assert _ev("(integer? 5)") is True
    assert _ev("(integer? -7)") is True
    assert _ev("(integer? true)") is False
    assert _ev("(integer? false)") is False
    assert _ev("(integer? 3.14)") is False
    assert _ev('(integer? "5")') is False


def test_double_pred():
    assert _ev("(double? 3.14)") is True
    assert _ev("(double? 5)") is False


def test_number_pred():
    assert _ev("(number? 5)") is True
    assert _ev("(number? 3.14)") is True
    assert _ev("(number? true)") is False
    assert _ev('(number? "x")') is False


def test_even_odd():
    assert _ev("(even? 0)") is True
    assert _ev("(even? 4)") is True
    assert _ev("(even? 5)") is False
    assert _ev("(odd? 5)") is True
    assert _ev("(odd? -3)") is True


def test_even_on_non_integer_raises():
    from clojure._core import IllegalArgumentException
    with pytest.raises(IllegalArgumentException):
        _ev("(even? 3.14)")


def test_int_variants():
    assert _ev("(int? 5)") is True
    assert _ev("(pos-int? 5)") is True
    assert _ev("(pos-int? 0)") is False
    assert _ev("(pos-int? -1)") is False
    assert _ev("(neg-int? -3)") is True
    assert _ev("(neg-int? 0)") is False
    assert _ev("(nat-int? 0)") is True
    assert _ev("(nat-int? 5)") is True
    assert _ev("(nat-int? -1)") is False


# --- Identity utilities ---

def test_identity():
    assert _ev("(identity 42)") == 42
    assert _ev("(identity :x)") == keyword("x")
    assert _ev("(identity nil)") is None


def test_constantly():
    assert _ev("((constantly :x) 1 2 3 4 5)") == keyword("x")
    assert _ev("((constantly 7))") == 7


def test_complement():
    assert _ev("((complement pos?) -3)") is True
    assert _ev("((complement pos?) 3)") is False
    assert _ev("((complement even?) 3)") is True


# --- Collection access ---

def test_peek_vector():
    assert _ev("(peek [1 2 3])") == 3
    assert _ev("(peek [])") is None


def test_peek_list():
    assert _ev("(peek '(1 2 3))") == 1


def test_pop_vector():
    assert list(_ev("(pop [1 2 3])")) == [1, 2]


def test_pop_list():
    assert list(_ev("(pop '(1 2 3))")) == [2, 3]


def test_contains_vector_by_index():
    assert _ev("(contains? [10 20 30] 0)") is True
    assert _ev("(contains? [10 20 30] 5)") is False


def test_contains_map_and_set():
    assert _ev("(contains? {:a 1} :a)") is True
    assert _ev("(contains? {:a 1} :b)") is False
    assert _ev("(contains? #{1 2 3} 2)") is True
    assert _ev("(contains? #{1 2 3} 9)") is False


def test_contains_nil():
    assert _ev("(contains? nil :anything)") is False


def test_get_variants():
    assert _ev("(get {:a 1} :a)") == 1
    assert _ev("(get {:a 1} :b)") is None
    assert _ev("(get {:a 1} :b :miss)") == keyword("miss")


def test_dissoc_multi():
    # The result keeps :c, drops :a and :b.
    assert _ev("(get (dissoc {:a 1 :b 2 :c 3} :a :b) :c)") == 3
    assert _ev("(get (dissoc {:a 1 :b 2 :c 3} :a :b) :a)") is None
    assert _ev("(get (dissoc {:a 1 :b 2 :c 3} :a :b) :b)") is None


def test_dissoc_no_keys():
    # (dissoc m) returns m unchanged.
    assert _ev("(get (dissoc {:a 1}) :a)") == 1


def test_disj_multi():
    r = _ev("(disj #{1 2 3 4 5} 2 4)")
    assert set(r) == {1, 3, 5}


def test_find_returns_entry_or_nil():
    r = _ev("(find {:a 1 :b 2} :a)")
    assert r.key == keyword("a")
    assert r.val == 1
    assert _ev("(find {:a 1} :missing)") is None


def test_select_keys():
    r = _ev("(select-keys {:a 1 :b 2 :c 3} [:a :c])")
    assert _ev("(get (select-keys {:a 1 :b 2 :c 3} [:a :c]) :a)") == 1
    assert _ev("(get (select-keys {:a 1 :b 2 :c 3} [:a :c]) :c)") == 3
    assert _ev("(get (select-keys {:a 1 :b 2 :c 3} [:a :c]) :b)") is None


def test_keys_vals_round_trip():
    ks = list(_ev("(keys {:a 1 :b 2 :c 3})"))
    vs = list(_ev("(vals {:a 1 :b 2 :c 3})"))
    assert sorted(k.name for k in ks) == ["a", "b", "c"]
    assert sorted(vs) == [1, 2, 3]


def test_key_val_on_map_entry():
    assert _ev("(key (find {:a 1} :a))") == keyword("a")
    assert _ev("(val (find {:a 1} :a))") == 1


def test_map_entry_pred():
    assert _ev("(map-entry? (find {:a 1} :a))") is True
    assert _ev("(map-entry? [:a 1])") is False
    assert _ev("(map-entry? nil)") is False


# --- Symbol / keyword utilities ---

def test_name_on_sym_kw_string():
    assert _ev("(name 'foo)") == "foo"
    assert _ev("(name 'foo/bar)") == "bar"
    assert _ev("(name :foo)") == "foo"
    assert _ev("(name :ns/foo)") == "foo"
    assert _ev('(name "just-a-string")') == "just-a-string"


def test_namespace():
    assert _ev("(namespace 'foo)") is None
    assert _ev("(namespace 'foo/bar)") == "foo"
    assert _ev("(namespace :foo)") is None
    assert _ev("(namespace :ns/foo)") == "ns"


def test_boolean_coercion():
    assert _ev("(boolean nil)") is False
    assert _ev("(boolean false)") is False
    assert _ev("(boolean 0)") is True
    assert _ev('(boolean "")') is True
    assert _ev("(boolean [])") is True


def test_ident_preds():
    assert _ev("(ident? 'foo)") is True
    assert _ev("(ident? :foo)") is True
    assert _ev('(ident? "foo")') is False
    assert _ev("(ident? 5)") is False


def test_simple_and_qualified_ident():
    assert _ev("(simple-ident? 'foo)") is True
    assert _ev("(simple-ident? 'foo/bar)") is False
    assert _ev("(qualified-ident? 'foo/bar)") is True
    assert _ev("(qualified-ident? 'foo)") is False


def test_simple_and_qualified_symbol():
    assert _ev("(simple-symbol? 'foo)") is True
    assert _ev("(simple-symbol? 'foo/bar)") is False
    assert _ev("(qualified-symbol? 'foo/bar)") is True
    assert _ev("(qualified-symbol? :foo)") is False


def test_simple_and_qualified_keyword():
    assert _ev("(simple-keyword? :foo)") is True
    assert _ev("(simple-keyword? :ns/foo)") is False
    assert _ev("(qualified-keyword? :ns/foo)") is True
    assert _ev("(qualified-keyword? 'foo/bar)") is False


# --- Property-based: keys/vals round-trip via hash-map ---

small_strs = st.text(
    alphabet=st.characters(whitelist_categories=('Lu', 'Ll', 'Nd')),
    min_size=1,
    max_size=10,
).filter(lambda s: s.isidentifier())


@settings(max_examples=50, deadline=None)
@given(pairs=st.lists(
    st.tuples(small_strs, st.integers(-100, 100)),
    min_size=0,
    max_size=10,
    unique_by=lambda t: t[0],
))
def test_keys_vals_round_trip(pairs):
    if not pairs:
        return
    items = " ".join(f":{k} {v}" for k, v in pairs)
    ks = list(_ev(f"(keys (hash-map {items}))"))
    vs = list(_ev(f"(vals (hash-map {items}))"))
    assert sorted(k.name for k in ks) == sorted(k for k, _ in pairs)
    assert sorted(vs) == sorted(v for _, v in pairs)
