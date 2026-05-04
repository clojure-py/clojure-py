"""Tests for core.clj batch 25 (selected from JVM 4807-5003): print-str,
assert, test, rand/rand-int, defn-, tree-seq.

Forms (8 + 1 var + 1 alias):
  print-str, println-str,
  *assert* (dynamic var), assert (macro),
  test,
  rand, rand-int,
  defn- (macro),
  tree-seq.
  AssertionError type alias added to the host-class header.

Skipped — saved for focused follow-up batches:
  ExceptionInfo / IExceptionInfo block (ex-info, ex-data, ex-message,
    ex-cause, elide-top-frames) — needs an ExceptionInfo Python class
    and Python-traceback adapter.
  re-* family (re-pattern, re-matcher, re-groups, re-seq, re-matches,
    re-find) — needs a JavaMatcher state-machine wrapper because
    Python's re uses stateless Match objects.

One adaptation from JVM source:
  rand uses (py.random/random) where JVM uses (. Math (random)) —
  Python's math module has no random attribute (random lives in its
  own module).
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- print-str / println-str -------------------------------------

def test_print_str_strings_unquoted():
    """print drops *print-readably*, so strings render without quotes."""
    assert E('(print-str "hello")') == "hello"

def test_print_str_multiple_args_space_separated():
    """print uses *out*'s append space — same as pr."""
    out = E("(print-str 1 2 3)")
    assert out == "1 2 3"

def test_print_str_no_args():
    assert E("(print-str)") == ""

def test_println_str_appends_newline():
    assert E('(println-str "hi")') == "hi\n"

def test_println_str_strings_unquoted():
    assert E('(println-str "hi" "bye")') == "hi bye\n"


# --- *assert* ----------------------------------------------------

def test_star_assert_default_true():
    val = E("(clojure.core/var *assert*)").deref()
    assert val is True


# --- assert ------------------------------------------------------

def test_assert_passing_returns_nil():
    assert E("(assert true)") is None
    assert E("(assert (= 1 1))") is None

def test_assert_failure_throws_with_default_message():
    with pytest.raises(AssertionError, match="Assert failed"):
        E("(assert false)")

def test_assert_failure_throws_with_custom_message():
    with pytest.raises(AssertionError, match="custom-message"):
        E('(assert false "custom-message")')

def test_assert_message_includes_form():
    """The default message includes the original form via pr-str."""
    try:
        E("(assert (= 1 2))")
        assert False, "should have thrown"
    except AssertionError as e:
        assert "(= 1 2)" in str(e)

def test_assert_short_circuits_when_var_false():
    """When *assert* is false at expansion time, assert vanishes."""
    out = E("""(binding [*assert* false]
                 (clojure.core/eval '(do (assert false) :got-here)))""")
    assert out == K("got-here")


# --- test --------------------------------------------------------

def test_test_returns_no_test_when_no_test_meta():
    out = E("(test (clojure.core/var +))")
    assert out == K("no-test")

def test_test_invokes_test_fn_in_meta():
    """When the var has a :test fn in its metadata, test calls it
    and returns :ok. Use alter-meta! to attach an evaluated fn —
    inline ^{:test (fn …)} stores the form, not the fn."""
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb25-touch!"),
               lambda: counter.append(1))
    E("(def tcb25-with-test 42)")
    E("(alter-meta! (var tcb25-with-test) assoc :test "
      "             (fn [] (user/tcb25-touch!)))")
    out = E("(test (var tcb25-with-test))")
    assert out == K("ok")
    assert sum(counter) == 1


# --- rand / rand-int ---------------------------------------------

def test_rand_default_range():
    """No-arg rand returns a float in [0, 1)."""
    for _ in range(20):
        v = E("(rand)")
        assert isinstance(v, float)
        assert 0.0 <= v < 1.0

def test_rand_with_n():
    for _ in range(20):
        v = E("(rand 10)")
        assert 0.0 <= v < 10.0

def test_rand_int_in_range():
    for _ in range(50):
        v = E("(rand-int 10)")
        assert isinstance(v, int)
        assert 0 <= v < 10

def test_rand_int_with_one():
    """(rand-int 1) is always 0."""
    for _ in range(10):
        assert E("(rand-int 1)") == 0


# --- defn- -------------------------------------------------------

def test_defn_dash_creates_var():
    E("(defn- tcb25-pdash [x] (* x 2))")
    assert E("(tcb25-pdash 21)") == 42

def test_defn_dash_marks_private():
    E("(defn- tcb25-pdash2 [x] x)")
    out = E("(:private (clojure.core/meta (clojure.core/var user/tcb25-pdash2)))")
    assert out is True

def test_defn_dash_excluded_from_ns_publics():
    """ns-publics filters out private vars."""
    E("(defn- tcb25-pdash3 [] :hidden)")
    publics = E("(clojure.core/ns-publics (quote user))")
    keys_str = {str(e.key()) for e in publics}
    assert "tcb25-pdash3" not in keys_str


# --- tree-seq ----------------------------------------------------

def test_tree_seq_flat_collection_yields_one_node():
    """Leaf node: only the root is visited."""
    out = list(E("(tree-seq (constantly false) identity 42)"))
    assert out == [42]

def test_tree_seq_simple_tree():
    """Root then left-to-right depth-first."""
    out = list(E("(tree-seq vector? identity [[1 2] [3 4]])"))
    # root + walk into [1 2] → 1, 2; then [3 4] → 3, 4
    # vector? on integers is false, so they're leaves.
    # Expected order: [[1 2] [3 4]], [1 2], 1, 2, [3 4], 3, 4
    assert len(out) == 7
    assert out[0] == E("[[1 2] [3 4]]")
    assert out[2] == 1

def test_tree_seq_lazy():
    """tree-seq is lazy — should not realize the whole tree if only
    head is requested."""
    out = E("(first (tree-seq (constantly true) identity (clojure.core/range 1000000)))")
    # The root is the range itself, returned lazily.
    assert out is not None

def test_tree_seq_nested_maps():
    """Use map-as-tree where children are vals of the map."""
    out = list(E("""
      (tree-seq
        clojure.core/map?
        clojure.core/vals
        {:a 1 :b {:c 2 :d {:e 3}}})"""))
    # All map levels + leaf values, depth-first.
    # Without specific ordering guarantees on map iteration,
    # just verify length + that all expected nodes appear.
    leaf_vals = [n for n in out if isinstance(n, int)]
    assert set(leaf_vals) == {1, 2, 3}


# --- AssertionError alias ----------------------------------------

def test_assertion_error_resolves_to_python_assertionerror():
    """JVM has java.lang.AssertionError; we alias to Python's."""
    out = E("clojure.core/AssertionError")
    assert out is AssertionError
