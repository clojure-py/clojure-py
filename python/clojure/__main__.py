"""`python -m clojure` — launch a REPL or run a script.

Flags:
  --simple          Force the stdlib-only REPL.
  --rich            Force the prompt_toolkit REPL (errors if unavailable).
  --no-banner       Suppress the startup banner.
  -v, --verbose     Show full Python tracebacks on exceptions.
  -e, --eval EXPR   Evaluate EXPR and exit.
  -h, --help        Show this help.

Positional:
  script            Path to a .clj file to load and evaluate.

Default (no --simple/--rich): rich if stdin is a tty AND prompt_toolkit
is importable; simple otherwise. Piped input always uses simple.
"""

from __future__ import annotations

import argparse
import sys


def _parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="python -m clojure",
        description="Clojure-on-Python REPL.",
    )
    mode = p.add_mutually_exclusive_group()
    mode.add_argument("--simple", action="store_true", help="stdlib-only REPL")
    mode.add_argument("--rich", action="store_true", help="prompt_toolkit REPL")
    p.add_argument("--no-banner", action="store_true", help="suppress banner")
    p.add_argument("--no-color", action="store_true",
                   help="disable syntax highlighting (rich REPL only)")
    p.add_argument("-v", "--verbose", action="store_true",
                   help="show full Python tracebacks")
    p.add_argument("-e", "--eval", dest="eval_expr", metavar="EXPR",
                   help="evaluate EXPR and exit")
    p.add_argument("script", nargs="?", default=None,
                   help="path to a .clj file to load and evaluate")
    return p.parse_args(argv)


def _run_one(expr: str, verbose: bool) -> int:
    from clojure import _core as _c
    from clojure.repl.core import (
        _ensure_history_vars,
        eval_and_print,
        install_repl_helpers,
    )
    _ensure_history_vars()
    install_repl_helpers()
    try:
        form = _c.eval_string(f"(read-string {_clj_string(expr)})")
    except Exception as e:  # noqa: BLE001
        print(f"{type(e).__name__}: {e}", file=sys.stderr)
        return 1
    eval_and_print(form, show_traceback=verbose)
    return 0


def _run_script(path: str, verbose: bool) -> int:
    """Load a .clj file into a fresh ns and return."""
    import os
    from clojure._core import (
        create_ns,
        load_file_into_ns,
        symbol,
    )
    if not os.path.exists(path):
        print(f"error: no such file: {path}", file=sys.stderr)
        return 2
    # Derive a ns name from the file path: strip extension, replace os.sep with '.'.
    rel = os.path.splitext(path)[0]
    ns_name = rel.replace(os.sep, ".").lstrip(".")
    ns = create_ns(symbol(ns_name))
    try:
        load_file_into_ns(path, ns)
    except Exception as e:  # noqa: BLE001
        print(f"{type(e).__name__}: {e}", file=sys.stderr)
        if verbose:
            import traceback
            traceback.print_exc()
        return 1
    return 0


def _clj_string(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _rich_available() -> bool:
    try:
        import prompt_toolkit  # noqa: F401
        return True
    except ImportError:
        return False


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)

    if args.eval_expr is not None:
        return _run_one(args.eval_expr, args.verbose)

    if args.script is not None:
        return _run_script(args.script, args.verbose)

    # Pick a REPL.
    if args.simple:
        mode = "simple"
    elif args.rich:
        if not _rich_available():
            print("error: --rich requires `pip install clojure[repl]`",
                  file=sys.stderr)
            return 2
        mode = "rich"
    else:
        mode = "rich" if (sys.stdin.isatty() and _rich_available()) else "simple"

    if mode == "rich":
        from clojure.repl import rich
        return rich.run(
            banner=not args.no_banner,
            show_traceback=args.verbose,
            color=not args.no_color,
        )
    else:
        from clojure.repl import simple
        return simple.run(banner=not args.no_banner, show_traceback=args.verbose)


if __name__ == "__main__":
    raise SystemExit(main())
