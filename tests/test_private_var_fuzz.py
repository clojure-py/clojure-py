"""Property-based fuzzing of private-var cross-ns visibility."""

from hypothesis import given, strategies as st
import pytest

from clojure._core import eval_string as e, EvalError


# Lowercase ASCII identifier names.
name_chars = st.text(
    alphabet="abcdefghijklmnopqrstuvwxyz", min_size=1, max_size=8
)


# Initialize stable namespaces once.
e("(in-ns 'fuzz-src)")
e("(clojure.core/refer 'clojure.core)")


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
