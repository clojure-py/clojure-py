"""Tests for core.clj batch 34 (selected from JVM 6255-6680):
update / type predicates / trampoline / intern / while / memoize /
condp / future? / fnil / zipmap / etc.

Forms (~20):
  update,
  coll?, list?, seqable?, ifn?, fn?,
  associative?, sorted?, counted?, empty?, reversible?, indexed?,
  trampoline,
  intern,
  while (macro),
  memoize,
  condp (macro),
  future?, future-done?,
  fnil,
  zipmap.

Skipped — saved for follow-up batches:
  *1 / *2 / *3 / *e / *repl*    — REPL-state vars.
  alter-meta! calls + add-doc-and-meta block (JVM 6477-6611) —
                                   pure documentation; no behavior.
  letfn                         — expands to letfn*, which is in
                                   SPECIAL_FORMS but not yet
                                   implemented in the compiler.
                                   Mutually-recursive fn bindings
                                   need cell-based slot promotion.

  sequential? / flatten redefs were already pulled forward to batch
  31; the JVM lines at 6310 and 7288 are no-ops in our port.

Backend additions:
  RT.can_seq(coll)
    Mirrors JVM RT.canSeq — checks whether seq() is supported.
    Used by seqable?.

  IFn registrations (afn.pxi):
    Python function types now register with IFn so (ifn? f) is true
    for plain Python fns, lambdas, built-in fns, bound methods, and
    Cython-compiled fns (e.g. our `+` dispatcher).

Adaptations from JVM source:
  fn? approximates JVM's clojure.lang.Fn marker via "Python function
       type or AFn-derived" — covers fn* outputs and the AFn / RestFn
       built-ins, but a class implementing IFn (like Keyword) returns
       false.
  future? uses py.concurrent.futures/Future (JVM:
       java.util.concurrent.Future).
  future-done? uses .done (Python attr; JVM .isDone).
  intern uses .alter_meta with a constant-fn since our Var doesn't
       expose .setMeta directly.
"""

import concurrent.futures as _cf

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Var, Symbol, Keyword,
    PersistentArrayMap,
    PersistentVector,
    Namespace,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


# --- update -------------------------------------------------------

def test_update_basic():
    out = E("(update {:a 1} :a inc)")
    assert dict(out) == {K("a"): 2}

def test_update_missing_key():
    """(update m k f) passes nil for missing key."""
    out = E("(update {} :missing (fn [v] (or v :default)))")
    assert dict(out) == {K("missing"): K("default")}

def test_update_with_extra_args():
    out = E("(update {:a 1} :a + 100 7)")
    assert dict(out) == {K("a"): 108}


# --- type predicates ---------------------------------------------

def test_coll_pred():
    assert E("(coll? [])") is True
    assert E("(coll? {})") is True
    assert E("(coll? #{})") is True
    assert E("(coll? (list))") is True
    assert E('(coll? "abc")') is False
    assert E("(coll? 42)") is False
    assert E("(coll? nil)") is False

def test_list_pred():
    assert E("(list? '(1 2 3))") is True
    assert E("(list? [1 2 3])") is False
    assert E("(list? nil)") is False

def test_seqable_pred():
    assert E("(seqable? [])") is True
    assert E("(seqable? {})") is True
    assert E('(seqable? "abc")') is True
    assert E("(seqable? nil)") is True
    assert E("(seqable? (range 5))") is True
    assert E("(seqable? 42)") is False

def test_ifn_pred():
    assert E("(ifn? +)") is True             # Cython-compiled built-in
    assert E("(ifn? (fn* [x] x))") is True   # plain Python fn
    assert E("(ifn? :keyword)") is True      # keywords are IFn
    assert E("(ifn? 42)") is False
    assert E('(ifn? "string")') is False

def test_fn_pred():
    assert E("(fn? (fn* [x] x))") is True    # user-defined fn
    assert E("(fn? +)") is True              # built-in (Cython fn)
    assert E("(fn? :a)") is False            # keyword — not Fn
    assert E("(fn? 42)") is False

def test_associative_pred():
    assert E("(associative? {})") is True
    assert E("(associative? [])") is True
    assert E("(associative? #{})") is False
    assert E("(associative? '(1 2 3))") is False

def test_sorted_pred():
    assert E("(sorted? (sorted-set 1 2 3))") is True
    assert E("(sorted? (sorted-map :a 1))") is True
    assert E("(sorted? #{})") is False
    assert E("(sorted? {})") is False

def test_counted_pred():
    assert E("(counted? [])") is True
    assert E("(counted? {})") is True
    assert E("(counted? #{})") is True
    # Lazy seqs: not counted (counting requires walk).
    assert E("(counted? (range))") is False

def test_empty_pred():
    assert E("(empty? [])") is True
    assert E("(empty? [1])") is False
    assert E("(empty? nil)") is True
    assert E("(empty? {})") is True
    assert E('(empty? "")') is True
    assert E('(empty? "abc")') is False

def test_reversible_pred():
    assert E("(reversible? [1 2 3])") is True
    assert E("(reversible? '(1 2 3))") is False  # lists not reversible
    assert E("(reversible? (sorted-set 1 2 3))") is True

def test_indexed_pred():
    assert E("(indexed? [1 2 3])") is True
    assert E("(indexed? '(1 2 3))") is False
    assert E("(indexed? {})") is False


# --- trampoline ---------------------------------------------------

def test_trampoline_simple():
    """Returns the value if not a fn."""
    assert E("(trampoline (fn* [] 42))") == 42

def test_trampoline_chain():
    """Each return-of-fn re-invokes."""
    out = E("""
      (let [step (fn step [n]
                   (if (zero? n) :done (fn* [] (step (dec n)))))]
        (trampoline step 5))""")
    assert out == K("done")

def test_trampoline_with_args():
    """(trampoline f & args) — initial call passes args."""
    out = E("(trampoline (fn [x] (* x 2)) 21)")
    assert out == 42


# --- intern -------------------------------------------------------

def test_intern_creates_var_with_value():
    v = E("(intern (quote user) (quote tcb34-iv) 99)")
    assert isinstance(v, Var)
    assert E("user/tcb34-iv") == 99

def test_intern_without_value():
    """Two-arg intern leaves the var unbound."""
    v = E("(intern (quote user) (quote tcb34-iv-unbound))")
    assert v.has_root() is False


# --- while --------------------------------------------------------

def test_while_iterates():
    counter = []
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb34-pop!"),
               lambda: (counter.append(1), len(counter))[1])
    out = E("""
      (let [n (atom 0)]
        (while (< @n 3)
          (swap! n inc)
          (user/tcb34-pop!)))""")
    assert out is None
    assert sum(counter) == 3

def test_while_zero_iterations():
    """When test is false from the start, body never runs."""
    out = E("(while false :never)")
    assert out is None


# --- memoize ------------------------------------------------------

def test_memoize_caches_results():
    counter = [0]
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb34-mem!"),
               lambda x: (counter.append(1), x * x)[1])
    E("(def tcb34-memoized (memoize user/tcb34-mem!))")
    a = E("(tcb34-memoized 5)")
    b = E("(tcb34-memoized 5)")
    c = E("(tcb34-memoized 6)")
    assert a == b == 25
    assert c == 36
    # 5 was computed once, 6 once → 2 calls
    assert sum(counter) == 2

def test_memoize_separate_keys_per_arity():
    """Args tuple is the cache key; (f 1) and (f 1 2) are distinct."""
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb34-multi"),
               lambda *args: sum(args))
    E("(def tcb34-mm (memoize user/tcb34-multi))")
    assert E("(tcb34-mm 1)") == 1
    assert E("(tcb34-mm 1 2)") == 3


# --- condp --------------------------------------------------------

def test_condp_first_match_wins():
    out = E("""
      (condp = 2
        1 :one
        2 :two
        3 :three)""")
    assert out == K("two")

def test_condp_default_when_no_match():
    out = E("""
      (condp = 99
        1 :one
        2 :two
        :default)""")
    assert out == K("default")

def test_condp_throws_when_no_match_no_default():
    with pytest.raises(Exception, match="No matching clause"):
        E("(condp = 99 1 :one 2 :two)")

def test_condp_with_arrow_clause():
    """Ternary :>> form: (pred test expr) result is passed to result-fn."""
    out = E("""
      (condp some [1 2 3 4]
        #{0 6 7} :>> inc
        #{4 5 9} :>> dec
        :default)""")
    # 4 is in the second set; the predicate returns truthy (the elem 4),
    # which is passed to dec → 3.
    assert out == 3


# --- future? / future-done? --------------------------------------

def test_future_pred_true_for_future():
    fut = _cf.Future()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb34-fut1"), fut)
    assert E("(future? user/tcb34-fut1)") is True

def test_future_pred_false_for_other():
    assert E("(future? 42)") is False
    assert E("(future? nil)") is False

def test_future_done_lifecycle():
    fut = _cf.Future()
    Var.intern(Compiler.current_ns(), Symbol.intern("tcb34-fut2"), fut)
    assert E("(future-done? user/tcb34-fut2)") is False
    fut.set_result(99)
    assert E("(future-done? user/tcb34-fut2)") is True


# --- fnil ---------------------------------------------------------

def test_fnil_one_x_replaces_first_arg_when_nil():
    inc0 = E("(fnil inc 0)")
    assert inc0(None) == 1
    assert inc0(5) == 6

def test_fnil_two_args_replace_first_two_when_nil():
    f = E("(fnil + 10 100)")
    assert f(None, None) == 110
    assert f(1, 2) == 3
    assert f(None, 5) == 15

def test_fnil_three_args_replace_first_three_when_nil():
    f = E("(fnil + 1 10 100)")
    assert f(None, None, None) == 111
    assert f(2, None, None) == 112


# --- zipmap -------------------------------------------------------

def test_zipmap_basic():
    out = E("(zipmap [:a :b :c] [1 2 3])")
    assert dict(out) == {K("a"): 1, K("b"): 2, K("c"): 3}

def test_zipmap_uneven_truncates():
    """When seqs differ in length, stops at the shorter."""
    out = E("(zipmap [:a :b :c :d] [1 2])")
    assert dict(out) == {K("a"): 1, K("b"): 2}

def test_zipmap_empty_keys_or_vals():
    assert dict(E("(zipmap [] [1 2 3])")) == {}
    assert dict(E("(zipmap [:a :b] [])")) == {}
