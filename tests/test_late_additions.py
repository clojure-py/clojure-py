"""Tests for the final wave of forms: map ops, transducer extras, vector
variants, tagged-literal, Inst, iteration, typed arrays, etc."""

import datetime
import pytest
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# --- Map ops ---------------------------------------------------------------


def test_get_in():
    assert _ev("(get-in {:a {:b 1}} [:a :b])") == 1
    assert _ev("(get-in {:a {:b 1}} [:a :z])") is None
    assert _ev("(get-in {:a {:b 1}} [:a :z] :default)") == _ev(":default")


def test_assoc_in_creates_intermediate_maps():
    assert _ev("(assoc-in {} [:a :b :c] 42)") == _ev("{:a {:b {:c 42}}}")


def test_assoc_in_overrides():
    assert _ev("(assoc-in {:a {:b 1}} [:a :b] 99)") == _ev("{:a {:b 99}}")


def test_update_in():
    assert _ev("(update-in {:a 1} [:a] inc)") == _ev("{:a 2}")
    assert _ev("(update-in {:a {:b 1}} [:a :b] + 10)") == _ev("{:a {:b 11}}")


def test_update_in_creates_intermediate():
    assert _ev("(update-in {} [:a :b] (fn [_] 5))") == _ev("{:a {:b 5}}")


def test_update():
    assert _ev("(update {:a 1} :a inc)") == _ev("{:a 2}")
    assert _ev("(update {:a 1} :a + 5)") == _ev("{:a 6}")


def test_update_vals():
    assert _ev("(update-vals {:a 1 :b 2} inc)") == _ev("{:a 2 :b 3}")


def test_update_keys():
    # Keys 1, 2 → 2, 3.
    result = _ev("(update-keys {1 :a 2 :b} inc)")
    assert result == _ev("{2 :a 3 :b}")


# --- Vector variants ------------------------------------------------------


def test_splitv_at():
    result = _ev("(splitv-at 2 [1 2 3 4 5])")
    head = result[0]
    tail = list(result[1])
    assert list(head) == [1, 2]
    assert tail == [3, 4, 5]


def test_partitionv():
    parts = [list(p) for p in _ev("(partitionv 2 [1 2 3 4 5])")]
    assert parts == [[1, 2], [3, 4]]


def test_partitionv_all():
    parts = [list(p) for p in _ev("(partitionv-all 2 [1 2 3 4 5])")]
    assert parts == [[1, 2], [3, 4], [5]]


# --- Transducer extras ----------------------------------------------------


def test_eduction_returns_seq():
    result = list(_ev("(eduction (map inc) [1 2 3])"))
    assert result == [2, 3, 4]


def test_halt_when_stops_at_pred():
    # Returns the input that triggered the predicate (3).
    assert _ev("(transduce (halt-when (fn [x] (= x 3))) conj [] [1 2 3 4 5])") == 3


def test_iteration_basic():
    """Build a paginated sequence: each step returns {:val n :next (inc n)} or nil."""
    result = list(_ev(
        "(iteration (fn [k] (when (< k 5) {:val (* k 10) :next (inc k)}))"
        " :somef :val :vf :val :kf :next :initk 0)"
    ))
    assert result == [0, 10, 20, 30, 40]


# --- cast / bytes? / uri? --------------------------------------------------


def test_cast_returns_x_when_compatible():
    import sys
    _ev("(def --intcls nil)")
    sys.modules["clojure.user"].__dict__["--intcls"].bind_root(int)
    assert _ev("(cast --intcls 5)") == 5


def test_cast_throws_when_not_compatible():
    import sys
    _ev("(def --strcls nil)")
    sys.modules["clojure.user"].__dict__["--strcls"].bind_root(str)
    with pytest.raises(TypeError):
        _ev("(cast --strcls 5)")


def test_bytes_pred():
    import sys
    _ev("(def --bs nil)")
    sys.modules["clojure.user"].__dict__["--bs"].bind_root(b"hello")
    assert _ev("(bytes? --bs)") is True
    assert _ev("(bytes? 5)") is False
    assert _ev('(bytes? "hello")') is False


def test_uri_pred_negative():
    assert _ev('(uri? "/tmp")') is False
    assert _ev("(uri? 5)") is False


def test_uri_pred_positive():
    import sys
    from urllib.parse import urlparse
    _ev("(def --uri nil)")
    sys.modules["clojure.user"].__dict__["--uri"].bind_root(urlparse("http://example.com"))
    assert _ev("(uri? --uri)") is True


# --- Tagged literal / reader conditional ---------------------------------


def test_tagged_literal_construct():
    tl = _ev("(tagged-literal (quote foo) 42)")
    assert _ev("(.-tag --tl)") if False else True  # smoke
    assert tl.tag == _ev("(quote foo)")
    assert tl.form == 42


def test_tagged_literal_pred():
    assert _ev("(tagged-literal? (tagged-literal (quote foo) 42))") is True
    assert _ev("(tagged-literal? 42)") is False


def test_reader_conditional_construct():
    rc = _ev("(reader-conditional [1 2] false)")
    assert rc.splicing is False


def test_reader_conditional_pred():
    assert _ev("(reader-conditional? (reader-conditional [1 2] false))") is True
    assert _ev("(reader-conditional? [1 2])") is False


# --- Inst protocol --------------------------------------------------------


def test_inst_pred_on_datetime():
    import sys
    _ev("(def --now nil)")
    sys.modules["clojure.user"].__dict__["--now"].bind_root(
        datetime.datetime(2026, 1, 1, 0, 0, 0)
    )
    # inst? still uses the RT shim — only checks Python's datetime.
    assert _ev("(inst? --now)") is True


def test_inst_ms_via_protocol():
    import sys
    _ev("(def --d nil)")
    sys.modules["clojure.user"].__dict__["--d"].bind_root(
        datetime.datetime(1970, 1, 1, 0, 0, 0, tzinfo=datetime.timezone.utc)
    )
    assert _ev("(inst-ms --d)") == 0


# --- ns-imports / with-precision / seque / agent setters / load / test ---


def test_ns_imports_returns_empty_map():
    assert _ev("(ns-imports (find-ns 'clojure.user))") == _ev("{}")


def test_with_precision_doesnt_throw():
    # The dynamic *math-context* binding is set; arithmetic still happens
    # in plain Python types so it has no observable effect on basic ops.
    assert _ev("(with-precision 5 (* 1 2))") == 2


def test_seque_returns_passthrough():
    assert list(_ev("(seque [1 2 3])")) == [1, 2, 3]


def test_set_agent_executors_no_op():
    assert _ev("(set-agent-send-executor! nil)") is None
    assert _ev("(set-agent-send-off-executor! nil)") is None


def test_test_with_no_test_meta():
    assert _ev("(test #'+)") == _ev(":no-test")


def test_compile_stub():
    assert _ev("(compile (quote some-lib))") is None


def test_add_classpath_stub():
    assert _ev('(add-classpath "/x")') is None


# --- Auto-promote arithmetic aliases --------------------------------------


def test_auto_promote_aliases():
    assert _ev("(+' 1 2)") == 3
    assert _ev("(-' 5 2)") == 3
    assert _ev("(*' 3 4)") == 12
    assert _ev("(inc' 5)") == 6
    assert _ev("(dec' 5)") == 4


# --- Typed arrays ---------------------------------------------------------


def test_boolean_array_size_only():
    assert list(_ev("(boolean-array 3)")) == [None, None, None]


def test_boolean_array_from_seq():
    assert list(_ev("(boolean-array [1 2 3])")) == [1, 2, 3]


def test_int_array_with_init():
    assert list(_ev("(int-array 4 7)")) == [7, 7, 7, 7]


def test_double_array_alias():
    assert list(_ev("(double-array [1.5 2.5])")) == [1.5, 2.5]


# --- definline ------------------------------------------------------------


def test_definline_works_like_defn():
    _ev("(definline def-inl-x [n] (* n 3))")
    assert _ev("(def-inl-x 5)") == 15


# --- xml-seq --------------------------------------------------------------


def test_xml_seq_walks_elements():
    import sys
    import xml.etree.ElementTree as ET
    root = ET.fromstring("<a><b/><c><d/></c></a>")
    _ev("(def --xml-root nil)")
    sys.modules["clojure.user"].__dict__["--xml-root"].bind_root(root)
    tags = list(_ev("(map (fn [e] (.-tag e)) (xml-seq --xml-root))"))
    assert tags == ["a", "b", "c", "d"]


# --- clojure-version / version utility -----------------------------------


def test_clojure_version_returns_string():
    s = _ev("(clojure-version)")
    assert isinstance(s, str)
    assert s.startswith("1.")


def test_clojure_version_var_is_a_map():
    v = _ev("*clojure-version*")
    # Has at least :major, :minor.
    assert _ev("(:major *clojure-version*)") == 1


def test_seq_to_map_for_destructuring_pairs():
    assert _ev("(seq-to-map-for-destructuring [:a 1 :b 2])") == _ev("{:a 1 :b 2}")


def test_seq_to_map_for_destructuring_singleton_map():
    # When the seq has exactly one element and that element is a map,
    # use it directly.
    assert _ev("(seq-to-map-for-destructuring [{:a 1}])") == _ev("{:a 1}")


def test_with_loading_context_passthrough():
    assert _ev("(with-loading-context (+ 1 2))") == 3
