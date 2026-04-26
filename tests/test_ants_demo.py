"""Smoke test: load the Ants demo without entering the Tk mainloop and
verify the world / agents / behave round-trip don't blow up.

Sets ANTS_NO_GUI=1 so the demo's auto-launch is skipped.
"""

import os
import subprocess
import sys


def test_ants_loads_and_setup_runs():
    """Loading the file must succeed (with the GUI suppressed), and a
    quick exercise of setup + one tick of behave must not raise."""
    code = """(do
  (load-file "examples/ants/ants.clj")
  (let [ants (examples.ants/setup)]
    (println (count ants))
    (clojure.core/send-off (first ants) examples.ants/behave)
    (println "ok")))
"""
    env = dict(os.environ)
    env["ANTS_NO_GUI"] = "1"
    result = subprocess.run(
        [sys.executable, "-m", "clojure", "-e", code],
        capture_output=True, text=True, env=env, timeout=30,
    )
    assert result.returncode == 0, (
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert "49" in result.stdout      # ant-count
    assert "ok" in result.stdout
