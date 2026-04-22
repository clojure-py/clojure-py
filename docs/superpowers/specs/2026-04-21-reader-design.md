# Lisp Reader — Design Spec

**Date:** 2026-04-21
**Status:** Draft for implementation
**Scope:** Third sub-project of the clojure-py revival. Ports Clojure-JVM's `LispReader.java` to Rust as a recursive-descent parser. Produces Clojure data (using the persistent collections + Symbol/Keyword from prior specs). Ships with a printer so reader output round-trips.

---

## 1. Goal

`read-string "(def x [1 2 3])"` → `(PersistentList quote? def? x [1 2 3])` — a parsed Clojure form ready for the evaluator.

After this spec:
- Python code can call `read_string(s) → PyObject` and get back Clojure data.
- `pr_str(x) → str` renders any Clojure value back to its textual form.
- The two are inverses over the supported reader surface (round-trip property-tested).

This is the last prerequisite before the evaluator spec. With the reader + data, the evaluator can ingest source code.

**Core design tenets:**

1. **Recursive-descent, like Clojure-JVM's `LispReader.java`** — a top-level `read_one` function that dispatches on the first character to sub-readers (list-reader, vector-reader, string-reader, number-parser, symbol-parser). Keeps the code structure 1-to-1 with the reference.
2. **Source abstraction** — a `Source` struct that wraps `&str` or (stretch) a Python stream, tracks line/column, supports peek/advance/unread of one char.
3. **Metadata-as-you-go** — line/col info attached to forms via `IMeta::with_meta` wherever the form type supports it (lists, vectors, maps, sets, symbols).
4. **No evaluation** — the reader only produces data. Reader macros that expand to calls (e.g., `'x` → `(quote x)`) produce **data** `(quote x)` as a list, not a symbol resolution.
5. **Minimum scope** — defer syntax-quote, reader conditionals, tagged literals, fn-literal, regex, namespaced maps, numeric suffixes. Add them in follow-on specs.

---

## 2. Scope

### In scope

**Atoms:**
- `nil`, `true`, `false`
- Integers (Python `int`, arbitrary precision via Rust `i128` then fall through to Python BigInt)
- Floats (Python `float`, IEEE-754)
- Strings `"..."` with escapes `\n \t \r \\ \" \0 \uHHHH`
- Characters `\a`, `\space`, `\newline`, `\tab`, `\return`, `\backspace`, `\formfeed`, `\uHHHH`
- Symbols (unqualified and `ns/name`)
- Keywords `:foo`, `:ns/foo`

**Collections:**
- Lists `(...)` → `PersistentList`
- Vectors `[...]` → `PersistentVector` (built via transient for efficiency)
- Maps `{...}` → `PersistentHashMap` or `PersistentArrayMap` depending on size (use `hash_map(*pairs)` constructor which does the right thing)
- Sets `#{...}` → `PersistentHashSet` (duplicate-key check raises)

**Reader macros:**
- `'form` → `(quote form)` (a 2-list)
- `@form` → `(deref form)`
- `^meta form` / `#^meta form` — attaches `meta` as metadata to the next form
- `#'sym` → `(var sym)`
- `;` end-of-line comment (skipped)
- `#_` discard the next form
- `\` character literal (already covered by Atoms)

**Line/column tracking:** each compound form (list/vector/map/set) gets a `{:line N, :column M}` metadata map attached via `IMeta::with_meta`. Symbols gain metadata when they originate in a form that tracked position; this is a soft invariant — if extracting line/col is awkward for an atom, the reader may skip attaching.

**Public API:**
- `read_string(s: str) → PyObject` — reads the first form from `s`; raises if the string contains more than one form or ends mid-form. Also `read_all(s: str) → Py<PersistentVector>` reads every form.
- `pr_str(x: PyObject) → str` — renders `x` in reader form.
- `pr(x: PyObject)` — prints `pr_str(x)` to stdout + newline.

### Deferred

- Syntax-quote `` ` ``, unquote `~`, unquote-splicing `~@` (requires gensym + namespace resolution — evaluator spec)
- Reader conditionals `#?` / `#?@`
- Tagged literals `#inst "..."`, `#uuid "..."`, user-registered tags
- Fn literal `#(...)`
- Regex literal `#"..."`
- Namespaced maps `#:ns{:k v}`
- Numeric suffixes: `N` (BigInt), `M` (BigDecimal), `r` (radix)
- Hex/octal/binary number literals (`0xFF`, `0o17`, `0b1010`)
- Auto-resolved keywords `::foo`, `::alias/foo`
- Shebang `#!`
- Multi-form `read` from a stream (we support single-form read-string; stream support can add in a follow-on)
- EDN-restricted mode (full-reader is a superset; EDN-only entry point is a future spec)

---

## 3. Architecture

### 3.1 File layout

```
crates/clojure_core/src/
  reader/
    mod.rs              # public API: read_string, read_all + registry init
    source.rs           # Source abstraction: char peek/advance/unread + line/col
    lexer.rs            # Char classifiers (whitespace, delimiter, number-start, etc.)
    number.rs           # Integer + float parser
    string.rs           # String + char literal parsers (escape handling)
    token.rs            # Symbol / keyword / nil / true / false parser
    forms.rs            # Per-reader-macro functions: list_reader, vector_reader, map_reader,
                        # set_reader, quote_reader, deref_reader, meta_reader,
                        # var_quote_reader, comment_reader, discard_reader
    dispatch.rs         # Char-indexed dispatch table + `#`-dispatch table
    errors.rs           # ReaderError type with line/col context
  printer/
    mod.rs              # public API: pr, pr_str + registry init
    print.rs            # Per-type rendering via Rust match on concrete type
```

### 3.2 Module dependencies

```
collections (PersistentList, PersistentVector, PersistentHashMap, PersistentHashSet, MapEntry)
symbol, keyword (from core-abstractions)
exceptions (IllegalArgumentException)
    │
    ▼
reader/source          # wraps &str, tracks line/col
    │
    ▼
reader/{lexer, number, string, token, forms, dispatch, errors}
    │
    ▼
reader/mod              # public API: read_string, read_all
    │
    ▼
printer/{mod, print}    # public API: pr, pr_str
```

### 3.3 Control flow — `read_string`

```
fn read_string(s: &str) -> PyResult<PyObject>:
    let mut src = Source::new(s)
    skip_whitespace_and_comments(&mut src)
    if src.at_eof():
        raise ReaderError("EOF while reading")
    let form = read_one(&mut src)?
    skip_whitespace_and_comments(&mut src)
    if !src.at_eof():
        raise ReaderError("Unexpected trailing content")
    Ok(form)
```

### 3.4 `read_one` — recursive core

```
fn read_one(src: &mut Source) -> PyResult<PyObject>:
    skip_whitespace_and_comments(src)
    let start_line = src.line
    let start_col = src.column
    let ch = src.peek_expect()?
    let form = match dispatch_macro(ch):
        Some(macro_fn) => macro_fn(src)?,
        None => match ch:
            '0'..='9' | '+' | '-' if looks_like_number(src) => read_number(src),
            _ => read_token(src),   // nil/true/false/symbol/keyword
    // Attach line/col metadata if form supports IMeta:
    let form_with_meta = maybe_attach_line_col(form, start_line, start_col)?
    Ok(form_with_meta)
```

Dispatch table (char → reader fn):

| Char | Reader |
|---|---|
| `(` | `list_reader` |
| `[` | `vector_reader` |
| `{` | `map_reader` |
| `"` | `string_reader` |
| `\` | `char_reader` |
| `:` | `keyword_reader` |
| `'` | `quote_reader` |
| `@` | `deref_reader` |
| `^` | `meta_reader` |
| `;` | `comment_skipper` (then re-entrant read) |
| `#` | `dispatch_hash_reader` — secondary table |
| `)` `]` `}` | Delimiter mismatch — ReaderError |

`#`-dispatch secondary table:

| Chars | Reader |
|---|---|
| `#'` | `var_quote_reader` → `(var sym)` |
| `#{` | `set_reader` |
| `#_` | `discard_reader` — skip next form and recursively read again |
| `#^` | `meta_reader` (alt form) |
| `#"` | future: regex (not in scope; raise ReaderError "regex literals not supported yet") |
| `#(` | future: fn literal (raise ReaderError) |
| `#?` | future: reader conditional (raise ReaderError) |
| `#inst` / `#uuid` / `#<tag>` | future: tagged literal (raise ReaderError) |

### 3.5 Collection readers

Each follows the same shape:

```
fn vector_reader(src) -> PyResult<PyObject>:
    consume '['
    let mut t = transient(empty_vector())
    loop:
        skip_whitespace_and_comments(src)
        if src.peek() == Some(']'):
            consume ']'
            return persistent_bang(t)
        let element = read_one(src)?
        t = conj_bang(t, element)
    end
```

Same for list/map/set, using their respective transient APIs. Maps check odd-count and raise; sets check duplicate and raise.

### 3.6 Printer

`pr_str(x)` walks `x` and renders:

| Type | Output |
|---|---|
| `None` (Python) | `nil` |
| `True` / `False` | `true` / `false` |
| `int` | `str(x)` |
| `float` | `str(x)` |
| `str` | `"..."` with `\n`/`\t`/`\"`/`\\` escaped |
| `Keyword` | `:ns/name` or `:name` |
| `Symbol` | `ns/name` or `name` |
| `PersistentList` | `(pr_str a pr_str b ...)` |
| `PersistentVector` | `[pr_str a pr_str b ...]` |
| `PersistentHashMap` / `PersistentArrayMap` | `{pr_str k pr_str v, ...}` |
| `PersistentHashSet` | `#{pr_str a pr_str b ...}` |
| `Cons` / `LazySeq` | `(... realize if lazy ...)` — same as list form |
| `Var` | `#'ns/sym` |

Implementation: a single function with a big Rust `match` (via downcast on each known type) + a final fallback that calls the object's `__repr__` (for types we don't know).

No user-extensibility yet (`print-method` multimethod is a later spec).

---

## 4. Error Handling

`ReaderError` extends `IllegalArgumentException` (so callers can catch it as a general "bad input"). Every error message includes `(line L, col C)` context.

Examples:
- `"EOF while reading (at line 3, col 7)"`
- `"Unmatched delimiter: expected ')' but got ']' (at line 5, col 12)"`
- `"Map literal must have an even number of forms (at line 1, col 1)"`
- `"Duplicate key in set literal: :foo (at line 2, col 4)"`

---

## 5. Testing Bar

### 5.1 Unit tests per reader (in `tests/test_reader.py`)

- Atoms: `nil`, `true`, `false`, `0`, `-42`, `3.14`, `"hello"`, `"a\nb"`, `\a`, `\space`, `\newline`, `\uHHHH`, `foo`, `my.ns/foo`, `:foo`, `:ns/foo`.
- Lists: `()`, `(1 2 3)`, `(foo (bar baz))`.
- Vectors: `[]`, `[1 2 3]`, `[[1 2] [3 4]]`.
- Maps: `{}`, `{:a 1 :b 2}`, nested `{:x {:y 1}}`.
- Sets: `#{}`, `#{1 2 3}`, duplicate-key error.
- Reader macros: `'x` → `(quote x)`, `@a` → `(deref a)`, `#'foo` → `(var foo)`, `^:meta form`, `;comment\nform`, `#_skip form`.
- Metadata attachment: `(meta (read-string "[1 2]"))` → `{:line 1 :column 1}`.
- Errors: unmatched `]`, `"` without close, `{` with odd entries, etc. — each raises ReaderError with position info.

### 5.2 Printer round-trip

For every example in the unit tests, assert `read_string(pr_str(read_string(input))) == read_string(input)` (equality via `rt::equiv`).

### 5.3 Property-based fuzzing (hypothesis)

- Strategy that generates arbitrary Clojure data (nil, ints, strings, keywords, nested lists/vectors/maps/sets).
- For each generated form `x`, assert `read_string(pr_str(x))` is structurally equal to `x` via `rt::equiv`.
- 200 cases.

### 5.4 Stress test

- Parse 10k-element vector literal; confirm length + a sampling of indices.
- Parse 10k-entry map literal; confirm count + random val_at sampling.

---

## 6. Non-Goals / Follow-on Specs

1. **Syntax-quote + unquote** — requires gensym + namespace resolution. Evaluator-adjacent.
2. **Reader conditionals** — `#?` / `#?@`.
3. **Tagged literals** — `#inst`, `#uuid`, user-defined `data_readers` registry.
4. **Fn literal `#(...)`** — needs the evaluator to instantiate the anonymous fn.
5. **Regex literal** — once we have a regex strategy.
6. **Numeric suffixes** — `N`, `M`, `r` radix, hex/octal/binary literals.
7. **Auto-resolved keywords** — `::foo` (needs current-ns resolution).
8. **EDN-restricted mode** — a separate `edn/read-string` that refuses reader macros like `#'`.
9. **Stream-based `read`** — incremental reading from a Python readable-like.
10. **`print-method` multimethod** — user-extensible printing.

This spec leaves hooks for (1-7) — the dispatch tables in `dispatch.rs` have explicit "not supported yet" stubs for each so upgrading is additive.
