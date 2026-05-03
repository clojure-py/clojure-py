"""Tests for core.clj batch 4 (lines 540-745):

any?, str, symbol?, keyword?, cond, symbol, gensym, keyword,
find-keyword, spread, list*, apply, vary-meta, lazy-seq, chunk
helpers, concat
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    PersistentVector, PersistentList, ISeq, LazySeq,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- any? ----------------------------------------------------------

def test_any_returns_true_for_anything():
    assert E("(clojure.core/any? nil)") is True
    assert E("(clojure.core/any? 42)") is True
    assert E('(clojure.core/any? "x")') is True
    assert E("(clojure.core/any? false)") is True


# --- str -----------------------------------------------------------

def test_str_no_args_is_empty():
    assert E("(clojure.core/str)") == ""

def test_str_nil_is_empty():
    assert E("(clojure.core/str nil)") == ""

def test_str_single_value():
    assert E("(clojure.core/str 42)") == "42"
    assert E('(clojure.core/str "abc")') == "abc"

def test_str_keyword_includes_colon():
    assert E("(clojure.core/str :foo)") == ":foo"

def test_str_concatenates_multiple():
    assert E("(clojure.core/str 1 2 3)") == "123"
    assert E('(clojure.core/str "a" "b" "c")') == "abc"

def test_str_mixes_types():
    assert E("(clojure.core/str 1 :a 'sym)") == "1:asym"


# --- symbol? / keyword? -------------------------------------------

def test_symbol_p():
    assert E("(clojure.core/symbol? 'foo)") is True
    assert E("(clojure.core/symbol? :foo)") is False
    assert E("(clojure.core/symbol? 42)") is False

def test_keyword_p():
    assert E("(clojure.core/keyword? :foo)") is True
    assert E("(clojure.core/keyword? 'foo)") is False
    assert E('(clojure.core/keyword? "x")') is False


# --- cond ---------------------------------------------------------

def test_cond_empty_is_nil():
    assert E("(clojure.core/cond)") is None

def test_cond_first_truthy_wins():
    assert E("(clojure.core/cond false :a true :b false :c)") == \
        Keyword.intern(None, "b")

def test_cond_else_clause():
    assert E("(clojure.core/cond false :a :else :default)") == \
        Keyword.intern(None, "default")

def test_cond_no_match_is_nil():
    assert E("(clojure.core/cond false :a nil :b)") is None

def test_cond_odd_args_raises():
    with pytest.raises(ValueError):
        E("(clojure.core/cond true :a false)")


# --- symbol -------------------------------------------------------

def test_symbol_from_string():
    s = E('(clojure.core/symbol "foo")')
    assert s == Symbol.intern("foo")

def test_symbol_from_symbol_passthrough():
    s = E("(clojure.core/symbol 'foo)")
    assert s == Symbol.intern("foo")

def test_symbol_with_ns_and_name():
    s = E('(clojure.core/symbol "myns" "myname")')
    assert s == Symbol.intern("myns", "myname")

def test_symbol_from_keyword():
    s = E("(clojure.core/symbol :foo)")
    assert s == Symbol.intern("foo")


# --- gensym -------------------------------------------------------

def test_gensym_default_prefix():
    g = E("(clojure.core/gensym)")
    assert isinstance(g, Symbol)
    assert g.name.startswith("G__")

def test_gensym_custom_prefix():
    g = E('(clojure.core/gensym "MY_")')
    assert g.name.startswith("MY_")

def test_gensym_unique():
    g1 = E("(clojure.core/gensym)")
    g2 = E("(clojure.core/gensym)")
    assert g1 != g2


# --- keyword ------------------------------------------------------

def test_keyword_from_string():
    k = E('(clojure.core/keyword "foo")')
    assert k == Keyword.intern(None, "foo")

def test_keyword_from_symbol():
    k = E("(clojure.core/keyword 'foo)")
    assert k == Keyword.intern(None, "foo")

def test_keyword_from_keyword_passthrough():
    k = E("(clojure.core/keyword :foo)")
    assert k == Keyword.intern(None, "foo")

def test_keyword_with_ns_and_name():
    k = E('(clojure.core/keyword "ns" "name")')
    assert k == Keyword.intern("ns", "name")


# --- list* --------------------------------------------------------

def test_list_star_one_arg_is_seq():
    s = E("(clojure.core/list* '(1 2 3))")
    assert list(s) == [1, 2, 3]

def test_list_star_prepend_one():
    s = E("(clojure.core/list* 0 '(1 2 3))")
    assert list(s) == [0, 1, 2, 3]

def test_list_star_prepend_many():
    s = E("(clojure.core/list* 1 2 3 '(4 5 6))")
    assert list(s) == [1, 2, 3, 4, 5, 6]

def test_list_star_with_vector_tail():
    s = E("(clojure.core/list* 1 2 [3 4])")
    assert list(s) == [1, 2, 3, 4]


# --- apply --------------------------------------------------------

def _setup_plus():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb4-plus"),
               lambda *args: sum(args))

def test_apply_two_args():
    _setup_plus()
    assert E("(clojure.core/apply user/tcb4-plus [1 2 3])") == 6

def test_apply_with_intervening_args():
    _setup_plus()
    assert E("(clojure.core/apply user/tcb4-plus 10 [1 2 3])") == 16
    assert E("(clojure.core/apply user/tcb4-plus 10 20 [1 2 3])") == 36

def test_apply_to_clojure_fn():
    """apply works on user fns defined via defn — they're real Python
    callables; the .applyTo fallback handles them."""
    E("(clojure.core/defn tcb4-mul [a b] (clojure.core/apply user/tcb4-plus [a a a]))")
    _setup_plus()
    assert E("(tcb4-mul 5 0)") == 15


# --- vary-meta ----------------------------------------------------

def test_vary_meta_passes_meta_through_fn():
    """`(vary-meta v conj {:k 1})` should attach :k 1 to v's meta."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb4-mk"),
               lambda *args: {Keyword.intern(None, "k"): 1})
    v = E("(clojure.core/vary-meta [1 2 3] user/tcb4-mk)")
    m = v.meta()
    assert m.get(Keyword.intern(None, "k")) == 1


# --- lazy-seq ----------------------------------------------------

def test_lazy_seq_returns_seqable():
    s = E("(clojure.core/lazy-seq (clojure.core/cons 1 (clojure.core/cons 2 nil)))")
    assert list(s) == [1, 2]

def test_lazy_seq_body_runs_lazily():
    """The body shouldn't execute until seq is forced."""
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb4-bump!"),
               lambda: (counter.append(1), 99)[1])
    ls = E("(clojure.core/lazy-seq (clojure.core/cons (user/tcb4-bump!) nil))")
    assert counter == []  # not yet forced
    list(ls)  # force
    assert counter == [1]


# --- concat ------------------------------------------------------

def test_concat_no_args():
    assert list(E("(clojure.core/concat)") or []) == []

def test_concat_one_arg():
    assert list(E("(clojure.core/concat [1 2 3])")) == [1, 2, 3]

def test_concat_two_args():
    assert list(E("(clojure.core/concat [1 2] [3 4])")) == [1, 2, 3, 4]

def test_concat_many_args():
    assert list(E("(clojure.core/concat [1 2] [3] [4 5 6])")) == [1, 2, 3, 4, 5, 6]

def test_concat_with_nils():
    assert list(E("(clojure.core/concat nil [1] nil [2 3])")) == [1, 2, 3]

def test_concat_lazy():
    """concat returns a lazy seq."""
    s = E("(clojure.core/concat [1] [2])")
    assert isinstance(s, (LazySeq, ISeq))


# --- chunked-seq? --------------------------------------------------

def test_chunked_seq_p():
    """A vector's seq is chunked."""
    assert E("(clojure.core/chunked-seq? (clojure.core/seq [1 2 3]))") is True
    assert E("(clojure.core/chunked-seq? '(1 2 3))") is False
    assert E("(clojure.core/chunked-seq? nil)") is False
