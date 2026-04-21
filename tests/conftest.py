import sys
import pytest

@pytest.fixture(autouse=True)
def _require_free_threaded():
    """Ensure tests run on 3.14t (free-threaded)."""
    if not getattr(sys, "_is_gil_enabled", lambda: True)() is False:
        # Some envs run these tests under GIL-ful 3.14 for iteration speed;
        # that's allowed, but any test that explicitly needs no-GIL should
        # use the `require_free_threaded` marker.
        pass
