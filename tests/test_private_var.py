"""Cross-namespace privacy enforcement for `^:private` vars."""

import pytest
from clojure._core import eval_string as e, EvalError


@pytest.fixture(autouse=True)
def _restore_ns():
    """Each test in this file installs throwaway namespaces via `(in-ns ...)`.
    Restore the previous current-ns on teardown so later tests see a clean
    starting state.
    """
    saved = e("(ns-name *ns*)")
    yield
    e(f"(in-ns '{saved})")


def test_cross_ns_private_var_rejected():
    """Vanilla: cross-ns access to a ^:private var raises EvalError."""
    e("(in-ns 'priv-test)")
    e("(clojure.core/refer 'clojure.core)")
    e("(def ^:private secret 42)")
    e("(in-ns 'pub-test)")
    e("(clojure.core/refer 'clojure.core)")
    with pytest.raises(EvalError) as ei:
        e("priv-test/secret")
    assert "is not public" in str(ei.value)


def test_same_ns_private_var_allowed():
    """Within its own namespace, a private var resolves normally."""
    e("(in-ns 'priv-test2)")
    e("(clojure.core/refer 'clojure.core)")
    e("(def ^:private secret 99)")
    assert e("priv-test2/secret") == 99


def test_cross_ns_public_var_allowed():
    """Public var across namespaces — baseline, unchanged behavior."""
    e("(in-ns 'pub-src)")
    e("(clojure.core/refer 'clojure.core)")
    e("(def public-thing 7)")
    e("(in-ns 'pub-cli)")
    e("(clojure.core/refer 'clojure.core)")
    assert e("pub-src/public-thing") == 7


def test_defn_dash_creates_private():
    """`defn-` should set :private and trigger cross-ns rejection."""
    e("(in-ns 'priv-test3)")
    e("(clojure.core/refer 'clojure.core)")
    e("(defn- private-fn [] :hidden)")
    e("(in-ns 'pub-test3)")
    e("(clojure.core/refer 'clojure.core)")
    with pytest.raises(EvalError) as ei:
        e("(priv-test3/private-fn)")
    assert "is not public" in str(ei.value)


def test_clojure_core_private_unqualified_rejected():
    """Bare-name access to a clojure.core private from a user ns is rejected."""
    e("(in-ns 'user-ns-x)")
    e("(clojure.core/refer 'clojure.core)")
    with pytest.raises(EvalError) as ei:
        e("(spread '(1 2 3))")
    assert "is not public" in str(ei.value)


def test_clojure_core_private_qualified_rejected():
    """Same as above but qualified — vanilla also rejects."""
    e("(in-ns 'user-ns-y)")
    e("(clojure.core/refer 'clojure.core)")
    with pytest.raises(EvalError) as ei:
        e("clojure.core/spread")
    assert "is not public" in str(ei.value)
