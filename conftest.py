"""Pytest configuration for the clojure-py repo. Loads the
clojure.pytest_plugin so any .clj files under tests/ are discovered
and run as clojure.test deftest forms.

We add tests/ to sys.path so Clojure's require system (which mirrors
JVM RT.load — searches sys.path for `<ns-path>.clj`) can resolve
`clojure.test-helper` to tests/clojure/test_helper.clj. This mirrors
the JVM convention where test/ is on the classpath alongside src/."""

import os
import sys

# Add tests/ to sys.path. clojure.test-helper sits at
# tests/clojure/test_helper.clj.
_TESTS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "tests")
if _TESTS_DIR not in sys.path:
    sys.path.insert(0, _TESTS_DIR)

pytest_plugins = ["clojure.pytest_plugin"]
