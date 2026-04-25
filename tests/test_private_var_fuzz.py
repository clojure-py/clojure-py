"""Property-based fuzzing of private-var cross-ns visibility."""

from hypothesis import given, strategies as st
import pytest

from clojure._core import eval_string as e, EvalError


# Lowercase ASCII identifier names.
name_chars = st.text(
    alphabet="abcdefghijklmnopqrstuvwxyz", min_size=1, max_size=8
)


@pytest.fixture(autouse=True)
def _restore_ns():
    """Each test in this file installs throwaway namespaces via `(in-ns ...)`.
    Restore the previous current-ns on teardown so later tests see a clean
    starting state.
    """
    saved = e("(ns-name *ns*)")
    yield
    e(f"(in-ns '{saved})")


@given(varname=name_chars, private=st.booleans())
def test_visibility_rule(varname, private):
    """Cross-ns access succeeds iff the var is not private."""
    decl = "^:private" if private else ""
    e("(in-ns 'fuzz-src)")
    e(f"(def {decl} ZZ_{varname} :sentinel)")
    e("(in-ns 'fuzz-cli)")
    e("(clojure.core/refer 'clojure.core)")
    if private:
        with pytest.raises(EvalError) as ei:
            e(f"fuzz-src/ZZ_{varname}")
        assert "is not public" in str(ei.value)
    else:
        assert e(f"fuzz-src/ZZ_{varname}") == e(":sentinel")
