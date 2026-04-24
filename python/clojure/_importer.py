"""Python import-machinery integration for Clojure namespaces.

Register `ClojureFinder` on `sys.meta_path` so that `import clojure.test`
(and `from clojure.test import pyrunner`) searches `sys.path` for
`clojure/test.clj`, loads it into a `ClojureNamespace`, and exposes its
interned Vars as module attributes.
"""
from __future__ import annotations

import sys
from importlib.abc import Loader, MetaPathFinder
from importlib.machinery import ModuleSpec

from clojure import _core


class ClojureLoader(Loader):
    def __init__(self, path: str) -> None:
        self._path = path

    def create_module(self, spec):
        # Initial placeholder ns — the file's (ns ...) form is authoritative
        # and exec_module rewires if it declares a different name (e.g.
        # `my_lib.clj` containing `(ns my-lib)`).
        sym = _core.symbol(spec.name)
        return _core.create_ns(sym)

    def exec_module(self, module) -> None:
        terminal_ns = _core.load_file_into_ns(self._path, module)
        # If the file's (ns ...) form switched to a differently-named ns
        # (e.g. `-`/`_` translation), point sys.modules[fullname] at the
        # terminal ns so `from <fullname> import x` reaches the interned
        # Vars. Python's _bootstrap does a pop+re-put of sys.modules[spec.name]
        # after exec_module, so this in-place rewire survives.
        if terminal_ns is not module:
            sys.modules[module.__spec__.name] = terminal_ns


class ClojureFinder(MetaPathFinder):
    def find_spec(self, fullname, path, target=None):
        sym = _core.symbol(fullname)
        source = _core.find_source_file(sym)
        if source is None:
            return None
        return ModuleSpec(fullname, ClojureLoader(source), origin=source)


def install() -> None:
    """Idempotently install a `ClojureFinder` on `sys.meta_path`."""
    for finder in sys.meta_path:
        if isinstance(finder, ClojureFinder):
            return
    sys.meta_path.append(ClojureFinder())
