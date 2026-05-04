"""Tests for core.clj batch 22 (lines 4208-4407): namespace ops, transducers,
and var/let helpers.

Forms (16):
  ns-unmap, ns-publics, ns-imports, ns-interns,
  refer (function), ns-refers,
  alias, ns-aliases, ns-unalias,
  take-nth, interleave,
  var-get, var-set, with-local-vars (macro),
  ns-resolve, resolve.

Compiler bug fix worth calling out (also from this batch):
  `..` (the threading macro) was being misinterpreted as `.method`
  interop sugar by macroexpand-1 and the special-form dispatcher.
  Now both check that the leading `.` is followed by a non-`.` char
  before treating the symbol as interop sugar — `..` falls through
  to normal macro lookup.

Bootstrap fix:
  user namespace now refers all of clojure.core's public Vars after
  bootstrap, matching JVM Clojure's default for namespaces created
  without an explicit (ns ...) form. Required for bare names like
  `let`, `defn`, `..` etc. to resolve from the REPL or fresh user
  code.

Adaptations from JVM source:
  ns-publics / ns-interns / ns-refers use (.- ns v) field access
    where JVM uses (.ns v).
  alias / ns-aliases / ns-unalias use snake_case
    .add_alias / .get_aliases / .remove_alias.
  with-local-vars uses snake_case set_dynamic / push_thread_bindings
    / pop_thread_bindings.
  ns-resolve calls Compiler/maybe_resolve_in (snake_case).
  refer's IllegalAccessError is aliased to RuntimeError in core.clj.

Skipped (deferred):
  - The `;(defn export ...)` commented-out form in JVM source is
    preserved verbatim, no behavior to test.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Namespace, Keyword,
    PersistentArrayMap,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- ns-unmap -----------------------------------------------------

def test_ns_unmap_removes_mapping():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb22-tmp"), 99)
    assert E("user/tcb22-tmp") == 99
    E("(clojure.core/ns-unmap 'user 'tcb22-tmp)")
    with pytest.raises(NameError):
        E("user/tcb22-tmp")

def test_ns_unmap_missing_no_op():
    """Unmapping a name that isn't there should be a no-op, not throw."""
    E("(clojure.core/ns-unmap 'user 'tcb22-not-there)")


# --- ns-publics ---------------------------------------------------

def test_ns_publics_includes_public_vars():
    out = E("(clojure.core/ns-publics 'clojure.core)")
    keys_str = {str(e.key()) for e in out}
    assert "+" in keys_str
    assert "inc" in keys_str

def test_ns_publics_excludes_private_vars():
    """filter-key, def-aset, etc. are private — not in ns-publics."""
    out = E("(clojure.core/ns-publics 'clojure.core)")
    keys_str = {str(e.key()) for e in out}
    assert "filter-key" not in keys_str
    assert "def-aset" not in keys_str

def test_ns_publics_excludes_referred():
    """Vars from another namespace shouldn't appear in this ns's publics."""
    user = E("(clojure.core/ns-publics 'user)")
    # user has clojure.core's vars referred but they aren't user's publics.
    keys_str = {str(e.key()) for e in user}
    # `+` is referred from clojure.core, not interned in user.
    assert "+" not in keys_str


# --- ns-imports ---------------------------------------------------

def test_ns_imports_returns_class_mappings_only():
    """ns-imports filters on (instance? Class v). With our type aliases
    being Vars (not class mappings), this is mostly empty in clojure.core."""
    out = E("(clojure.core/ns-imports 'clojure.core)")
    # Type aliases are Vars now, not class mappings — output is empty.
    # Just verify it doesn't crash and returns a map.
    assert hasattr(out, "count")


# --- ns-interns ---------------------------------------------------

def test_ns_interns_includes_private_vars():
    """ns-interns is all-Vars-this-ns-owns; includes both public and private."""
    out = E("(clojure.core/ns-interns 'clojure.core)")
    keys_str = {str(e.key()) for e in out}
    assert "+" in keys_str
    assert "filter-key" in keys_str  # private but interned

def test_ns_interns_excludes_referred():
    out = E("(clojure.core/ns-interns 'user)")
    keys_str = {str(e.key()) for e in out}
    assert "+" not in keys_str  # `+` lives in clojure.core


# --- refer (function) ---------------------------------------------

def test_refer_brings_publics_into_current_ns():
    """Set up a source namespace with a public Var, refer it elsewhere."""
    src_ns = Namespace.find_or_create(Symbol.intern("tcb22.refer-src"))
    Var.intern(src_ns, Symbol.intern("answer"), 42)
    target_ns = Namespace.find_or_create(Symbol.intern("tcb22.refer-target"))

    # Switch *ns* to target via thread binding, refer src.
    from clojure.lang import RT
    Var.push_thread_bindings(
        PersistentArrayMap.create(RT.CURRENT_NS, target_ns))
    try:
        E("(clojure.core/refer 'tcb22.refer-src)")
    finally:
        Var.pop_thread_bindings()

    # target_ns should now have `answer` referred.
    answer = target_ns.get_mapping(Symbol.intern("answer"))
    assert answer is not None
    assert answer.deref() == 42

def test_refer_throws_on_missing_ns():
    with pytest.raises(Exception, match="No namespace"):
        E("(clojure.core/refer 'tcb22.does-not-exist)")


# --- ns-refers ----------------------------------------------------

def test_ns_refers_includes_clojure_core_vars():
    """user has clojure.core referred — those show up in ns-refers."""
    out = E("(clojure.core/ns-refers 'user)")
    keys_str = {str(e.key()) for e in out}
    assert "+" in keys_str
    assert "let" in keys_str

def test_ns_refers_excludes_local_vars():
    """user's own interned vars don't appear in its refers."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb22-local"), 1)
    out = E("(clojure.core/ns-refers 'user)")
    keys_str = {str(e.key()) for e in out}
    assert "tcb22-local" not in keys_str


# --- alias / ns-aliases / ns-unalias ------------------------------

def test_alias_and_ns_aliases():
    E("(clojure.core/create-ns 'tcb22.alias-target)")
    E("(clojure.core/alias 'tgt 'tcb22.alias-target)")
    aliases = E("(clojure.core/ns-aliases 'user)")
    keys_str = {str(e.key()) for e in aliases}
    assert "tgt" in keys_str

def test_ns_unalias_removes():
    E("(clojure.core/create-ns 'tcb22.alias-rm)")
    E("(clojure.core/alias 'rm 'tcb22.alias-rm)")
    E("(clojure.core/ns-unalias 'user 'rm)")
    aliases = E("(clojure.core/ns-aliases 'user)")
    keys_str = {str(e.key()) for e in aliases}
    assert "rm" not in keys_str


# --- take-nth ------------------------------------------------------

def test_take_nth_basic():
    assert list(E("(clojure.core/take-nth 2 [1 2 3 4 5 6])")) == [1, 3, 5]

def test_take_nth_one_returns_all():
    assert list(E("(clojure.core/take-nth 1 [1 2 3])")) == [1, 2, 3]

def test_take_nth_empty():
    assert list(E("(clojure.core/take-nth 2 [])")) == []

def test_take_nth_transducer():
    out = list(E("(clojure.core/sequence (clojure.core/take-nth 3) [10 20 30 40 50 60 70])"))
    assert out == [10, 40, 70]

def test_take_nth_lazy():
    """Doesn't realize past what's needed."""
    out = list(E("(clojure.core/take 3 (clojure.core/take-nth 2 (clojure.core/range 100)))"))
    assert out == [0, 2, 4]


# --- interleave ----------------------------------------------------

def test_interleave_zero_args():
    assert list(E("(clojure.core/interleave)")) == []

def test_interleave_one_arg():
    assert list(E("(clojure.core/interleave [1 2 3])")) == [1, 2, 3]

def test_interleave_two_args():
    out = list(E("(clojure.core/interleave [1 2 3] [:a :b :c])"))
    assert out == [1, K("a"), 2, K("b"), 3, K("c")]

def test_interleave_three_args():
    out = list(E('(clojure.core/interleave [1 2] [:a :b] ["x" "y"])'))
    assert out == [1, K("a"), "x", 2, K("b"), "y"]

def test_interleave_uneven_truncates_to_shortest():
    out = list(E("(clojure.core/interleave [1 2 3 4] [:a :b])"))
    assert out == [1, K("a"), 2, K("b")]


# --- var-get / var-set --------------------------------------------

def test_var_get_returns_var_value():
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb22-vg"), 42)
    assert E("(clojure.core/var-get (clojure.core/var user/tcb22-vg))") == 42

def test_var_set_requires_thread_binding():
    """var-set on a non-thread-bound dynamic var should throw."""
    v = Var.intern(Compiler.current_ns(), Symbol.intern("tcb22-vs"), 0)
    v.set_dynamic()
    with pytest.raises((RuntimeError, Exception)):
        E("(clojure.core/var-set (clojure.core/var user/tcb22-vs) 99)")


# --- with-local-vars ----------------------------------------------

def test_with_local_vars_basic():
    out = E("(clojure.core/with-local-vars [x 1 y 2] "
            "  (clojure.core/+ (clojure.core/var-get x) (clojure.core/var-get y)))")
    assert out == 3

def test_with_local_vars_var_set():
    out = E("(clojure.core/with-local-vars [x 1] "
            "  (clojure.core/var-set x 99) "
            "  (clojure.core/var-get x))")
    assert out == 99

def test_with_local_vars_assert_args_non_vector():
    with pytest.raises(Exception, match="vector for its binding"):
        E("(clojure.core/with-local-vars (x 1) x)")

def test_with_local_vars_assert_args_odd_bindings():
    with pytest.raises(Exception, match="even number"):
        E("(clojure.core/with-local-vars [x 1 y] x)")


# --- ns-resolve / resolve -----------------------------------------

def test_ns_resolve_existing_var():
    out = E("(clojure.core/ns-resolve 'clojure.core '+)")
    assert isinstance(out, Var)
    assert str(out) == "#'clojure.core/+"

def test_ns_resolve_missing_returns_nil():
    assert E("(clojure.core/ns-resolve 'clojure.core 'tcb22-nonexistent)") is None

def test_ns_resolve_with_env_filters_locals():
    """If the symbol is in env, ns-resolve returns nil — env shadows ns."""
    out = E("(clojure.core/ns-resolve 'clojure.core {'+ 'local} '+)")
    # `+` is in env, so it shouldn't resolve via ns
    assert out is None

def test_resolve_in_current_ns():
    out = E("(clojure.core/resolve '+)")
    assert isinstance(out, Var)
    assert str(out) == "#'clojure.core/+"

def test_resolve_qualified():
    out = E("(clojure.core/resolve 'clojure.core/+)")
    assert isinstance(out, Var)


# --- regression: `..` macro now expands properly ------------------

def test_dot_dot_expands_as_macro():
    """Was previously caught by the `.method` interop-sugar check."""
    out = E("(clojure.core/.. clojure.lang.Var create set_dynamic)")
    assert isinstance(out, Var)
    assert out.dynamic is True


# --- regression: user ns refers clojure.core ---------------------

def test_user_ns_refers_clojure_core_publics():
    """Bare `let`, `defn`, `..`, etc. resolve from user ns."""
    assert E("(let [x 5] (+ x 10))") == 15

def test_user_ns_can_call_macros_unqualified():
    """when, if-let, doseq etc. all available bare in user."""
    out = E("(when true (+ 1 2))")
    assert out == 3
