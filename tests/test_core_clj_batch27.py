"""Tests for core.clj batch 27 (JVM lines 4896-4966): the regex family.

Forms (6 + 2 aliases):
  re-pattern, re-matcher, re-groups, re-seq, re-matches, re-find.
  Pattern alias → py.re/Pattern.
  Matcher alias → clojure.lang.JavaMatcher.

Backend additions:
  clojure.lang.JavaMatcher
    Stateful wrapper around (re.Pattern, string, position).
    Exposes the JVM Matcher API surface that core.clj reaches for:
    .find / .matches / .group / .groupCount.
    Python's re.Match is stateless; this shim bridges to JVM's
    stateful Matcher contract — find advances past the previous
    match (with zero-length-match guard), group reads from the
    last successful match.

  Compiler self-eval check now recognizes re.Pattern objects.
    Regex literals like #"..." land in code as compiled
    re.Pattern instances (the reader produces them); they're
    treated as constants now, not invalid forms.

Adaptations from JVM source:
  re-pattern uses py.re/compile and (instance? Pattern s) where
    JVM uses (. java.util.regex.Pattern (compile s)) and
    (instance? java.util.regex.Pattern s).
  re-matcher constructs JavaMatcher directly, since Python's
    re.Pattern has no .matcher method (re.Match is stateless).
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    JavaMatcher,
)


def E(src):
    return Compiler.eval(read_string(src))


# --- JavaMatcher class --------------------------------------------

def test_java_matcher_basic_find():
    import re
    m = JavaMatcher(re.compile(r"\d+"), "a 1 b 22 c 333")
    assert m.find() is True
    assert m.group() == "1"
    assert m.find() is True
    assert m.group() == "22"
    assert m.find() is True
    assert m.group() == "333"
    assert m.find() is False

def test_java_matcher_matches_full_string():
    import re
    m = JavaMatcher(re.compile(r"\d+"), "12345")
    assert m.matches() is True
    assert m.group() == "12345"

def test_java_matcher_matches_returns_false_when_partial():
    import re
    m = JavaMatcher(re.compile(r"\d+"), "abc 123")
    assert m.matches() is False

def test_java_matcher_groupcount():
    import re
    m = JavaMatcher(re.compile(r"(\w+)=(\w+)"), "a=1")
    assert m.groupCount() == 2
    m.find()
    assert m.group(0) == "a=1"
    assert m.group(1) == "a"
    assert m.group(2) == "1"

def test_java_matcher_zero_length_match_advance():
    """Zero-length matches should still advance position so .find()
    doesn't loop forever."""
    import re
    m = JavaMatcher(re.compile(r"\b"), "abc def")
    found = []
    while m.find() and len(found) < 10:  # safety cap
        found.append(m.group())
    # Should terminate; multiple word boundaries.
    assert len(found) < 10

def test_java_matcher_group_before_match_raises():
    import re
    m = JavaMatcher(re.compile(r"\d+"), "abc")
    with pytest.raises(Exception):
        m.group()


# --- Pattern / Matcher aliases ------------------------------------

def test_pattern_alias_resolves_to_re_pattern():
    import re
    assert E("Pattern") is re.Pattern

def test_matcher_alias_resolves_to_javamatcher():
    assert E("Matcher") is JavaMatcher


# --- re-pattern ---------------------------------------------------

def test_re_pattern_compiles_string():
    out = E(r'(re-pattern "\\d+")')
    import re
    assert isinstance(out, re.Pattern)

def test_re_pattern_passes_through_compiled():
    out = E(r'(let [p #"\d+"] (identical? p (re-pattern p)))')
    assert out is True


# --- re-matcher ---------------------------------------------------

def test_re_matcher_returns_java_matcher():
    out = E(r'(re-matcher #"\d+" "abc 123")')
    assert isinstance(out, JavaMatcher)


# --- re-find (2-arg) ----------------------------------------------

def test_re_find_no_groups_returns_match_string():
    assert E(r'(re-find #"\d+" "hello 42 world")') == "42"

def test_re_find_with_groups_returns_vector():
    out = E(r'(re-find #"(\w+)=(\w+)" "name=alice")')
    assert list(out) == ["name=alice", "name", "alice"]

def test_re_find_no_match_returns_nil():
    assert E(r'(re-find #"\d+" "no digits")') is None

def test_re_find_finds_first_match_only():
    """re-find on (pattern, string) returns just the first match."""
    assert E(r'(re-find #"\d+" "1 2 3")') == "1"


# --- re-find (1-arg, on a matcher) -------------------------------

def test_re_find_matcher_advances():
    """Calling re-find with a matcher returns successive matches and
    nil at end."""
    out = E(r'''(let [m (re-matcher #"\d+" "a 1 b 22")]
                 [(re-find m) (re-find m) (re-find m)])''')
    parts = list(out)
    assert parts[0] == "1"
    assert parts[1] == "22"
    assert parts[2] is None

def test_re_find_matcher_with_groups():
    out = E(r'''(let [m (re-matcher #"(\w+)=(\w+)" "a=1 b=2")]
                 [(re-find m) (re-find m)])''')
    parts = list(out)
    assert list(parts[0]) == ["a=1", "a", "1"]
    assert list(parts[1]) == ["b=2", "b", "2"]


# --- re-matches ---------------------------------------------------

def test_re_matches_full_match_returns_string():
    assert E(r'(re-matches #"\d+" "123")') == "123"

def test_re_matches_with_groups_returns_vector():
    out = E(r'(re-matches #"(\w+)=(\w+)" "alice=42")')
    assert list(out) == ["alice=42", "alice", "42"]

def test_re_matches_partial_match_returns_nil():
    """re-matches requires the entire string to match."""
    assert E(r'(re-matches #"\d+" "abc 123")') is None

def test_re_matches_no_match_returns_nil():
    assert E(r'(re-matches #"\d+" "hello")') is None


# --- re-seq -------------------------------------------------------

def test_re_seq_no_groups():
    out = list(E(r'(re-seq #"\d+" "a 1 b 22 c 333")'))
    assert out == ["1", "22", "333"]

def test_re_seq_with_groups():
    out = list(E(r'(re-seq #"(\w+)=(\w+)" "a=1 b=2 c=3")'))
    assert [list(g) for g in out] == [
        ["a=1", "a", "1"],
        ["b=2", "b", "2"],
        ["c=3", "c", "3"],
    ]

def test_re_seq_no_matches_returns_nil():
    assert E(r'(re-seq #"\d+" "hello")') is None

def test_re_seq_lazy():
    """re-seq returns a lazy seq; takes-from-it work efficiently."""
    out = list(E(r'(take 2 (re-seq #"\d+" "1 2 3 4 5 6 7 8 9 10"))'))
    assert out == ["1", "2"]


# --- re-groups ----------------------------------------------------

def test_re_groups_after_find():
    out = E(r'''(let [m (re-matcher #"(\w+)=(\w+)" "k=v")]
                 (re-find m)
                 (re-groups m))''')
    assert list(out) == ["k=v", "k", "v"]

def test_re_groups_no_groups_returns_match_string():
    out = E(r'''(let [m (re-matcher #"\d+" "abc 42 def")]
                 (re-find m)
                 (re-groups m))''')
    assert out == "42"


# --- regex-literal compiler support -------------------------------

def test_regex_literal_evaluates_to_pattern():
    import re
    out = E(r'#"\d+"')
    assert isinstance(out, re.Pattern)

def test_regex_literal_in_let_binding():
    out = E(r'(let [p #"\w+"] (re-find p "hello world"))')
    assert out == "hello"
