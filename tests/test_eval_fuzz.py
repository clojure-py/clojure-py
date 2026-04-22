"""Phase E5 — evaluator property-based fuzz: arithmetic round-trip."""

from hypothesis import given, settings, strategies as st
from clojure._core import eval_string, read_string, pr_str


# --- Arithmetic expression fuzz ---

@st.composite
def arithmetic_expr(draw, depth=0):
    """Generate a small arithmetic expression."""
    if depth > 3 or draw(st.booleans()):
        return draw(st.integers(min_value=-100, max_value=100))
    op = draw(st.sampled_from(["+", "-", "*"]))
    arity = draw(st.integers(min_value=2, max_value=4))
    operands = [draw(arithmetic_expr(depth + 1)) for _ in range(arity)]
    return (op, operands)


def render(expr):
    """Render a generated expression as a Clojure string."""
    if isinstance(expr, int):
        return str(expr)
    op, operands = expr
    return "(" + op + " " + " ".join(render(o) for o in operands) + ")"


def compute(expr):
    """Python reference computation."""
    if isinstance(expr, int):
        return expr
    op, operands = expr
    vals = [compute(o) for o in operands]
    if op == "+": return sum(vals)
    if op == "-":
        r = vals[0]
        for v in vals[1:]: r -= v
        return r
    if op == "*":
        r = 1
        for v in vals: r *= v
        return r
    raise ValueError(op)


@given(arithmetic_expr())
@settings(max_examples=200, deadline=None)
def test_arithmetic_matches_python(expr):
    src = render(expr)
    expected = compute(expr)
    got = eval_string(src)
    assert got == expected, f"{src!r}: expected {expected}, got {got}"


# --- Eval idempotence via reader+printer ---

@given(arithmetic_expr())
@settings(max_examples=100, deadline=None)
def test_eval_idempotent_through_pr(expr):
    """eval(parse(pr(parse(src)))) == eval(src)."""
    src = render(expr)
    form = read_string(src)
    reprinted = pr_str(form)
    assert eval_string(reprinted) == eval_string(src)
