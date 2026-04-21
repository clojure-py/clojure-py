"""Clojure on Python — PyO3-backed core."""

from clojure import _core  # noqa: F401  — registers types in sys.modules at import time

__all__ = ["_core"]
