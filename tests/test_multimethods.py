"""Multimethod tests — defmulti, defmethod, dispatch algorithm, prefer-method,
cache invalidation, introspection."""

import pytest
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


@pytest.fixture(autouse=True)
def _reset_hierarchy():
    _ev("(alter-var-root #'clojure.core/global-hierarchy (constantly (make-hierarchy)))")
    yield


# --- Basic defmulti/defmethod ---

def test_value_dispatch():
    _ev("(defmulti mm-area :shape)")
    _ev("(defmethod mm-area :circle [s] (:r s))")
    _ev("(defmethod mm-area :square [s] (:side s))")
    assert _ev("(mm-area {:shape :circle :r 5})") == 5
    assert _ev("(mm-area {:shape :square :side 10})") == 10


def test_default_method():
    _ev("(defmulti mm-k identity)")
    _ev("(defmethod mm-k :default [x] :fallback)")
    assert _ev("(mm-k :anything)") == keyword("fallback")


def test_no_method_error():
    from clojure._core import IllegalArgumentException
    _ev("(defmulti mm-none identity)")
    with pytest.raises(IllegalArgumentException):
        _ev("(mm-none :nothing-registered)")


def test_methods_introspection():
    _ev("(defmulti mm-i identity)")
    _ev("(defmethod mm-i :a [_] 1)")
    _ev("(defmethod mm-i :b [_] 2)")
    assert _ev("(count (methods mm-i))") == 2


def test_get_method_returns_callable():
    _ev("(defmulti mm-g identity)")
    _ev("(defmethod mm-g :a [_] 1)")
    r = _ev("(get-method mm-g :a)")
    assert callable(r)


def test_get_method_none_when_no_match():
    _ev("(defmulti mm-h identity)")
    assert _ev("(get-method mm-h :missing)") is None


# --- Hierarchy-based dispatch ---

def test_hierarchy_dispatch_one_level():
    _ev("(derive :dog :mammal)")
    _ev("(defmulti mm-talk identity)")
    _ev("(defmethod mm-talk :mammal [_] :sound)")
    assert _ev("(mm-talk :dog)") == keyword("sound")


def test_hierarchy_dispatch_transitive():
    _ev("(derive :poodle :dog)")
    _ev("(derive :dog :mammal)")
    _ev("(derive :mammal :animal)")
    _ev("(defmulti mm-type identity)")
    _ev("(defmethod mm-type :animal [_] :animal-kind)")
    assert _ev("(mm-type :poodle)") == keyword("animal-kind")


def test_hierarchy_exact_beats_ancestor():
    _ev("(derive :dog :mammal)")
    _ev("(defmulti mm-beat identity)")
    _ev("(defmethod mm-beat :mammal [_] :mammal)")
    _ev("(defmethod mm-beat :dog [_] :dog)")
    assert _ev("(mm-beat :dog)") == keyword("dog")


# --- Class-based dispatch (Python issubclass) ---

def test_class_dispatch_int():
    _ev("(defmulti mm-c clojure.core/class)")
    _ev("(defmethod mm-c (clojure.core/class 1) [_] :int-type)")
    assert _ev("(mm-c 42)") == keyword("int-type")


def test_class_dispatch_string():
    _ev("(defmulti mm-cs clojure.core/class)")
    _ev("(defmethod mm-cs (clojure.core/class \"x\") [_] :str-type)")
    assert _ev('(mm-cs "hello")') == keyword("str-type")


# --- Prefer-method ---

def test_prefer_method_resolves_ambiguity():
    _ev("(derive :X :Both)")
    _ev("(derive :Y :Both)")
    _ev("(derive :XY :X)")
    _ev("(derive :XY :Y)")
    _ev("(defmulti mm-p identity)")
    _ev("(defmethod mm-p :X [_] :x-method)")
    _ev("(defmethod mm-p :Y [_] :y-method)")
    _ev("(prefer-method mm-p :X :Y)")
    # :XY isa? both :X and :Y — prefer-method says :X wins.
    assert _ev("(mm-p :XY)") == keyword("x-method")


def test_ambiguity_without_prefer_raises():
    from clojure._core import IllegalArgumentException
    _ev("(derive :XY :X)")
    _ev("(derive :XY :Y)")
    _ev("(defmulti mm-a identity)")
    _ev("(defmethod mm-a :X [_] :x)")
    _ev("(defmethod mm-a :Y [_] :y)")
    with pytest.raises(IllegalArgumentException):
        _ev("(mm-a :XY)")


# --- Cache invalidation ---

def test_cache_invalidated_by_new_defmethod():
    _ev("(defmulti mm-cv identity)")
    _ev("(defmethod mm-cv :default [_] :old)")
    assert _ev("(mm-cv :x)") == keyword("old")
    # Add a new, specific method — the cached default should be invalidated.
    _ev("(defmethod mm-cv :x [_] :specific)")
    assert _ev("(mm-cv :x)") == keyword("specific")


def test_cache_invalidated_by_derive():
    _ev("(defmulti mm-ci identity)")
    _ev("(defmethod mm-ci :mammal [_] :is-mammal)")
    _ev("(defmethod mm-ci :default [_] :other)")
    # :cat isn't derived yet — should hit default.
    assert _ev("(mm-ci :cat)") == keyword("other")
    # Now derive — the cached :other should be invalidated by hierarchy change.
    _ev("(derive :cat :mammal)")
    assert _ev("(mm-ci :cat)") == keyword("is-mammal")


def test_remove_method_removes_entry():
    _ev("(defmulti mm-rm identity)")
    _ev("(defmethod mm-rm :a [_] 1)")
    _ev("(defmethod mm-rm :b [_] 2)")
    _ev("(remove-method mm-rm :a)")
    assert _ev("(count (methods mm-rm))") == 1


def test_remove_all_methods_clears():
    _ev("(defmulti mm-ra identity)")
    _ev("(defmethod mm-ra :a [_] 1)")
    _ev("(defmethod mm-ra :b [_] 2)")
    _ev("(.removeAllMethods mm-ra)")
    assert _ev("(count (methods mm-ra))") == 0


# --- Multi-arity + dispatch-fn-closures ---

def test_multi_arg_dispatch():
    _ev("(defmulti mm-op (fn [op _a _b] op))")
    _ev("(defmethod mm-op :add [_ a b] (+ a b))")
    _ev("(defmethod mm-op :sub [_ a b] (- a b))")
    assert _ev("(mm-op :add 5 3)") == 8
    assert _ev("(mm-op :sub 5 3)") == 2


def test_nil_dispatch_value():
    _ev("(defmulti mm-nil identity)")
    _ev("(defmethod mm-nil nil [_] :got-nil)")
    assert _ev("(mm-nil nil)") == keyword("got-nil")
