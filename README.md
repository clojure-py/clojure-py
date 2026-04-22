# clojure-py

Clojure core on Python 3.14t, implemented in Rust via PyO3.

## Status

Core abstractions (protocols, IFn, Var, Namespace, Symbol, Keyword) —
see `docs/superpowers/specs/2026-04-21-core-abstractions-design.md` for
the design spec and `docs/superpowers/plans/2026-04-21-core-abstractions-plan.md`
for the implementation plan.

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
docs/superpowers/{specs,plans}/         # design + implementation documents
```

## Design notes

The extension targets **free-threaded CPython 3.14t specifically**. It is not ABI3-compatible; there is no abi3 level that includes the free-threaded build. The project exists to exercise no-GIL Python — every shared-mutable primitive (method cache, keyword intern table, var root, binding stack, namespace mappings) is lock-free, atomic, or explicitly locked with correctness verified under loom.
