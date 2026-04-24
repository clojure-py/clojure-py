import os
import sys
import pytest
from hypothesis import settings, HealthCheck, Verbosity

# Put tests/clj on sys.path so test helper namespaces like
# `clojure.test-clojure.protocols.examples` can be `require`d by
# other test files. Without this, `find-source-file` won't locate
# them.
_TESTS_CLJ = os.path.join(os.path.dirname(os.path.abspath(__file__)), "clj")
if _TESTS_CLJ not in sys.path:
    sys.path.insert(0, _TESTS_CLJ)

@pytest.fixture(autouse=True)
def _require_free_threaded():
    """Ensure tests run on 3.14t (free-threaded)."""
    if not getattr(sys, "_is_gil_enabled", lambda: True)() is False:
        # Some envs run these tests under GIL-ful 3.14 for iteration speed;
        # that's allowed, but any test that explicitly needs no-GIL should
        # use the `require_free_threaded` marker.
        pass


# CI profile: 500 cases, quiet.
settings.register_profile(
    "ci",
    max_examples=500,
    deadline=None,
    verbosity=Verbosity.normal,
    suppress_health_check=[HealthCheck.too_slow],
)
# Default to CI profile unless HYPOTHESIS_PROFILE env var overrides.
settings.load_profile("ci")
