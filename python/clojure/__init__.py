"""clojure-py — Clojure on the Python runtime.

This module re-exports the native extension as ``clojure._core`` so that
``import clojure`` is sufficient to make the Rust runtime discoverable.
The user-facing Clojure-on-Python API will grow here as protocols and
RT helpers gain Python wrappers.
"""

from clojure import _core  # noqa: F401  (re-export for discoverability)
