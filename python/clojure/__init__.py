"""Clojure on Python — PyO3-backed core."""

from clojure import _core  # noqa: F401  — registers types in sys.modules at import time
from clojure._importer import install as _install_importer

_install_importer()

__all__ = ["_core"]
