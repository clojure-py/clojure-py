"""Property-based fuzzing of (agent ... :error-mode :error-handler) options.

Mirrors vanilla Clojure JVM rule:
- explicit :error-mode wins;
- otherwise :continue if handler is present else :fail.
"""

from hypothesis import given, strategies as st

from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# Strategies for option presence + values.
mode_choices = st.sampled_from([None, ":fail", ":continue"])
handler_choices = st.booleans()


def _build_agent_form(mode, handler):
    """Build a (agent nil ...) source form with the given options."""
    parts = []
    if mode is not None:
        parts.append(f":error-mode {mode}")
    if handler:
        parts.append(":error-handler (fn [_a _e])")
    opts = " ".join(parts)
    return f"(error-mode (agent nil {opts}))"


@given(mode=mode_choices, handler=handler_choices)
def test_resulting_mode_matches_vanilla_rule(mode, handler):
    """The error-mode of an agent matches the vanilla precedence rule."""
    src = _build_agent_form(mode, handler)
    actual = _ev(src)
    if mode is not None:
        expected_kw = mode
    elif handler:
        expected_kw = ":continue"
    else:
        expected_kw = ":fail"
    assert actual == _ev(expected_kw), (
        f"src={src!r}: got {actual!r}, expected {expected_kw}"
    )
