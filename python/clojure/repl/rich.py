"""prompt_toolkit-based REPL.

Features:
  * paren-aware multi-line editing (Enter submits iff the buffer parses;
    otherwise inserts a newline and auto-indents)
  * syntax highlighting via Pygments' ClojureLexer
  * tab completion of symbols from the current ns + clojure.core + aliases
  * signature hints (eldoc) in a bottom toolbar for the head-symbol of
    the form the cursor sits in
  * persistent history + Ctrl-R search (prompt_toolkit default)

Missing from this first cut: paredit-style structural editing, tap
sidebar, doc-at-point popup. Add in a follow-up.
"""

from __future__ import annotations

import os
import sys
from typing import Iterable, Optional

from clojure import _core as _c
from .core import (
    EOF,
    _ensure_history_vars,
    eval_and_print,
    install_repl_helpers,
)


HISTORY_FILE = os.path.expanduser("~/.clojure-py_history_rich")


def run(*, banner: bool = True, show_traceback: bool = False,
        color: bool = True) -> int:
    # Lazy imports so `--simple` doesn't pay the startup cost.
    from prompt_toolkit import PromptSession
    from prompt_toolkit.history import FileHistory
    from prompt_toolkit.key_binding import KeyBindings
    from prompt_toolkit.lexers import PygmentsLexer
    from pygments.lexers.jvm import ClojureLexer

    _ensure_history_vars()
    install_repl_helpers()

    kb = _build_key_bindings()
    session_kwargs = dict(
        message=_prompt_text,
        multiline=True,
        history=FileHistory(HISTORY_FILE),
        completer=_ClojureCompleter(),
        complete_while_typing=False,  # only on Tab — less noisy
        bottom_toolbar=_eldoc_toolbar,
        key_bindings=kb,
        enable_history_search=True,
    )
    if color:
        session_kwargs["lexer"] = PygmentsLexer(ClojureLexer)
        session_kwargs["style"] = _build_style()
        session_kwargs["include_default_pygments_style"] = False
    session = PromptSession(**session_kwargs)

    if banner:
        _print_banner()

    while True:
        try:
            text = session.prompt()
        except KeyboardInterrupt:
            continue  # clear the line and re-prompt
        except EOFError:
            print()
            return 0
        if not text.strip():
            continue
        try:
            form = _c.eval_string("(read-string %s)" % _clj_str(text))
        except _c.ReaderError as e:
            print(f"ReaderError: {e}", file=sys.stderr)
            continue
        if form is EOF:
            return 0
        eval_and_print(form, show_traceback=show_traceback)


# --- Prompt / toolbar ---------------------------------------------------


def _prompt_text() -> str:
    # prompt_toolkit accepts a callable re-evaluated per frame — so the
    # prompt picks up (in-ns ...) / (ns ...) switches immediately.
    try:
        return f"{_c.current_ns_name()}=> "
    except Exception:
        return "=> "


def _eldoc_toolbar():
    """Bottom toolbar: show arglists for the head symbol of the form at point.

    prompt_toolkit invokes this each keystroke — keep it fast and
    tolerant of partial input. On any failure we return an empty
    string (prompt_toolkit hides the toolbar when that happens).
    """
    try:
        from prompt_toolkit.application import get_app
        app = get_app()
        buf = app.current_buffer
        text = buf.document.text_before_cursor
        sym = _head_symbol_of_enclosing_form(text)
        if not sym:
            return ""
        arglists = _arglists_for_symbol(sym)
        if not arglists:
            return ""
        return f"{sym}: {arglists}"
    except Exception:
        return ""


def _head_symbol_of_enclosing_form(text: str) -> Optional[str]:
    """Return the first symbol after the innermost unclosed `(` in `text`.

    E.g. for `(map inc [1 2 | 3])` returns "map". Tolerant of partial
    strings and nested forms."""
    depth = 0
    target = -1
    in_str = False
    i = 0
    # Walk forward, tracking the most recent `(` that hasn't been closed.
    open_positions: list[int] = []
    while i < len(text):
        c = text[i]
        if c == "\\" and not in_str:
            i += 2
            continue
        if c == '"':
            in_str = not in_str
        elif not in_str:
            if c == "(":
                open_positions.append(i)
            elif c == ")":
                if open_positions:
                    open_positions.pop()
        i += 1
    if not open_positions:
        return None
    start = open_positions[-1] + 1
    # Skip whitespace after `(`.
    while start < len(text) and text[start].isspace():
        start += 1
    if start >= len(text):
        return None
    # Collect until whitespace / paren / comment.
    end = start
    while end < len(text) and text[end] not in " \t\n\r()[]{}\";":
        end += 1
    sym = text[start:end]
    return sym or None


def _arglists_for_symbol(sym: str) -> str:
    """Render `:arglists` meta for `sym` as a one-line string, if any."""
    try:
        # Resolve in clojure.user first, then clojure.core.
        for ns_name in ("clojure.user", "clojure.core"):
            ns = sys.modules.get(ns_name)
            if not ns:
                continue
            v = ns.__dict__.get(sym)
            if v is None:
                continue
            meta_fn = sys.modules["clojure.core"].__dict__["meta"].deref()
            m = meta_fn(v)
            if m is None:
                return ""
            pr_str = sys.modules["clojure.core"].__dict__["pr-str"].deref()
            # Fetch :arglists off the meta map.
            get_fn = sys.modules["clojure.core"].__dict__["get"].deref()
            kw_arglists = _c.eval_string(":arglists")
            args = get_fn(m, kw_arglists)
            if args is None:
                return ""
            return pr_str(args)
    except Exception:
        return ""
    return ""


# --- Completion ---------------------------------------------------------


class _ClojureCompleter:
    """Tab-completion over the current ns, clojure.core, and aliases."""

    def get_completions(self, document, complete_event):
        from prompt_toolkit.completion import Completion
        word = document.get_word_before_cursor(WORD=True) or ""
        if not word:
            return
        candidates = self._candidates()
        word_lower = word.lower()
        for name in candidates:
            if name.startswith(word) or (
                len(word) >= 2 and name.lower().startswith(word_lower)
            ):
                yield Completion(name, start_position=-len(word))

    def _candidates(self) -> Iterable[str]:
        names: set[str] = set()
        for ns_name in ("clojure.user", "clojure.core"):
            ns = sys.modules.get(ns_name)
            if ns is None:
                continue
            for k in ns.__dict__.keys():
                if k.startswith("_") or k.startswith("-"):
                    continue
                names.add(k)
        # Alias prefixes (foo/ from (require '[x.y :as foo]))
        user_ns = sys.modules.get("clojure.user")
        if user_ns is not None:
            aliases = user_ns.__dict__.get("__clj_aliases__")
            if aliases is not None:
                try:
                    for k in aliases.keys():
                        sym_name = getattr(k, "name", None) or str(k)
                        names.add(sym_name + "/")
                except Exception:
                    pass
        return sorted(names)


# --- Key bindings -------------------------------------------------------


def _build_key_bindings():
    from prompt_toolkit.key_binding import KeyBindings
    from prompt_toolkit.filters import has_focus
    from prompt_toolkit.enums import DEFAULT_BUFFER

    kb = KeyBindings()

    @kb.add("enter", filter=has_focus(DEFAULT_BUFFER))
    def _submit_or_newline(event):
        """Submit iff buffer parses; else insert newline + auto-indent."""
        b = event.current_buffer
        text = b.text
        if _buffer_is_complete(text):
            b.validate_and_handle()
        else:
            indent = _indent_for_newline(text)
            b.insert_text("\n" + " " * indent)

    return kb


def _buffer_is_complete(text: str) -> bool:
    """True if `text` parses as a complete Clojure form.

    Empty / whitespace-only → True (submit = no-op, re-prompts).
    EOF-while-reading → False (need more input).
    Any other reader error → True (let the REPL surface it).
    """
    if not text.strip():
        return True
    try:
        _c.eval_string("(read-string %s)" % _clj_str(text))
        return True
    except _c.ReaderError as e:
        if "EOF while reading" in str(e):
            return False
        return True
    except Exception:
        return True


def _indent_for_newline(text: str) -> int:
    """Compute the column to indent to after Enter mid-form.

    Rule (Lisp standard): indent to the column immediately after the
    innermost unclosed `(`'s first argument. Falls back to the column
    of the `(` + 1 when there's no first arg on the same line.
    """
    # Walk back to find the innermost unclosed open-paren.
    depth = 0
    paren_pos = -1
    i = len(text) - 1
    in_str = False
    # Walk forward more robustly.
    opens: list[int] = []
    j = 0
    while j < len(text):
        c = text[j]
        if c == "\\" and not in_str:
            j += 2
            continue
        if c == '"':
            in_str = not in_str
        elif not in_str:
            if c in "([{":
                opens.append(j)
            elif c in ")]}":
                if opens:
                    opens.pop()
        j += 1
    if not opens:
        return 0
    open_pos = opens[-1]
    # Column of the `(` on its line:
    line_start = text.rfind("\n", 0, open_pos) + 1
    paren_col = open_pos - line_start
    # Scan past `(` + head symbol; if there's a first arg on the same line,
    # use its column. Otherwise indent to paren_col + 2 (after `(f `).
    after_paren = open_pos + 1
    # Skip the head symbol + following whitespace; if we hit a newline
    # before finding a first arg, fall back to paren_col + 2.
    k = after_paren
    while k < len(text) and text[k] not in " \t\n\r()[]{}":
        k += 1
    # Skip ws on this line.
    while k < len(text) and text[k] in " \t":
        k += 1
    if k < len(text) and text[k] not in "\n\r":
        first_arg_col = k - line_start
        return first_arg_col
    return paren_col + 2


# --- Helpers ------------------------------------------------------------


def _build_style():
    """Build a prompt_toolkit Style using ANSI color *names*, not hex.

    Each `ansi*` value is remapped by the terminal to whatever the
    user has configured in their theme (e.g. iTerm colors, GNOME
    terminal palette, Alacritty config). Hex values would bypass this
    and render the same on every terminal, ignoring the user's
    preferences — which is what Pygments' default style does, and why
    the colors looked bad.
    """
    from prompt_toolkit.styles import Style, merge_styles
    from prompt_toolkit.styles.pygments import style_from_pygments_dict
    from pygments.token import Token

    # Clojure syntax colors. Empty string = inherit the terminal's
    # default foreground (so the bulk of code stays in your FG color).
    pygments_style = style_from_pygments_dict({
        Token.Comment:                   "ansibrightblack italic",
        Token.Keyword:                   "ansiyellow",        # defn, let*, etc.
        Token.Keyword.Declaration:       "ansiyellow",
        Token.Literal.String:            "ansigreen",
        Token.Literal.String.Symbol:     "ansimagenta",       # :keyword
        Token.Literal.Number:            "ansicyan",
        Token.Name.Builtin:              "ansiblue",          # core fns like +, map
        Token.Name.Variable:             "",                  # user identifiers
        Token.Name.Function:             "ansiblue",
        Token.Punctuation:               "",                  # parens inherit FG
        Token.Operator:                  "",
        Token.Error:                     "ansired bold",
    })

    # prompt_toolkit UI elements. Every value explicitly names fg+bg
    # because prompt_toolkit's built-in UI defaults use 256-color hex
    # values (e.g. "#888888" on `prompt`) — if we only specify
    # attributes like "bold", the default color bleeds through and
    # hard-codes a gray that ignores the user's theme. `fg:default`
    # resets to the terminal's own foreground.
    ui_style = Style.from_dict({
        "":                              "noinherit",
        "prompt":                        "bold fg:default bg:default",
        "bottom-toolbar":                "fg:default bg:ansibrightblack",
        "bottom-toolbar.text":           "fg:default bg:ansibrightblack",
        "completion-menu":               "bg:ansibrightblack fg:default",
        "completion-menu.completion":    "bg:ansibrightblack fg:default",
        "completion-menu.completion.current": "bold fg:default bg:ansiyellow",
        "completion-menu.meta.completion": "bg:ansibrightblack fg:default",
        "completion-menu.meta.completion.current": "bold fg:default bg:ansiyellow",
        "search":                        "fg:ansiyellow bg:default",
        "search.current":                "fg:ansiyellow bg:default bold",
        "system-toolbar":                "fg:default bg:ansibrightblack",
        # prompt_toolkit's defaults use '#888888' for these transient
        # render states, which the terminal approximates as 256-color
        # index 102 (a fixed gray). Override with `ansibrightblack`
        # so the user's theme controls it.
        "aborting":                      "fg:ansibrightblack bg:default",
        "exiting":                       "fg:ansibrightblack bg:default",
        "line-number":                   "fg:ansibrightblack bg:default",
        "tilde":                         "fg:ansibrightblack bg:default",
    })

    # Merge Pygments syntax style with UI style. UI style wins on conflict.
    return merge_styles([pygments_style, ui_style])


def _clj_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _print_banner() -> None:
    try:
        version = _c.eval_string("(clojure-version)")
    except Exception:
        version = "unknown"
    print(f"clojure-py {version} (rich)")
    print("Tab to complete, Ctrl-D to exit, (doc 'sym) for docs, (pst) for stack.")
