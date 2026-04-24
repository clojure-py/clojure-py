# clojure-py

Clojure core on Python 3.14t, implemented in Rust via PyO3.

## Status

Core abstractions (protocols, IFn, Var, Namespace, Symbol, Keyword) are
in place, along with persistent collections, reader, bytecode evaluator,
a port of `clojure.core`, and a growing port of the vanilla Clojure test
suite.

## Build

Requires CPython 3.14t (free-threaded) and Rust 1.85+.

```bash
# One-time: install 3.14t via uv (or use your system's python-freethreaded package)
uv python install 3.14t

# Create and activate a venv
uv venv --python 3.14t .venv
source .venv/bin/activate

# Install build + test dependencies
uv pip install maturin pytest pytest-timeout

# Build the extension (editable install into the venv)
maturin develop --release
```

## REPL

```bash
# Simple REPL — stdlib only.
python -m clojure --simple

# Rich REPL — paren-aware multi-line input, syntax highlighting,
# symbol completion, eldoc hints. Needs the optional 'repl' extras.
uv pip install "prompt_toolkit>=3.0" "pygments>=2.15"
python -m clojure --rich

# One-shot eval.
python -m clojure -e '(+ 1 2)'

# Auto-pick: rich if tty + prompt_toolkit importable, else simple.
python -m clojure
```

## Test

```bash
source .venv/bin/activate
pytest              # full Python suite
pytest -q           # terse output
```

### Rust concurrency model-checking with loom

```bash
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_method_cache
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_keyword_intern
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_var_root
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_binding_stack
```

## Repository layout

```
Cargo.toml                              # Rust workspace
pyproject.toml                          # maturin build config
crates/
  clojure_core/                         # PyO3 cdylib (the Python extension)
    src/
      exceptions.rs                     # ArityException, IllegalState*, IllegalArgument*
      symbol.rs                         # Symbol (non-interned, value-equal)
      keyword.rs                        # Keyword (globally interned, callable IFn)
      protocol.rs                       # Protocol / ProtocolMethod / MethodCache
      dispatch.rs                       # exact → MRO → metadata → fallback
      ifn.rs                            # IFn trait (invoke0..20 + variadic) + callable fallback
      ilookup.rs                        # ILookup trait + __getitem__ fallback
      var.rs                            # Var + delegation dunders
      binding.rs                        # Thread-local binding stack
      bound_fn.rs                       # bound-fn* cross-thread conveyance
      pmap.rs                           # Minimal persistent map for bindings
      namespace.rs                      # ClojureNamespace + create-ns
      ns_ops.rs                         # intern / refer / alias / import / ns-*
      rt.rs                             # rt::get, rt::invoke_n — protocol-routed
      registry.rs                       # inventory-based init registration
      test_protocols.rs                 # Greeter protocol — test fixture
    tests/
      loom_method_cache.rs              # loom model-check: MethodCache epoch ordering
      loom_keyword_intern.rs            # loom: concurrent intern preserves identity
      loom_var_root.rs                  # loom: alter-var-root CAS
      loom_binding_stack.rs             # loom: bound-fn snapshot safety
  clojure_core_macros/                  # proc-macro crate (#[protocol], #[implements])
python/clojure/                         # Python package (re-exports from _core)
tests/                                  # pytest suite
```

## Collections

Main-line Clojure persistent collections, all protocol-routed:

- `PersistentList` + `EmptyList` singleton
- `PersistentVector` (32-way HAMT + tail optimization) + `TransientVector`
- `PersistentArrayMap` (flat-array small-map, auto-promotes to hash-map past 8 entries) + `TransientArrayMap`
- `PersistentHashMap` (32-way HAMT) + `TransientHashMap`
- `PersistentHashSet` (wraps HashMap) + `TransientHashSet`
- `Cons`, `LazySeq`, `VectorSeq` seq types
- `MapEntry` for map iteration

Python examples:

```python
from clojure._core import (
    vector, hash_map, hash_set, keyword,
    transient, persistent_bang, assoc_bang,
    cons, lazy_seq, first, rest, seq,
)

# HAMT vector, O(log32 n) ops:
v = vector(*range(1000)).conj(1001).assoc_n(500, "mid")

# Transient batch build:
t = transient(hash_map())
for i in range(5000):
    assoc_bang(t, keyword(f"k{i}"), i)
m = persistent_bang(t)
assert m(keyword("k42")) == 42

# Lazy infinite seq:
def ints(i): return cons(i, lazy_seq(lambda: ints(i + 1)))
xs = ints(0)
# Take first 10 without realizing the tail:
first_ten = [first(xs := (xs if i == 0 else rest(xs))) for i in range(10)]
```

### Testing

- 333 pytest tests including property-based fuzzing via `hypothesis`
- 3 Rust `proptest` properties on HAMT internals
- 4 Loom model-checks for core-abstractions concurrency primitives (method-cache, intern, CAS, bound-fn)

Run the fuzz tests:

```bash
pytest tests/test_collections_fuzz.py -v
cargo test --test proptest_hamt --release
```

## Design notes

The extension targets **free-threaded CPython 3.14t specifically**. It is not ABI3-compatible; there is no abi3 level that includes the free-threaded build. The project exists to exercise no-GIL Python — every shared-mutable primitive (method cache, keyword intern table, var root, binding stack, namespace mappings) is lock-free, atomic, or explicitly locked with correctness verified under loom.

## Reader

Clojure reader — recursive-descent port of `LispReader.java`:

```python
from clojure._core import read_string, pr_str

data = read_string("(def x {:items [1 2 3]})")
# A PersistentList of (Symbol, Symbol, PersistentHashMap)

print(pr_str(data))
# (def x {:items [1 2 3]})
```

**Supported:**
- Atoms: nil, true, false, int (bignum), float, string, character, symbol, keyword
- Collections: `()`, `[]`, `{}`, `#{}`
- Reader macros: `'quote`, `@deref`, `#'var`, `^meta`, `;comment`, `#_discard`
- Round-trip via `pr_str` → `read_string` (property-tested with 200 hypothesis cases)

**Deferred**: reader conditionals, tagged literals, namespaced maps,
shebang, EDN-only mode. (Syntax-quote, fn-literal, regex, numeric
suffixes, and auto-resolved keywords have since landed.)

## Evaluator

Tree-walking evaluator that runs Clojure forms from the reader:

```python
from clojure._core import eval_string

# Define a recursive function:
eval_string("(defn fact [n] (if (= n 0) 1 (* n (fact (- n 1)))))")
eval_string("(fact 10)")
# => 3628800

# Closures:
eval_string("(defn make-adder [n] (fn [x] (+ x n)))")
eval_string("(def add5 (make-adder 5))")
eval_string("(add5 37)")
# => 42

# Data manipulation:
eval_string('(:name {:name "alice" :age 30})')
# => "alice"
```

**Special forms:** `quote`, `if`, `do`, `let` / `let*`, `fn` / `fn*`, `def`, `var`.

**Hardcoded macros:** `defn`, `when`, `when-not`, `cond`, `or`, `and`.

**Pre-populated `clojure.core`:** arithmetic (`+`, `-`, `*`, `/`, `inc`, `dec`), comparison (`=`, `<`, `>`, `<=`, `>=`), logical (`not`, `nil?`), collections (`list`, `vector`, `hash-map`, `hash-set`), seq ops (`count`, `first`, `rest`, `next`, `seq`, `cons`), and `str`.

**Deferred** (tracked in the evaluator plan): `loop`/`recur`, `try`/`catch`/`throw`, Python interop (`.method`, `new`), multi-arity fn, destructuring, user-defined `defmacro`, `letfn`, tail-call optimization, multimethods, records/deftype.
