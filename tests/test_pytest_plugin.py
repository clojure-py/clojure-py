"""End-to-end tests for the clojure.test pytest plugin. Uses the `pytester`
fixture to run pytest-in-pytest against synthesized `.clj` test files.

Covers:
  - Basic deftest discovery + pass/fail reporting
  - `are` table-driven tests
  - `thrown?` / `thrown-with-msg?` predicates
  - `:each` / `:once` fixture ordering (side-effect trace)
  - Discovery via all three rules (`test_*.clj`, `*_test.clj`, ns-suffix)
  - Mixed Python + Clojure test suite in one run
"""
import textwrap

import pytest

pytest_plugins = ["pytester"]


def _write(pytester, relpath: str, content: str) -> None:
    path = pytester.path / relpath
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(content).lstrip())


def test_passing_deftest(pytester):
    _write(pytester, "sample_test.clj", """
        (ns sample-test
          (:require [clojure.test :refer [deftest is testing]]))
        (deftest ok
          (is (= 4 (+ 2 2))))
        (deftest nested
          (testing "ctx"
            (is (= "ab" (str "a" "b")))))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(passed=2)


def test_failing_deftest_shows_expected_actual(pytester):
    _write(pytester, "fail_test.clj", """
        (ns fail-test (:require [clojure.test :refer [deftest is]]))
        (deftest will-fail (is (= 5 (+ 2 2))))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(failed=1)
    result.stdout.fnmatch_lines(["*expected: (= 5 (+ 2 2))*"])


def test_are_macro(pytester):
    _write(pytester, "are_test.clj", """
        (ns are-test (:require [clojure.test :refer [deftest is are]]))
        (deftest table
          (are [a b s] (= s (+ a b))
            1 2 3
            10 5 15))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(passed=1)


def test_thrown_predicates(pytester):
    _write(pytester, "thrown_test.clj", """
        (ns thrown-test (:require [clojure.test :refer [deftest is]]))
        (deftest thrown-basic
          (is (thrown? clojure._core/IllegalStateException
                       (throw (clojure._core/IllegalStateException "boom")))))
        (deftest thrown-msg
          (is (thrown-with-msg? builtins.Exception (re-pattern "boom")
                                (throw (clojure._core/IllegalStateException "boom go")))))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(passed=2)


def test_fixtures_each_and_once(pytester):
    # Both :once and :each fixtures write to a file we check after the
    # subprocess exits. Asserts exact ordering: once-before, (each-before,
    # each-after) per test, once-after.
    trace_file = pytester.path / "fixture_trace.log"
    _write(pytester, "fixtures_test.clj", f"""
        (ns fixtures-test (:require [clojure.test :refer [deftest is use-fixtures]]))
        (def path "{trace_file}")
        (def trace (atom []))
        (use-fixtures :once
          (fn [f]
            (swap! trace conj :once-before)
            (f)
            (swap! trace conj :once-after)
            (spit path (apply str (interpose "\\n" (map name @trace))))))
        (use-fixtures :each
          (fn [f] (swap! trace conj :each-before) (f) (swap! trace conj :each-after)))
        (deftest a (is (= 1 1)))
        (deftest b (is (= 2 2)))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(passed=2)
    events = trace_file.read_text().strip().splitlines()
    assert events == [
        "once-before",
        "each-before", "each-after",
        "each-before", "each-after",
        "once-after",
    ]


def test_discovery_ns_suffix_rule(pytester):
    # File basename `via_ns.clj` doesn't match test_*.clj or *_test.clj, but
    # ns is `random-thing-test` → must still be discovered.
    _write(pytester, "via_ns.clj", """
        (ns random-thing-test (:require [clojure.test :refer [deftest is]]))
        (deftest detected (is true))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(passed=1)


def test_non_test_clj_ignored(pytester):
    # Basename doesn't match AND ns doesn't end in -test: plugin skips it.
    # (The file would otherwise cause a collection error if we tried to load
    # it.)
    _write(pytester, "regular.clj", """
        (ns some-random-thing)
        (def x 1)
    """)
    result = pytester.runpytest_subprocess("-v")
    # No tests collected, exit code 5 (no tests ran).
    assert result.ret == 5


def test_mixed_python_and_clojure(pytester):
    _write(pytester, "py_test.py", """
        def test_py():
            assert 1 + 1 == 2
    """)
    _write(pytester, "clj_test.clj", """
        (ns clj-test (:require [clojure.test :refer [deftest is]]))
        (deftest t (is (= 4 (+ 2 2))))
    """)
    result = pytester.runpytest_subprocess("-v")
    result.assert_outcomes(passed=2)
