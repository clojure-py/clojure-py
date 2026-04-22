# Lisp Reader Implementation Plan

> **For agentic workers:** Steps use checkbox syntax. Each phase is a bite-sized task dispatch.

**Goal:** Port Clojure-JVM's `LispReader.java` to Rust. Recursive-descent parser with char-indexed dispatch. Plus a printer for round-trip.

**Architecture:** Reader module with `Source` + `read_one` dispatching on first char; sub-readers for atoms/collections/macros. Printer as a Rust `match` on concrete type.

**Tech Stack:** pyo3 0.28, existing collections (PersistentList/Vector/HashMap/HashSet) + Symbol/Keyword. `hypothesis` for round-trip fuzz.

**Spec:** `docs/superpowers/specs/2026-04-21-reader-design.md`

---

## Phase R1: Source abstraction + ReaderError + number/string/char parsers

Foundation. `Source` wraps `&str` + tracks line/col. `ReaderError` extends `IllegalArgumentException` with position info. Low-level parsers for numbers, strings, characters.

**Files:**
- Create: `crates/clojure_core/src/reader/mod.rs`, `source.rs`, `lexer.rs`, `number.rs`, `string.rs`, `errors.rs`
- Modify: `crates/clojure_core/src/lib.rs` — add `mod reader;`
- Create: `tests/test_reader_source.py`

**Deliverables:** Source tests (peek/advance/unread/line-col), number parser (int + float with sign), string parser (escapes), char parser (`\a`, `\space`, `\uHHHH`).

## Phase R2: Token parser (nil/true/false/symbol/keyword) + read_one skeleton

**Files:**
- Create: `crates/clojure_core/src/reader/token.rs`, `dispatch.rs`
- Append to `reader/mod.rs`: `read_string`, `read_one` dispatching to atom parsers (collection readers stubbed to error until Phase R3).

**Deliverables:** `read_string("nil") == None`, `read_string("true") == True`, `read_string("42") == 42`, `read_string("foo") == symbol("foo")`, `read_string(":foo") == keyword("foo")`, etc.

## Phase R3: Collection readers

**Files:**
- Create: `crates/clojure_core/src/reader/forms.rs`
- Wire list/vector/map/set readers into dispatch.rs

**Deliverables:** `read_string("(1 2 3)")`, `read_string("[1 2 3]")`, `read_string("{:a 1}")`, `read_string("#{1 2}")` all work. Odd-count maps raise; duplicate-set raises.

## Phase R4: Reader macros + line/col metadata

**Files:**
- Append to `reader/forms.rs`: `quote_reader`, `deref_reader`, `var_quote_reader`, `meta_reader`, `comment_skipper`, `discard_reader`
- Wire into dispatch + `#`-dispatch

**Deliverables:** `'x` → `(quote x)`, `@x` → `(deref x)`, `^{:a 1} foo` attaches meta, `;comment\nx` reads `x`, `#_ skipped form` reads next.

## Phase R5: Printer

**Files:**
- Create: `crates/clojure_core/src/printer/mod.rs`, `print.rs`
- Modify: `lib.rs` — add `mod printer;`

**Deliverables:** `pr_str(x)` for every type in the reader output. `pr(x)` prints.

## Phase R6: Round-trip + hypothesis fuzz + integration tests

**Files:**
- Create: `tests/test_reader.py` (unit tests if not already), `tests/test_reader_roundtrip.py` (hypothesis fuzz)

**Deliverables:** for arbitrary generated Clojure data `x`, `read_string(pr_str(x))` equals `x` via `rt::equiv`. 200 cases.

---

## Execution Status

All 6 phases landed. **446 pytest tests passing.**

| Phase | Description | Commit |
|---|---|---|
| R1 | Source + ReaderError + number/string/char parsers | `467fe41` |
| R2 | read_string on atoms (nil/bool/int/float/string/char/symbol/keyword) | `33faca4` |
| R3 | Collection readers ((), [], {}, #{}) | `3f68d06` |
| R4 | Reader macros (', @, #', ^, ;, #_) + meta attachment | `a35d863` |
| R5 | Printer (pr, pr_str) — round-trip capable | `5e0af07` |
| R6 | hypothesis round-trip fuzzing + phashmap cross-type equality fix | `585b5b1` |

**Real bug caught by R6 fuzzing** (documented in commit `585b5b1`): `PersistentHashMap::equiv` wasn't symmetric with `PersistentArrayMap::equiv`. Direction-dependent `x == y` vs `y == x`. Fixed by extending PHashMap's equiv to cross-type-match PArrayMap. Exactly what property-based testing is for.

**Deferred** (explicit non-goals from the spec — future specs):

- Syntax-quote (\`), unquote (~), unquote-splicing (~@) — evaluator-adjacent
- Reader conditionals #? / #?@
- Tagged literals #inst, #uuid, user-registered
- Fn-literal #(...)
- Regex literal #"..."
- Namespaced maps #:ns{...}
- Numeric suffixes: N, M, r (radix), hex/octal/binary literals
- Auto-resolved keywords ::foo
- Shebang #!
- EDN-only mode
- Multi-form read-from-stream

The reader is ready to feed the evaluator.
