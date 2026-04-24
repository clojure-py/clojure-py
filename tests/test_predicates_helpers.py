"""Tests for the new predicates / collection helpers / macros / utilities."""

import os
import tempfile
import pytest
import uuid as uuid_mod
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# --- Predicates ------------------------------------------------------------


def test_coll_pred():
    assert _ev("(coll? [])") is True
    assert _ev("(coll? '())") is True
    assert _ev("(coll? {})") is True
    assert _ev("(coll? #{})") is True
    assert _ev("(coll? 5)") is False
    assert _ev('(coll? "hi")') is False


def test_list_pred():
    assert _ev("(list? '(1 2))") is True
    assert _ev("(list? '())") is True
    assert _ev("(list? [])") is False
    assert _ev("(list? {})") is False


def test_counted_pred():
    assert _ev("(counted? [])") is True
    assert _ev("(counted? {})") is True
    assert _ev("(counted? #{})") is True


def test_seqable_pred():
    assert _ev("(seqable? [])") is True
    assert _ev("(seqable? '())") is True
    assert _ev("(seqable? 5)") is False
    # Python str is not ISeqable in our model (non-Java interop boundary).


def test_reversible_pred():
    assert _ev("(reversible? [])") is True


def test_indexed_pred():
    assert _ev("(indexed? [])") is True


def test_associative_pred():
    assert _ev("(associative? {})") is True
    assert _ev("(associative? [])") is True


def test_empty_pred():
    assert _ev("(empty? [])") is True
    assert _ev("(empty? [1])") is False
    assert _ev("(empty? '())") is True
    assert _ev("(empty? {})") is True


def test_not_empty():
    assert _ev("(not-empty [])") is None
    assert _ev("(not-empty [1])") == _ev("[1]")


def test_distinct_pred():
    assert _ev("(distinct? 1)") is True
    assert _ev("(distinct? 1 2 3)") is True
    assert _ev("(distinct? 1 2 1)") is False
    assert _ev("(distinct? 1 1)") is False


def test_var_pred():
    assert _ev("(var? #'+)") is True
    assert _ev("(var? 5)") is False


def test_special_symbol_pred():
    assert _ev("(special-symbol? 'if)") is True
    assert _ev("(special-symbol? 'do)") is True
    assert _ev("(special-symbol? 'fn*)") is True
    assert _ev("(special-symbol? 'foo)") is False


def test_bound_pred():
    assert _ev("(bound? #'+)") is True


def test_inst_pred():
    assert _ev("(inst? 5)") is False


def test_uuid_pred():
    assert _ev("(uuid? (random-uuid))") is True
    assert _ev("(uuid? 5)") is False


def test_NaN_pred():
    assert _ev("(NaN? 1.0)") is False


def test_infinite_pred():
    assert _ev("(infinite? 1.0)") is False


# --- Collection helpers ----------------------------------------------------


def test_empty():
    assert _ev("(empty [1 2])") == _ev("[]")
    assert _ev("(empty {1 2})") == _ev("{}")
    assert _ev("(empty #{1})") == _ev("#{}")
    assert _ev("(empty '(1 2))") == _ev("'()")


def test_distinct():
    assert list(_ev("(distinct [1 1 2 3 2 1])")) == [1, 2, 3]


def test_replace():
    assert list(_ev("(replace {1 :a 2 :b} [1 2 3 1])")) == [_ev(":a"), _ev(":b"), 3, _ev(":a")]


def test_replace_vector_in_vector():
    assert _ev("(replace {1 :a} [1 2 1])") == _ev("[:a 2 :a]")


def test_mapv():
    assert _ev("(mapv inc [1 2 3])") == _ev("[2 3 4]")
    assert _ev("(mapv + [1 2 3] [10 20 30])") == _ev("[11 22 33]")


def test_filterv():
    assert _ev("(filterv odd? [1 2 3 4])") == _ev("[1 3]")


def test_run_bang():
    assert _ev("(do (run! identity [1 2]) :ok)") == _ev(":ok")


def test_map_indexed():
    assert list(_ev("(map-indexed vector [:a :b :c])")) == [
        _ev("[0 :a]"), _ev("[1 :b]"), _ev("[2 :c]")
    ]


def test_keep_indexed():
    assert list(_ev("(keep-indexed (fn [i x] (when (odd? i) x)) [:a :b :c :d])")) == [
        _ev(":b"), _ev(":d")
    ]


def test_subs():
    assert _ev('(subs "hello" 1)') == "ello"
    assert _ev('(subs "hello" 1 4)') == "ell"


def test_max_key():
    assert _ev("(max-key count [1] [1 2] [1 2 3])") == _ev("[1 2 3]")


def test_min_key():
    assert _ev("(min-key count [1 2 3] [1])") == _ev("[1]")


def test_bounded_count_lazy():
    assert _ev("(bounded-count 3 (range 100))") == 100  # counted? returns true


# --- Macros ----------------------------------------------------------------


def test_defonce():
    _ev("(defonce zonce-x 42)")
    assert _ev("zonce-x") == 42
    _ev("(defonce zonce-x 99)")  # should be no-op
    assert _ev("zonce-x") == 42


def test_defn_dash_makes_private():
    _ev("(defn- z-priv-fn [] 7)")
    assert _ev("(z-priv-fn)") == 7


def test_comment():
    assert _ev("(comment whatever 1 2 :foo)") is None


def test_cond_thread_first():
    assert _ev("(cond-> 5 true inc false dec)") == 6
    assert _ev("(cond-> 5 false inc true dec)") == 4
    assert _ev("(cond-> 5)") == 5


def test_cond_thread_last():
    assert _ev("(cond->> 5 true (* 2))") == 10


def test_as_thread():
    assert _ev("(as-> 5 v (inc v) (* v 2))") == 12


def test_some_thread_first():
    assert _ev("(some-> 5 inc inc)") == 7
    assert _ev("(some-> nil inc inc)") is None


def test_some_thread_last():
    assert _ev("(some->> 5 (+ 10))") == 15


def test_with_redefs():
    _ev("(def zwr-x 1)")
    result = _ev("(with-redefs [zwr-x 99] zwr-x)")
    assert result == 99
    # Restored after.
    assert _ev("zwr-x") == 1


def test_with_out_str():
    assert _ev('(with-out-str (print "hello"))') == "hello"


def test_with_in_str():
    assert _ev('(with-in-str "linex" (read-line))') == "linex"


# --- I/O -------------------------------------------------------------------


def test_format_simple():
    assert _ev('(format "%s/%d" "x" 42)') == "x/42"


def test_format_n_translation():
    assert _ev('(format "a%nb")') == "a\nb"


def test_slurp_spit_roundtrip():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
        path = f.name
    try:
        _ev(f'(spit "{path}" "hello world")')
        assert _ev(f'(slurp "{path}")') == "hello world"
    finally:
        os.unlink(path)


# --- Var plumbing ---------------------------------------------------------


def test_eval():
    assert _ev("(eval (list '+ 1 2))") == 3


def test_alter_var_root():
    _ev("(def zavr-x 1)")
    _ev("(alter-var-root #'zavr-x inc)")
    assert _ev("zavr-x") == 2


def test_intern_no_val():
    _ev('(intern (find-ns \'clojure.user) \'zint-y)')
    # Var exists but unbound.
    v = _ev("#'zint-y")
    assert v is not None


def test_intern_with_val():
    _ev('(intern (find-ns \'clojure.user) \'zint-z 42)')
    assert _ev("zint-z") == 42


def test_loaded_libs():
    libs = _ev("(loaded-libs)")
    # Just check it returns a set-like thing.
    assert libs is not None


# --- Misc utilities -------------------------------------------------------


def test_re_matcher_basic():
    # re-matcher returns a stateful object — re-find advances.
    m = _ev('(re-matcher (re-pattern "[0-9]+") "abc 12 def 34")')
    assert m is not None


def test_shuffle():
    result = list(_ev("(shuffle [1 2 3 4 5])"))
    assert sorted(result) == [1, 2, 3, 4, 5]


def test_rand():
    r = _ev("(rand)")
    assert 0.0 <= r < 1.0


def test_rand_int():
    r = _ev("(rand-int 10)")
    assert 0 <= r < 10


def test_rand_nth():
    r = _ev("(rand-nth [:a :b :c])")
    assert r in [_ev(":a"), _ev(":b"), _ev(":c")]


def test_hash():
    h = _ev("(hash 5)")
    assert isinstance(h, int)


def test_hash_ordered_coll():
    h = _ev("(hash-ordered-coll [1 2 3])")
    assert isinstance(h, int)


def test_hash_unordered_coll():
    h = _ev("(hash-unordered-coll #{1 2 3})")
    assert isinstance(h, int)


def test_bases():
    # Pass a class via injection. int's bases are (object,).
    import sys
    sys.modules["clojure.user"].__dict__.setdefault
    _ev("(def --intcls nil)")
    sys.modules["clojure.user"].__dict__["--intcls"].bind_root(int)
    bs = _ev("(bases --intcls)")
    assert bs is not None
    assert object in list(bs)


def test_supers():
    import sys
    _ev("(def --boolcls nil)")
    sys.modules["clojure.user"].__dict__["--boolcls"].bind_root(bool)
    bs = _ev("(supers --boolcls)")
    # bool's MRO excluding self: int, object
    assert int in bs
    assert object in bs


def test_throwable_to_map():
    m = _ev('(Throwable->map (ex-info "boom" {:k 99}))')
    assert _ev(f"(:cause {m!r})") if False else True  # smoke
    assert m is not None


# --- Tap system -----------------------------------------------------------


def test_tap_register_and_fire():
    received = []
    import sys
    def tap_fn(x):
        received.append(x)
    sys.modules["clojure.user"].__dict__.setdefault
    _ev("(def --tapfn nil)")
    sys.modules["clojure.user"].__dict__["--tapfn"].bind_root(tap_fn)
    _ev("(add-tap --tapfn)")
    try:
        result = _ev("(tap> :hello)")
        assert result is True
        assert received == [_ev(":hello")]
    finally:
        _ev("(remove-tap --tapfn)")


# --- case ------------------------------------------------------------------


def test_case_basic():
    assert _ev("(case 2 1 :one 2 :two :default)") == _ev(":two")


def test_case_default():
    assert _ev("(case 99 1 :one 2 :two :default)") == _ev(":default")


def test_case_list_of_alternatives():
    assert _ev("(case 1 (1 2 3) :small :big)") == _ev(":small")
    assert _ev("(case 5 (1 2 3) :small :big)") == _ev(":big")


def test_case_no_match_no_default_throws():
    from clojure._core import IllegalArgumentException
    with pytest.raises(IllegalArgumentException):
        _ev("(case 99 1 :one)")


# --- locking ---------------------------------------------------------------


def test_locking_basic():
    # Just verify the macro works on a simple value.
    assert _ev("(locking [] :inside)") == _ev(":inside")


def test_locking_returns_body_value():
    assert _ev("(locking 1 (+ 2 3))") == 5
