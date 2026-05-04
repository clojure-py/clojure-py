"""Tests for core.clj batch 10 (lines 2575-2874):

comp, juxt, partial,
sequence, every?, not-every?, some, not-any?,
dotimes (macro),
map, declare (macro), mapcat, filter, remove,
reduced, reduced?, ensure-reduced
"""

import pytest

import clojure.core  # triggers load

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword, Namespace, RT,
    PersistentVector, Reduced, ISeq,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- comp ----------------------------------------------------------

def test_comp_no_args_is_identity():
    f = E("(clojure.core/comp)")
    assert f(99) == 99

def test_comp_single_fn_passthrough():
    f = E("(clojure.core/comp clojure.core/inc)")
    assert f(5) == 6

def test_comp_two_fns():
    """(comp inc inc) — apply right to left."""
    assert E("((clojure.core/comp clojure.core/inc clojure.core/inc) 5)") == 7

def test_comp_three_fns():
    """((comp str inc inc) 5) → str(inc(inc(5))) → \"7\"."""
    assert E("((clojure.core/comp clojure.core/str clojure.core/inc clojure.core/inc) 5)") == "7"

def test_comp_variadic_args_to_innermost():
    """((comp f g) x y) → (f (g x y))."""
    assert E("((clojure.core/comp clojure.core/inc clojure.core/+) 1 2 3)") == 7


# --- juxt ----------------------------------------------------------

def test_juxt_one_fn():
    assert list(E("((clojure.core/juxt clojure.core/inc) 5)")) == [6]

def test_juxt_two_fns():
    assert list(E("((clojure.core/juxt clojure.core/inc clojure.core/dec) 5)")) == [6, 4]

def test_juxt_three_fns():
    assert list(E(
        "((clojure.core/juxt clojure.core/+ clojure.core/- clojure.core/*) 4 2)"
    )) == [6, 2, 8]

def test_juxt_four_plus_fns():
    assert list(E(
        "((clojure.core/juxt clojure.core/+ clojure.core/- clojure.core/* clojure.core//) 12 3)"
    )) == [15, 9, 36, 4]


# --- partial -------------------------------------------------------

def test_partial_zero_extra():
    f = E("(clojure.core/partial clojure.core/inc)")
    assert f(5) == 6

def test_partial_one_arg():
    f = E("(clojure.core/partial clojure.core/+ 10)")
    assert f(5) == 15
    assert f() == 10

def test_partial_two_args():
    f = E("(clojure.core/partial clojure.core/+ 10 20)")
    assert f(5) == 35
    assert f() == 30

def test_partial_three_args():
    f = E("(clojure.core/partial clojure.core/+ 1 2 3)")
    assert f(4) == 10
    assert f(4 ,5) == 15

def test_partial_four_plus_args():
    f = E("(clojure.core/partial clojure.core/+ 1 2 3 4 5)")
    assert f() == 15
    assert f(10) == 25


# --- sequence ------------------------------------------------------

def test_sequence_on_seq_returns_self():
    s1 = E("'(1 2 3)")
    assert E("(clojure.core/sequence '(1 2 3))").first() == 1

def test_sequence_on_vector_returns_seq():
    s = E("(clojure.core/sequence [1 2 3])")
    assert isinstance(s, ISeq)
    assert list(s) == [1, 2, 3]

def test_sequence_on_nil_yields_empty():
    s = E("(clojure.core/sequence nil)")
    # () empty list — has seq nil
    assert s is not None
    if hasattr(s, "seq"):
        assert s.seq() is None


# --- transducer support via sequence -----------------------------

def test_sequence_with_map_transducer():
    s = E("(clojure.core/sequence (clojure.core/map clojure.core/inc) [1 2 3])")
    assert list(s) == [2, 3, 4]

def test_sequence_with_filter_transducer():
    s = E("(clojure.core/sequence (clojure.core/filter clojure.core/pos?) [-1 1 -2 2 -3 3])")
    assert list(s) == [1, 2, 3]

def test_sequence_with_composed_transducers():
    """((comp (map inc) (filter even?)) ...) — left-to-right composition."""
    s = E("(clojure.core/sequence "
          " (clojure.core/comp (clojure.core/map clojure.core/inc) "
          "                    (clojure.core/filter clojure.core/even?)) "
          " [1 2 3 4 5])")
    assert list(s) == [2, 4, 6]

def test_sequence_with_remove_transducer():
    s = E("(clojure.core/sequence (clojure.core/remove clojure.core/neg?) [-1 1 -2 2])")
    assert list(s) == [1, 2]

def test_sequence_with_empty_input():
    s = E("(clojure.core/sequence (clojure.core/map clojure.core/inc) [])")
    assert list(s) == []


# --- every? / some ------------------------------------------------

def test_every_empty_is_true():
    assert E("(clojure.core/every? clojure.core/pos? [])") is True
    assert E("(clojure.core/every? clojure.core/pos? nil)") is True

def test_every_all_truthy():
    assert E("(clojure.core/every? clojure.core/pos? [1 2 3])") is True

def test_every_one_false():
    assert E("(clojure.core/every? clojure.core/pos? [1 -1 3])") is False

def test_some_returns_first_truthy():
    assert E("(clojure.core/some clojure.core/pos? [-1 -2 3 -4])") is True

def test_some_returns_nil_when_none_match():
    assert E("(clojure.core/some clojure.core/pos? [-1 -2])") is None

def test_some_with_set_as_pred():
    """(some #{:fred} coll) — common idiom."""
    assert E("(clojure.core/some #{:fred} [:a :b :fred :c])") == K("fred")

def test_not_every():
    assert E("(clojure.core/not-every? clojure.core/pos? [1 -1])") is True
    assert E("(clojure.core/not-every? clojure.core/pos? [1 2])") is False

def test_not_any():
    assert E("(clojure.core/not-any? clojure.core/pos? [-1 -2])") is True
    assert E("(clojure.core/not-any? clojure.core/pos? [-1 1])") is False


# --- dotimes ------------------------------------------------------

def test_dotimes_runs_body_n_times():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-c"), counter)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-bump!"),
               lambda i: counter.append(i))
    E("(clojure.core/dotimes [i 5] (user/tcb10-bump! i))")
    assert counter == [0, 1, 2, 3, 4]

def test_dotimes_zero_runs_nothing():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-c2"), counter)
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-b2!"),
               lambda i: counter.append(i))
    E("(clojure.core/dotimes [i 0] (user/tcb10-b2! i))")
    assert counter == []


# --- map ----------------------------------------------------------

def test_map_one_coll():
    assert list(E("(clojure.core/map clojure.core/inc [1 2 3])")) == [2, 3, 4]

def test_map_two_colls():
    assert list(E("(clojure.core/map clojure.core/+ [1 2 3] [10 20 30])")) == [11, 22, 33]

def test_map_three_colls():
    assert list(E(
        "(clojure.core/map clojure.core/+ [1 2 3] [10 20 30] [100 200 300])"
    )) == [111, 222, 333]

def test_map_stops_at_shortest():
    assert list(E("(clojure.core/map clojure.core/+ [1 2 3 4 5] [10 20])")) == [11, 22]

def test_map_empty_coll():
    s = E("(clojure.core/map clojure.core/inc [])")
    assert list(s) == []

def test_map_nil_coll():
    s = E("(clojure.core/map clojure.core/inc nil)")
    assert list(s) == []

def test_map_is_lazy():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-counter"), [0])
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-inc-counted!"),
               lambda x: (Compiler.eval(read_string("user/tcb10-counter")).append(1), x + 1)[1])
    # Don't force the lazy seq
    s = E("(clojure.core/map user/tcb10-inc-counted! '(1 2 3 4 5))")
    counter = Compiler.eval(read_string("user/tcb10-counter"))
    assert len(counter) == 1  # nothing yet (initial 0)


# --- filter / remove ----------------------------------------------

def test_filter_basic():
    assert list(E("(clojure.core/filter clojure.core/pos? [-1 1 -2 2 -3 3])")) == [1, 2, 3]

def test_filter_empty():
    assert list(E("(clojure.core/filter clojure.core/pos? [])")) == []

def test_filter_all_pass():
    assert list(E("(clojure.core/filter clojure.core/pos? [1 2 3])")) == [1, 2, 3]

def test_filter_none_pass():
    assert list(E("(clojure.core/filter clojure.core/pos? [-1 -2 -3])")) == []

def test_remove_basic():
    """remove = filter (complement pred)."""
    assert list(E("(clojure.core/remove clojure.core/neg? [-1 1 -2 2])")) == [1, 2]


# --- mapcat -------------------------------------------------------

def test_mapcat_with_fn_returning_seq():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-rep2"),
               lambda x: [x, x])
    assert list(E("(clojure.core/mapcat user/tcb10-rep2 [1 2 3])")) == [1, 1, 2, 2, 3, 3]

def test_mapcat_with_multiple_colls():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-pair"),
               lambda a, b: [a, b])
    assert list(E("(clojure.core/mapcat user/tcb10-pair [1 2 3] [10 20 30])")) == \
        [1, 10, 2, 20, 3, 30]


# --- declare ------------------------------------------------------

def test_declare_creates_unbound_vars():
    E("(clojure.core/declare tcb10-fwdA tcb10-fwdB)")
    a = E("(var user/tcb10-fwdA)")
    b = E("(var user/tcb10-fwdB)")
    assert a is not None
    assert b is not None
    # Vars are interned but unbound
    assert not a.has_root()
    assert not b.has_root()

def test_declare_marks_declared():
    E("(clojure.core/declare tcb10-fwdC)")
    v = E("(var user/tcb10-fwdC)")
    assert v.meta().val_at(K("declared")) is True


# --- reduced ------------------------------------------------------

def test_reduced_wraps():
    r = E("(clojure.core/reduced 42)")
    assert isinstance(r, Reduced)

def test_reduced_p():
    assert E("(clojure.core/reduced? (clojure.core/reduced 1))") is True
    assert E("(clojure.core/reduced? 1)") is False
    assert E("(clojure.core/reduced? nil)") is False

def test_ensure_reduced_passthrough():
    r = E("(clojure.core/reduced 99)")
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb10-r"), r)
    same = E("(clojure.core/ensure-reduced user/tcb10-r)")
    assert same is r

def test_ensure_reduced_wraps_unreduced():
    r = E("(clojure.core/ensure-reduced 99)")
    assert isinstance(r, Reduced)
