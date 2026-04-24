"""Shared read/eval/print machinery for both REPL front-ends.

- `read_form` accumulates lines until a complete form parses (or the
  reader raises a non-EOF error). Mirrors vanilla's REPL multi-line
  handling.
- `eval_and_print` evaluates a form, rotates `*1`/`*2`/`*3`, binds
  `*e` on error, and prints the result via Clojure's `pr-str`.
- `install_repl_helpers` interns small convenience fns (`pst`, `doc`)
  into `clojure.user`.
"""

from __future__ import annotations

import sys
import traceback
from typing import Callable, Optional

from clojure import _core as _c


# Sentinel for end-of-input (Ctrl-D / stream closed).
EOF = object()


# --- Clojure fn handles cached on first use ------------------------------
#
# Clojure-layer fns live in clojure.core (or clojure.user) as Vars on
# the module object. We grab the callable once and cache it.

_pr_str_fn: Optional[Callable[[object], str]] = None
_read_string_fn: Optional[Callable[[str], object]] = None
_eval_fn: Optional[Callable[[object], object]] = None


def _resolve_core_fns() -> None:
    global _pr_str_fn, _read_string_fn, _eval_fn
    if _pr_str_fn is not None:
        return
    core_ns = sys.modules["clojure.core"]
    _pr_str_fn = core_ns.__dict__["pr-str"].deref()
    _read_string_fn = core_ns.__dict__["read-string"].deref()
    _eval_fn = core_ns.__dict__["eval"].deref()


# --- History vars (*1, *2, *3, *e) ---------------------------------------


def _ensure_history_vars() -> None:
    """First-time init of *1 / *2 / *3 / *e in clojure.user."""
    for name in ("*1", "*2", "*3", "*e"):
        _c.eval_string(f"(def {name} nil)")


def _shift_history(result: object) -> None:
    """Rotate (*1 *2 *3) ← (result *1 *2)."""
    user_ns = sys.modules["clojure.user"]
    v1 = user_ns.__dict__["*1"]
    v2 = user_ns.__dict__["*2"]
    v3 = user_ns.__dict__["*3"]
    try:
        r1 = v1.deref()
    except Exception:
        r1 = None
    try:
        r2 = v2.deref()
    except Exception:
        r2 = None
    v3.bind_root(r2)
    v2.bind_root(r1)
    v1.bind_root(result)


def _set_last_exception(exc: BaseException) -> None:
    user_ns = sys.modules["clojure.user"]
    user_ns.__dict__["*e"].bind_root(exc)


# --- Read (multi-line aware) ---------------------------------------------


def _is_eof_reader_error(exc: BaseException) -> bool:
    """True if the reader failed *because input ran out mid-form*.
    We treat this as "need more input" rather than a real error."""
    return isinstance(exc, _c.ReaderError) and "EOF while reading" in str(exc)


def read_form(read_line: Callable[[bool], Optional[str]]) -> object:
    """Read one complete Clojure form.

    `read_line(continuation)` returns the next line of input, or None
    on EOF. `continuation` is True for prompt-2 lines (the caller may
    display a different prompt).

    Returns:
      * the parsed form, or
      * `EOF` when input is exhausted with no buffered form,
      * raises `ReaderError` on a non-EOF reader failure
        (malformed input that won't parse no matter what).
    """
    _resolve_core_fns()
    buf = ""
    continuation = False
    while True:
        line = read_line(continuation)
        if line is None:
            return EOF
        buf += line + "\n"
        if not buf.strip():
            # Blank input — keep waiting (ignore the blank).
            buf = ""
            continuation = False
            continue
        try:
            return _read_string_fn(buf)
        except _c.ReaderError as e:
            if _is_eof_reader_error(e):
                continuation = True
                continue
            raise


# --- Eval + print --------------------------------------------------------


def eval_and_print(
    form: object,
    *,
    print_fn: Callable[[str], None] = print,
    err_fn: Callable[[str], None] = lambda s: print(s, file=sys.stderr),
    show_traceback: bool = False,
) -> None:
    """Evaluate `form`, rotate history vars, and print via pr-str.

    On exception: prints the error, binds `*e`, returns without
    raising. When `show_traceback` is true, also prints the full
    Python traceback (handy in verbose mode).
    """
    _resolve_core_fns()
    try:
        result = _eval_fn(form)
    except BaseException as e:  # noqa: BLE001
        _set_last_exception(e)
        _print_exception(e, err_fn, show_traceback)
        return
    _shift_history(result)
    try:
        rendered = _pr_str_fn(result)
    except Exception:
        rendered = repr(result)
    print_fn(rendered)


def _print_exception(
    e: BaseException,
    err_fn: Callable[[str], None],
    show_traceback: bool,
) -> None:
    name = type(e).__name__
    msg = str(e).strip()
    err_fn(f"{name}{': ' + msg if msg else ''}")
    if show_traceback:
        for line in traceback.format_exception(type(e), e, e.__traceback__):
            for subline in line.rstrip().splitlines():
                err_fn(subline)


# --- REPL convenience helpers installed into clojure.user ----------------


def install_repl_helpers() -> None:
    """Intern `pst` and `doc` convenience fns in clojure.user.

    `pst` is bound to a Python callable (easier than wiring the
    `traceback` module in through Clojure interop). `doc` is pure
    Clojure — only uses core forms we already ship.
    """
    import traceback as _tb

    def _pst(exc: Optional[BaseException] = None) -> None:
        if exc is None:
            user_ns = sys.modules["clojure.user"]
            v = user_ns.__dict__.get("*e")
            try:
                exc = v.deref() if v is not None else None
            except Exception:
                exc = None
        if exc is None:
            print("no exception to print", file=sys.stderr)
            return
        _tb.print_exception(type(exc), exc, exc.__traceback__)

    # Intern `pst` as a var in clojure.user and bind its root.
    _c.eval_string("(def pst nil)")
    sys.modules["clojure.user"].__dict__["pst"].bind_root(_pst)

    # `doc` is pure Clojure — avoids any Python interop.
    _c.eval_string(
        """
        (defn doc
          "Print the documentation for a var or symbol."
          [x]
          (let [m (cond
                    (var? x)    (meta x)
                    (symbol? x) (some-> (resolve x) meta)
                    :else       (meta x))]
            (when m
              (println "-------------------------")
              (when-let [n (:name m)] (println (str (:ns m) "/" n)))
              (when-let [a (:arglists m)] (println a))
              (when-let [d (:doc m)] (println " " d)))))
        """
    )


__all__ = [
    "EOF",
    "read_form",
    "eval_and_print",
    "install_repl_helpers",
    "_ensure_history_vars",
]
