"""Pytest configuration for the clojure-py repo. Loads the
clojure.pytest_plugin so any *_test.clj files under tests/ are
discovered and run as clojure.test deftest forms."""

pytest_plugins = ["clojure.pytest_plugin"]
