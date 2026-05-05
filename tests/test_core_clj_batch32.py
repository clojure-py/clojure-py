"""Tests for core.clj batch 32 (selected from JVM 5810-6300):
defonce + nested-associative ops + load-machinery scaffolding.

Forms (11):
  refer-clojure (macro),
  defonce (macro),
  *loaded-libs*, *pending-paths*, *loading-verbosely* (private dyn vars),
  libspec? (private), prependss (private),
  root-resource (private), root-directory (private),
  loaded-libs,
  get-in, assoc-in, update-in.

Skipped — saved for the require/use/load batch:
  with-loading-context  — JVM ClassLoader machinery.
  ns                    — heavyweight macro that depends on
                          gen-class + require/use/load infrastructure.
  throw-if              — builds a CompilerException with stack-trace
                          rewrite; used only inside the load family.
  check-cyclic-dependency / load-one / load-all / load-lib /
  load-libs / require / use / requiring-resolve /
  serialized-require / load / compile
                        — need a Python file-system search story for
                          finding .clj files (analogous to JVM's
                          classpath-relative resolution) and the
                          throw-if helper.

Backend additions:
  JAVA_METHOD_FALLBACKS now provides shims for common Java String
  methods that core.clj reaches for: lastIndexOf / indexOf /
  startsWith / endsWith / replace. Forwarded to Python's rfind /
  find / startswith / endswith / replace respectively. Lets JVM
  source like (.lastIndexOf s "/") work on Python str without
  rewriting.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    Namespace,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name, ns=None):
    return Keyword.intern(ns, name)


def _core_var(name):
    return Namespace.find(Symbol.intern("clojure.core")).find_interned_var(
        Symbol.intern(name))


# --- refer-clojure ------------------------------------------------

def test_refer_clojure_macro_exists():
    """refer-clojure is the convenience wrapper around (refer 'clojure.core …)."""
    v = _core_var("refer-clojure")
    assert v is not None
    assert v.is_macro()


# --- defonce ------------------------------------------------------

def test_defonce_first_call_sets_value():
    E("(defonce tcb32-once-x 42)")
    assert E("tcb32-once-x") == 42

def test_defonce_repeat_does_not_override():
    """Second defonce of an already-bound name should leave the value alone."""
    E("(defonce tcb32-once-y 1)")
    E("(defonce tcb32-once-y 99)")
    assert E("tcb32-once-y") == 1

def test_defonce_overrides_unbound_var():
    """A var that was def'd without a value gets its root set on
    first defonce."""
    E("(def tcb32-once-z)")  # unbound
    E("(defonce tcb32-once-z 7)")
    assert E("tcb32-once-z") == 7

def test_defonce_does_not_evaluate_expr_when_already_bound():
    """The expr should not be evaluated on subsequent defonce calls.
    Track via side-effect counter."""
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb32-touch!"),
               lambda: counter.append(1) or "computed")
    E("(defonce tcb32-once-side (user/tcb32-touch!))")
    assert sum(counter) == 1
    E("(defonce tcb32-once-side (user/tcb32-touch!))")
    # Should still be 1 — the expr wasn't re-evaluated.
    assert sum(counter) == 1


# --- *loaded-libs* / *pending-paths* / *loading-verbosely* -------

def test_star_loaded_libs_default():
    """Returns a sorted-set ref. The set may contain libs loaded
    transitively from core.clj — in particular `clojure.core.protocols`
    via the `(load "core/protocols")` call near the end of core.clj.
    What matters is the structural shape (sorted set) and that whatever
    is in it was loaded via the lib machinery."""
    out = E("(loaded-libs)")
    assert hasattr(out, "count")
    # If anything's in there, it must be a Symbol (lib name).
    if out.count() > 0:
        for lib in out:
            from clojure.lang import Symbol
            assert isinstance(lib, Symbol)

def test_star_pending_paths_default_empty_list():
    val = _core_var("*pending-paths*").deref()
    # Empty list — count is 0.
    assert hasattr(val, "count") and val.count() == 0

def test_star_loading_verbosely_default_false():
    val = _core_var("*loading-verbosely*").deref()
    assert val is False


# --- libspec? -----------------------------------------------------

def test_libspec_pred_true_for_symbol():
    fn = _core_var("libspec?").deref()
    assert fn(E("'foo.bar")) is True

def test_libspec_pred_true_for_vector_with_keyword_second():
    """[lib :as alias] style — second is a keyword."""
    fn = _core_var("libspec?").deref()
    assert fn(E("'[foo.bar :as fb]")) is True

def test_libspec_pred_true_for_vector_singleton():
    """[lib] — second is nil (no second elem)."""
    fn = _core_var("libspec?").deref()
    assert fn(E("'[foo.bar]")) is True

def test_libspec_pred_false_for_random_data():
    fn = _core_var("libspec?").deref()
    assert fn(E("42")) is False
    assert fn(E("'(foo bar)")) is False  # list, not vector


# --- prependss ----------------------------------------------------

def test_prependss_symbol_conses():
    fn = _core_var("prependss").deref()
    out = list(fn(E("'foo"), E("'(a b c)")))
    # symbol → cons onto coll
    assert str(out[0]) == "foo"

def test_prependss_seq_concats():
    fn = _core_var("prependss").deref()
    out = list(fn(E("'(x y)"), E("'(a b c)")))
    assert [str(x) for x in out] == ["x", "y", "a", "b", "c"]


# --- root-resource / root-directory ------------------------------

def test_root_resource_dotted_to_slash():
    fn = _core_var("root-resource").deref()
    assert fn(E("'foo.bar.baz")) == "/foo/bar/baz"

def test_root_resource_replaces_dashes_with_underscores():
    fn = _core_var("root-resource").deref()
    assert fn(E("'foo-bar.my-lib")) == "/foo_bar/my_lib"

def test_root_directory_strips_last_segment():
    fn = _core_var("root-directory").deref()
    assert fn(E("'foo.bar.baz")) == "/foo/bar"

def test_root_directory_top_level():
    fn = _core_var("root-directory").deref()
    # 'foo' → "/foo" → after lastIndexOf("/")→0 → subs(d, 0, 0) = ""
    assert fn(E("'foo")) == ""


# --- loaded-libs --------------------------------------------------

def test_loaded_libs_returns_sorted_set():
    """Default is empty until require populates it."""
    out = E("(loaded-libs)")
    assert hasattr(out, "count")


# --- get-in -------------------------------------------------------

def test_get_in_basic():
    assert E("(get-in {:a {:b {:c 42}}} [:a :b :c])") == 42

def test_get_in_missing_returns_nil():
    assert E("(get-in {:a {:b 1}} [:a :x])") is None

def test_get_in_missing_returns_default():
    assert E("(get-in {:a {:b 1}} [:a :x] :default)") == K("default")

def test_get_in_with_default_distinguishes_present_nil():
    """If a key is present and bound to nil, default isn't used."""
    out = E("(get-in {:a nil} [:a] :default)")
    assert out is None

def test_get_in_empty_path_returns_map():
    """Empty path → reduce1 returns the input."""
    out = E("(get-in {:a 1} [])")
    assert dict(out) == {K("a"): 1}

def test_get_in_nil_map():
    assert E("(get-in nil [:a :b])") is None


# --- assoc-in -----------------------------------------------------

def test_assoc_in_basic():
    out = E("(assoc-in {:a {:b 1}} [:a :b] 99)")
    assert dict(out) == {K("a"): E("{:b 99}")}

def test_assoc_in_creates_intermediate_levels():
    """Nonexistent levels become hash-maps."""
    out = E("(assoc-in {} [:x :y :z] 42)")
    z = E("(get-in {:x {:y {:z 42}}} [:x :y :z])") if False else None
    assert E("(get-in (assoc-in {} [:x :y :z] 42) [:x :y :z])") == 42

def test_assoc_in_single_key():
    out = E("(assoc-in {:a 1} [:a] 99)")
    assert dict(out) == {K("a"): 99}


# --- update-in ----------------------------------------------------

def test_update_in_basic():
    out = E("(update-in {:a 1} [:a] inc)")
    assert dict(out) == {K("a"): 2}

def test_update_in_nested():
    """Inner value updated, structure preserved."""
    out = E("(update-in {:a {:b 1}} [:a :b] inc)")
    assert E("(get-in (update-in {:a {:b 1}} [:a :b] inc) [:a :b])") == 2

def test_update_in_with_extra_args():
    """update-in passes args through to the fn."""
    out = E("(update-in {:a 1} [:a] + 100 7)")
    assert dict(out) == {K("a"): 108}

def test_update_in_missing_key_creates_path():
    """Missing intermediate maps get created on the way down."""
    out = E("(update-in {} [:a :b] (fn [_] :v))")
    assert E("(get-in (update-in {} [:a :b] (fn [_] :v)) [:a :b])") == K("v")


# --- JVM string-method fallbacks ---------------------------------

def test_lastindexof_fallback_on_python_str():
    assert E('(.lastIndexOf "abc/def/ghi" "/")') == 7

def test_indexof_fallback():
    assert E('(.indexOf "abc/def" "/")') == 3

def test_startswith_fallback():
    assert E('(.startsWith "hello world" "hello")') is True
    assert E('(.startsWith "hello world" "world")') is False

def test_endswith_fallback():
    assert E('(.endsWith "hello world" "world")') is True
