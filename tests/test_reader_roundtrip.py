"""Reader R6 — property-based round-trip: read_string(pr_str(x)) == x."""

import pytest
from hypothesis import given, settings, strategies as st, assume
from clojure._core import (
    read_string, pr_str,
    keyword, symbol,
    vector, hash_map, hash_set, list_,
)


# Strategies for generating Clojure values.

simple_atoms = st.one_of(
    st.none(),
    st.booleans(),
    st.integers(min_value=-10**9, max_value=10**9),
    # Use a constrained float range — NaN/inf don't round-trip through our simple printer.
    st.floats(min_value=-1e9, max_value=1e9, allow_nan=False, allow_infinity=False),
    # Plain ASCII strings to keep the printer's escape logic simple; no control chars.
    st.text(alphabet=st.characters(
        whitelist_categories=("Lu", "Ll", "Nd"),
        min_codepoint=32, max_codepoint=126,
    ), min_size=0, max_size=10),
)


def keyword_strategy():
    """Keyword with ASCII-ish name."""
    name = st.text(
        alphabet=st.characters(whitelist_categories=("Lu", "Ll")),
        min_size=1, max_size=6,
    )
    return st.builds(lambda n: keyword(n), name)


def symbol_strategy():
    """Symbol with ASCII-ish name, excluding reserved reader words."""
    name = st.text(
        alphabet=st.characters(whitelist_categories=("Lu", "Ll")),
        min_size=1, max_size=6,
    ).filter(lambda n: n not in ("true", "false", "nil"))
    return st.builds(lambda n: symbol(n), name)


# Compound-value strategies built recursively.

def clojure_value(max_depth=3):
    """Recursive strategy for Clojure values."""
    if max_depth <= 0:
        return st.one_of(simple_atoms, keyword_strategy(), symbol_strategy())
    child = clojure_value(max_depth - 1)
    return st.one_of(
        simple_atoms,
        keyword_strategy(),
        symbol_strategy(),
        st.lists(child, max_size=5).map(lambda xs: vector(*xs)),
        st.lists(child, max_size=5).map(lambda xs: list_(*xs)),
        # Map: keys must be hashable — use atoms.
        st.dictionaries(
            st.one_of(simple_atoms, keyword_strategy()),
            child,
            max_size=5,
        ).map(lambda d: _build_map(d)),
        st.sets(st.one_of(
            st.integers(min_value=-100, max_value=100),
            keyword_strategy(),
        ), max_size=5).map(lambda s: hash_set(*s)),
    )


def _build_map(d):
    m = hash_map()
    for k, v in d.items():
        m = m.assoc(k, v)
    return m


@given(clojure_value())
@settings(max_examples=200, deadline=None)
def test_roundtrip(x):
    """For any generated Clojure value x, read_string(pr_str(x)) == x."""
    s = pr_str(x)
    y = read_string(s)
    # Equal via our IEquiv-routed __eq__.
    assert x == y, f"round-trip mismatch: {s!r} → {pr_str(y)!r}"


# Specific scenario tests (not hypothesis, just examples).

@pytest.mark.parametrize("src", [
    "nil",
    "true",
    "false",
    "42",
    "-17",
    "3.14",
    '"hello"',
    ":foo",
    ":ns/foo",
    "foo",
    "my.ns/foo",
    "()",
    "(1 2 3)",
    "[]",
    "[1 2 3]",
    "[1 [2 3] 4]",
    "{}",
    "{:a 1 :b 2}",
    "{:nested {:deep 42}}",
    "#{}",
    "#{1 2 3}",
    "[:a [1 2] {:k :v} #{:c}]",
])
def test_read_then_print_idempotent(src):
    v = read_string(src)
    printed = pr_str(v)
    v2 = read_string(printed)
    printed2 = pr_str(v2)
    assert printed == printed2, f"double-print differs: {printed!r} vs {printed2!r}"
