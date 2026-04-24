"""Stdlib-only REPL.

No third-party dependencies. Uses `readline` if available (POSIX
systems have it in the stdlib) for line editing + Ctrl-R history
search; falls back to plain `input()` on platforms without it.

Designed to work identically whether stdin is a tty or piped — piped
input just suppresses the prompt.
"""

from __future__ import annotations

import os
import sys
from typing import Optional

from .core import (
    EOF,
    _ensure_history_vars,
    eval_and_print,
    install_repl_helpers,
    read_form,
)


HISTORY_FILE = os.path.expanduser("~/.clojure-py_history")


def _init_readline() -> bool:
    """Set up line editing + persistent history. Returns True on success."""
    try:
        import readline  # type: ignore[import-not-found]
    except ImportError:
        return False
    try:
        readline.read_history_file(HISTORY_FILE)
    except (OSError, FileNotFoundError):
        pass
    # Reasonable history cap — avoids the file growing unbounded.
    readline.set_history_length(10_000)
    import atexit
    atexit.register(_save_history)
    return True


def _save_history() -> None:
    try:
        import readline  # type: ignore[import-not-found]
        readline.write_history_file(HISTORY_FILE)
    except Exception:
        pass


def run(*, banner: bool = True, show_traceback: bool = False) -> int:
    """Run the simple REPL. Returns a process exit code.

    When stdin is not a tty, prompts are suppressed — suitable for
    `echo '(+ 1 2)' | python -m clojure --simple`.
    """
    _ensure_history_vars()
    install_repl_helpers()

    is_tty = sys.stdin.isatty()
    if is_tty:
        _init_readline()
        if banner:
            _print_banner()

    def _read_line(continuation: bool) -> Optional[str]:
        prompt = ""
        if is_tty:
            prompt = "... " if continuation else _current_prompt()
        try:
            return input(prompt)
        except EOFError:
            return None

    while True:
        try:
            form = read_form(_read_line)
        except Exception as e:  # noqa: BLE001 — includes ReaderError
            print(f"{type(e).__name__}: {e}", file=sys.stderr)
            continue
        if form is EOF:
            if is_tty:
                print()  # newline after ^D
            return 0
        eval_and_print(form, show_traceback=show_traceback)


def _current_prompt() -> str:
    """`clojure.user=> ` style prompt that tracks the live compilation
    namespace — updated by `(in-ns ...)` / `(ns ...)`."""
    try:
        from clojure import _core as _c
        return f"{_c.current_ns_name()}=> "
    except Exception:
        return "=> "


def _print_banner() -> None:
    # Match the Clojure-version tuple we define in core.clj.
    try:
        from clojure import _core as _c
        version = _c.eval_string("(clojure-version)")
    except Exception:
        version = "unknown"
    print(f"clojure-py {version}")
    print("Type (doc 'sym) for docs, (pst) for full stack, Ctrl-D to exit.")


if __name__ == "__main__":
    raise SystemExit(run())
