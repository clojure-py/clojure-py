"""Tests for `python -m clojure <file>` script-loading behaviour."""

import subprocess
import sys


def _python() -> str:
    """Return the python executable used to run tests (3.14t)."""
    return sys.executable


def test_run_script_executes_file(tmp_path):
    script = tmp_path / "hello.clj"
    script.write_text("(println \"hello from script\")\n")
    result = subprocess.run(
        [_python(), "-m", "clojure", str(script)],
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 0, result.stderr
    assert "hello from script" in result.stdout


def test_run_script_missing_file(tmp_path):
    result = subprocess.run(
        [_python(), "-m", "clojure", str(tmp_path / "nope.clj")],
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 2
    assert "no such file" in result.stderr
