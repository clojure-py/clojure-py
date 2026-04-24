"""Tests for threading macros (->, ->>, ..) and binding macros
(if-let, when-let, if-some, when-some)."""

from clojure._core import eval_string, keyword


def _ev(src):
    return eval_string(src)


# --- -> (thread-first) ---

def test_arrow_simple_chain():
    assert _ev("(-> 5 inc inc dec)") == 6


def test_arrow_into_expr_inserts_as_second():
    assert _ev("(-> 10 (+ 5))") == 15
    assert _ev("(-> 10 (- 3))") == 7


def test_arrow_builds_map():
    m = _ev("(-> {} (assoc :a 1) (assoc :b 2))")
    assert _ev("(get (-> {} (assoc :a 1) (assoc :b 2)) :a)") == 1


# --- ->> (thread-last) ---

def test_thread_last_reduce():
    assert _ev("(->> [1 2 3 4] (reduce +))") == 10


def test_thread_last_chain():
    # (->> [1 2 3] (map inc) (reduce +)) — map isn't ported yet, so use a simpler chain.
    assert _ev("(->> 5 (+ 10) (* 2))") == 30


# --- .. (member access chain) ---

def test_dot_dot_builds_chain():
    # Build a fresh symbol and grab its name via .-name — .. is just nested ..
    assert _ev("(.. 'foo/bar -name)") == "bar"
    assert _ev("(.. :ns/kw -name)") == "kw"


# --- if-let ---

def test_if_let_true():
    assert _ev("(if-let [x (+ 1 2)] x :none)") == 3


def test_if_let_false_branches():
    assert _ev("(if-let [x nil] x :none)") == keyword("none")
    assert _ev("(if-let [x false] x :none)") == keyword("none")


def test_if_let_no_else_returns_nil():
    assert _ev("(if-let [x nil] x)") is None


# --- when-let ---

def test_when_let_binds_on_truthy():
    assert _ev("(when-let [x 5] (* x x))") == 25


def test_when_let_nil_returns_nil():
    assert _ev("(when-let [x nil] :body)") is None
    assert _ev("(when-let [x false] :body)") is None


# --- if-some ---

def test_if_some_binds_on_non_nil():
    # false is non-nil, so body runs.
    assert _ev("(if-some [x false] :got-false :nil)") == keyword("got-false")
    assert _ev("(if-some [x 0] x :nil)") == 0


def test_if_some_nil_goes_else():
    assert _ev("(if-some [x nil] :got :nil)") == keyword("nil")


# --- when-some ---

def test_when_some_nil_short_circuits():
    assert _ev("(when-some [x nil] :body)") is None


def test_when_some_runs_body_on_non_nil():
    assert _ev("(when-some [x 42] (inc x))") == 43
    # Even `false` runs the body (non-nil).
    assert _ev("(when-some [x false] :body)") == keyword("body")
