"""Hierarchy tests — make-hierarchy, derive, underive, parents,
ancestors, descendants, isa?, and class-based isa? semantics."""

import pytest
from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


@pytest.fixture(autouse=True)
def _reset_hierarchy():
    """Reset the global hierarchy between tests to avoid cross-test pollution."""
    _ev("(alter-var-root #'clojure.core/global-hierarchy (constantly (make-hierarchy)))")
    yield


# --- make-hierarchy ---

def test_make_hierarchy_has_three_maps():
    h = _ev("(make-hierarchy)")
    # Result is a Clojure map with :parents, :ancestors, :descendants.
    for k in ("parents", "ancestors", "descendants"):
        assert _ev(f"(get (make-hierarchy) :{k})") is not None


# --- derive + isa? ---

def test_isa_equality():
    assert _ev("(isa? 1 1)") is True
    assert _ev("(isa? :k :k)") is True
    assert _ev("(isa? 1 2)") is False


def test_derive_direct_parent():
    _ev("(derive :child :parent)")
    assert _ev("(isa? :child :parent)") is True


def test_derive_transitive():
    _ev("(derive :a :b)")
    _ev("(derive :b :c)")
    assert _ev("(isa? :a :c)") is True


def test_derive_self_errors():
    # Vanilla `derive` uses `(assert (not= tag parent))` — AssertionError.
    with pytest.raises(AssertionError):
        _ev("(derive :x :x)")


def test_derive_cycle_errors():
    from clojure._core import IllegalArgumentException
    _ev("(derive :a :b)")
    with pytest.raises(IllegalArgumentException):
        _ev("(derive :b :a)")


def test_derive_is_idempotent():
    _ev("(derive :a :b)")
    _ev("(derive :a :b)")
    # Still just one parent.
    ps = _ev("(parents :a)")
    assert ps is not None


# --- parents / ancestors / descendants ---

def test_parents_single():
    _ev("(derive :dog :mammal)")
    assert _ev("(contains? (parents :dog) :mammal)") is True


def test_parents_multiple():
    _ev("(derive :a :b)")
    _ev("(derive :a :c)")
    assert _ev("(count (parents :a))") == 2


def test_ancestors_includes_transitive():
    _ev("(derive :a :b)")
    _ev("(derive :b :c)")
    assert _ev("(contains? (ancestors :a) :c)") is True


def test_descendants_of_root():
    _ev("(derive :dog :mammal)")
    _ev("(derive :cat :mammal)")
    assert _ev("(contains? (descendants :mammal) :dog)") is True
    assert _ev("(contains? (descendants :mammal) :cat)") is True


# --- underive ---

def test_underive_removes_relationship():
    _ev("(derive :a :b)")
    _ev("(underive :a :b)")
    assert _ev("(isa? :a :b)") is False


def test_underive_preserves_other_parents():
    _ev("(derive :a :b)")
    _ev("(derive :a :c)")
    _ev("(underive :a :b)")
    assert _ev("(isa? :a :b)") is False
    assert _ev("(isa? :a :c)") is True


def test_underive_preserves_unrelated_relationships():
    _ev("(derive :a :b)")
    _ev("(derive :x :y)")
    _ev("(underive :a :b)")
    assert _ev("(isa? :x :y)") is True


# --- class-based isa? (Python issubclass) ---

def test_isa_class_subclass():
    # bool is a Python subclass of int.
    assert _ev("(isa? (clojure.core/class true) (clojure.core/class 1))") is True


def test_isa_class_self():
    assert _ev("(isa? (clojure.core/class 1) (clojure.core/class 1))") is True


def test_parents_of_python_class():
    ps = _ev("(parents (clojure.core/class 1))")
    # int's parent is object.
    assert ps is not None


# --- vector elementwise ---

def test_isa_vector_elementwise():
    _ev("(derive :dog :mammal)")
    assert _ev("(isa? [:dog :dog] [:mammal :mammal])") is True


def test_isa_vector_mismatch_length():
    assert _ev("(isa? [:a :b] [:a])") is False


def test_isa_vector_one_mismatch():
    _ev("(derive :dog :mammal)")
    # :cat isn't a mammal — fail elementwise.
    assert _ev("(isa? [:dog :cat] [:mammal :mammal])") is False
