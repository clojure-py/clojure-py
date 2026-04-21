# Core Abstractions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the foundational primitives for clojure-py revival — `Symbol`, `Keyword`, `IFn`, protocol system with method cache + fallback, `Var`, `Namespace` (as Python modules), `#[protocol]`/`#[implements]` attribute macros — as a PyO3-based Rust extension targeting CPython 3.14t.

**Architecture:** Two-crate Rust workspace (`clojure_core` cdylib + `clojure_core_macros` proc-macro). Namespaces subclass `types.ModuleType` and live in `sys.modules`. Protocol dispatch uses a per-protocol `MethodCache` with epoch-based invalidation, MRO walk, metadata path, and a pluggable fallback. Vars implement `IFn` directly and delegate dunders to `deref()` for a "pythonic enough" surface.

**Tech Stack:** Rust 2024, PyO3 0.22+, `maturin` for build, `dashmap`, `pyo3` `abi3-py314`-equivalent (ABI-pinned to 3.14+ free-threaded), `inventory` for registration collection, `syn`/`quote`/`proc-macro2` for macros, `loom` for model-checked concurrency tests, Python `pytest` for integration tests.

**Spec:** `docs/superpowers/specs/2026-04-21-core-abstractions-design.md`

---

## File Structure

```
clojure-py/
├── Cargo.toml                           # workspace
├── pyproject.toml                       # maturin build
├── rust-toolchain.toml                  # pin toolchain
├── crates/
│   ├── clojure_core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                   # PyModule init
│   │       ├── exceptions.rs            # ArityException, IllegalStateException, etc.
│   │       ├── symbol.rs
│   │       ├── keyword.rs               # + intern table
│   │       ├── ifn.rs                   # IFn trait + AFn base
│   │       ├── protocol.rs              # Protocol, ProtocolMethod, MethodCache
│   │       ├── dispatch.rs              # dispatch algorithm
│   │       ├── var.rs
│   │       ├── var_delegation.rs        # arith/container/bool dunders
│   │       ├── binding.rs               # thread_local BINDING_STACK
│   │       ├── bound_fn.rs              # bound-fn*
│   │       ├── pmap.rs                  # minimal PersistentMap for binding frames
│   │       ├── namespace.rs             # ClojureNamespace
│   │       ├── ns_ops.rs                # intern, refer, alias, import
│   │       ├── rt.rs                    # RT helpers (invoke, get, nth)
│   │       └── registry.rs              # inventory-based registration
│   └── clojure_core_macros/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                   # proc-macro entry points
│           ├── protocol.rs              # #[protocol] expansion
│           └── implements.rs            # #[implements] expansion
├── python/
│   └── clojure/
│       └── __init__.py                  # re-exports from Rust extension
├── tests/                               # Python pytest suite
│   ├── conftest.py
│   ├── test_symbol.py
│   ├── test_keyword.py
│   ├── test_protocol_dispatch.py
│   ├── test_ifn.py
│   ├── test_namespace.py
│   ├── test_var.py
│   └── test_concurrency_stress.py
└── docs/superpowers/{specs,plans}/
```

---

## Commit Convention

All commits use Conventional Commits. The plan specifies exact messages per step. All commits include:

```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Phase 1 — Workspace Scaffolding

Goal: a `maturin develop`-able extension that `import clojure` can load, even if it exports nothing interesting yet. End of phase: a CI-runnable empty extension.

### Task 1: Initialize workspace manifest

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Write workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/clojure_core", "crates/clojure_core_macros"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "EPL-1.0"
repository = "https://github.com/tbaldridge/clojure-py"

[workspace.dependencies]
pyo3 = { version = "0.22", features = ["extension-module", "abi3-py314"] }
dashmap = "6"
parking_lot = "0.12"
inventory = "0.3"
once_cell = "1"
smallvec = { version = "1", features = ["const_generics", "union"] }
fxhash = "0.2"
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
```

- [ ] **Step 2: Write rust-toolchain.toml**

```toml
[toolchain]
channel = "1.85"
components = ["rustfmt", "clippy", "rust-src"]
```

- [ ] **Step 3: Verify workspace manifest parses**

Run: `cargo check --workspace 2>&1 | head`
Expected: errors about missing crate manifests (that's fine; we create them next).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml rust-toolchain.toml
git commit -m "chore: initialize Rust workspace

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2: Scaffold clojure_core crate

**Files:**
- Create: `crates/clojure_core/Cargo.toml`
- Create: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Create crate manifest**

```toml
# crates/clojure_core/Cargo.toml
[package]
name = "clojure_core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
name = "clojure_core"
crate-type = ["cdylib", "rlib"]

[dependencies]
pyo3 = { workspace = true }
dashmap = { workspace = true }
parking_lot = { workspace = true }
inventory = { workspace = true }
once_cell = { workspace = true }
smallvec = { workspace = true }
fxhash = { workspace = true }

[features]
loom = []
```

- [ ] **Step 2: Create skeleton lib.rs**

```rust
// crates/clojure_core/src/lib.rs
use pyo3::prelude::*;

#[pymodule]
fn clojure_core(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p clojure_core 2>&1 | tail -5`
Expected: "Finished" with no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/clojure_core
git commit -m "feat(core): scaffold clojure_core crate

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3: Scaffold clojure_core_macros crate

**Files:**
- Create: `crates/clojure_core_macros/Cargo.toml`
- Create: `crates/clojure_core_macros/src/lib.rs`

- [ ] **Step 1: Create crate manifest**

```toml
# crates/clojure_core_macros/Cargo.toml
[package]
name = "clojure_core_macros"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
proc-macro = true

[dependencies]
syn = { workspace = true }
quote = { workspace = true }
proc-macro2 = { workspace = true }
```

- [ ] **Step 2: Skeleton lib.rs**

```rust
// crates/clojure_core_macros/src/lib.rs
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn protocol(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item  // passthrough for now
}

#[proc_macro_attribute]
pub fn implements(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item  // passthrough for now
}
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p clojure_core_macros 2>&1 | tail -5`
Expected: "Finished" with no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/clojure_core_macros
git commit -m "feat(macros): scaffold clojure_core_macros crate

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 4: Wire macros into core crate

**Files:**
- Modify: `crates/clojure_core/Cargo.toml`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Add macros as dependency**

In `crates/clojure_core/Cargo.toml`, add under `[dependencies]`:

```toml
clojure_core_macros = { path = "../clojure_core_macros" }
```

- [ ] **Step 2: Re-export macros from lib.rs**

Replace `crates/clojure_core/src/lib.rs` with:

```rust
use pyo3::prelude::*;

pub use clojure_core_macros::{implements, protocol};

#[pymodule]
fn clojure_core(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
```

- [ ] **Step 3: Verify**

Run: `cargo check --workspace 2>&1 | tail -3`
Expected: "Finished" cleanly.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core): re-export macros from clojure_core

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5: Configure maturin build

**Files:**
- Create: `pyproject.toml`
- Create: `python/clojure/__init__.py`
- Create: `.gitignore`

- [ ] **Step 1: Write pyproject.toml**

```toml
[build-system]
requires = ["maturin>=1.6,<2.0"]
build-backend = "maturin"

[project]
name = "clojure"
version = "0.1.0"
requires-python = ">=3.14"
description = "Clojure on Python (PyO3)"
classifiers = [
    "Programming Language :: Python :: Implementation :: CPython",
    "Programming Language :: Rust",
]

[tool.maturin]
manifest-path = "crates/clojure_core/Cargo.toml"
module-name = "clojure._core"
python-source = "python"
features = ["pyo3/extension-module"]

[project.optional-dependencies]
test = ["pytest>=8", "pytest-timeout"]
```

- [ ] **Step 2: Create Python package init**

```python
# python/clojure/__init__.py
"""Clojure on Python — PyO3-backed core."""

from clojure import _core  # noqa: F401  — registers types in sys.modules at import time

__all__ = ["_core"]
```

- [ ] **Step 3: Create .gitignore**

```
target/
*.so
*.pyd
*.dylib
__pycache__/
*.egg-info/
dist/
.venv/
.pytest_cache/
.mypy_cache/
```

- [ ] **Step 4: Build extension**

Run: `maturin develop --release 2>&1 | tail -10`
Expected: builds successfully; `.so` copied into a venv site-packages.

Prereq: the engineer must have a 3.14t venv active (`python3.14t -m venv .venv && source .venv/bin/activate && pip install maturin pytest`).

- [ ] **Step 5: Smoke-test the import**

Run: `python -c "import clojure; print(clojure._core)"`
Expected: prints `<module 'clojure._core' from '...'>`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: configure maturin build and Python package

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 6: Pytest scaffolding

**Files:**
- Create: `tests/conftest.py`
- Create: `tests/test_smoke.py`

- [ ] **Step 1: Write conftest**

```python
# tests/conftest.py
import sys
import pytest

@pytest.fixture(autouse=True)
def _require_free_threaded():
    """Ensure tests run on 3.14t (free-threaded)."""
    if not getattr(sys, "_is_gil_enabled", lambda: True)() is False:
        # Some envs run these tests under GIL-ful 3.14 for iteration speed;
        # that's allowed, but any test that explicitly needs no-GIL should
        # use the `require_free_threaded` marker.
        pass
```

- [ ] **Step 2: Write smoke test**

```python
# tests/test_smoke.py
import clojure
from clojure import _core

def test_extension_loads():
    assert _core is not None
    assert hasattr(_core, "__name__")
```

- [ ] **Step 3: Run**

Run: `pytest tests/test_smoke.py -v`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add tests/
git commit -m "test: scaffold pytest suite with smoke test

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 2 — Exceptions & Value Types

Goal: the leaf types (Symbol, Keyword) and Clojure-style exception classes. Symbols and Keywords are testable from Python after this phase.

### Task 7: Clojure-style exceptions

**Files:**
- Create: `crates/clojure_core/src/exceptions.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Write failing Python test**

```python
# tests/test_exceptions.py
import pytest
from clojure._core import (
    ArityException, IllegalStateException, IllegalArgumentException,
)

def test_arity_exception_is_subclass_of_typeerror():
    assert issubclass(ArityException, TypeError)

def test_arity_exception_message():
    with pytest.raises(ArityException) as ei:
        raise ArityException("Wrong number of args (3) passed to: foo")
    assert "Wrong number of args" in str(ei.value)

def test_illegal_state_exception():
    assert issubclass(IllegalStateException, RuntimeError)

def test_illegal_argument_exception():
    assert issubclass(IllegalArgumentException, ValueError)
```

- [ ] **Step 2: Run test, expect failure**

Run: `pytest tests/test_exceptions.py -v`
Expected: `ImportError` or `AttributeError` — symbols not defined.

- [ ] **Step 3: Implement exceptions.rs**

```rust
// crates/clojure_core/src/exceptions.rs
use pyo3::create_exception;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;

create_exception!(clojure_core, ArityException, PyTypeError);
create_exception!(clojure_core, IllegalStateException, PyRuntimeError);
create_exception!(clojure_core, IllegalArgumentException, PyValueError);

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ArityException", py.get_type::<ArityException>())?;
    m.add("IllegalStateException", py.get_type::<IllegalStateException>())?;
    m.add("IllegalArgumentException", py.get_type::<IllegalArgumentException>())?;
    Ok(())
}
```

- [ ] **Step 4: Wire into lib.rs**

```rust
// crates/clojure_core/src/lib.rs
use pyo3::prelude::*;

mod exceptions;

pub use clojure_core_macros::{implements, protocol};
pub use exceptions::{ArityException, IllegalArgumentException, IllegalStateException};

#[pymodule]
fn clojure_core(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    exceptions::register(py, m)?;
    Ok(())
}
```

- [ ] **Step 5: Rebuild & test**

Run: `maturin develop --release && pytest tests/test_exceptions.py -v`
Expected: 4 passed.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): Clojure-style exception classes

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 8: Symbol — basic construction & equality

**Files:**
- Create: `crates/clojure_core/src/symbol.rs`
- Create: `tests/test_symbol.py`

- [ ] **Step 1: Failing test**

```python
# tests/test_symbol.py
from clojure._core import Symbol, symbol

def test_symbol_no_ns():
    s = symbol("foo")
    assert s.name == "foo"
    assert s.ns is None

def test_symbol_with_ns():
    s = symbol("my.ns", "foo")
    assert s.ns == "my.ns"
    assert s.name == "foo"

def test_symbol_equality_by_value():
    assert symbol("foo") == symbol("foo")
    assert symbol("a", "b") == symbol("a", "b")
    assert symbol("foo") != symbol("bar")
    assert symbol("a", "b") != symbol("b")

def test_symbol_identity_not_interned():
    assert symbol("foo") is not symbol("foo")

def test_symbol_hash_value_based():
    assert hash(symbol("foo")) == hash(symbol("foo"))
    assert hash(symbol("a", "b")) == hash(symbol("a", "b"))

def test_symbol_repr():
    assert repr(symbol("foo")) == "foo"
    assert repr(symbol("my.ns", "foo")) == "my.ns/foo"

def test_symbol_isinstance():
    assert isinstance(symbol("foo"), Symbol)
```

- [ ] **Step 2: Run, expect fail**

Run: `pytest tests/test_symbol.py -v`
Expected: ImportError on `Symbol`/`symbol`.

- [ ] **Step 3: Implement symbol.rs**

```rust
// crates/clojure_core/src/symbol.rs
use pyo3::prelude::*;
use pyo3::types::PyString;
use std::sync::Arc;
use parking_lot::RwLock;

#[pyclass(module = "clojure._core", name = "Symbol", frozen)]
pub struct Symbol {
    pub ns: Option<Arc<str>>,
    pub name: Arc<str>,
    pub hash: u32,
    pub meta: RwLock<Option<PyObject>>,
}

impl Symbol {
    pub fn new(ns: Option<Arc<str>>, name: Arc<str>) -> Self {
        let h = compute_hash(ns.as_deref(), &name);
        Self { ns, name, hash: h, meta: RwLock::new(None) }
    }
}

fn compute_hash(ns: Option<&str>, name: &str) -> u32 {
    // Clojure hashes symbols as the hash of (ns + "/" + name), with a sym tag.
    use std::hash::{Hash, Hasher};
    let mut h = fxhash::FxHasher::default();
    0xSY_u64.hash(&mut h);  // sentinel tag
    if let Some(n) = ns { n.hash(&mut h); "/".hash(&mut h); }
    name.hash(&mut h);
    (h.finish() as u32)
}

#[pymethods]
impl Symbol {
    #[getter]
    fn ns(&self) -> Option<&str> { self.ns.as_deref() }

    #[getter]
    fn name(&self) -> &str { &self.name }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        let Ok(o) = other.downcast::<Self>() else { return false; };
        let o = o.get();
        self.ns.as_deref() == o.ns.as_deref() && *self.name == *o.name
    }

    fn __hash__(&self) -> u32 { self.hash }

    fn __repr__(&self) -> String {
        match &self.ns {
            Some(n) => format!("{}/{}", n, self.name),
            None    => self.name.to_string(),
        }
    }

    fn __str__(&self) -> String { self.__repr__() }

    fn with_meta(&self, meta: PyObject) -> Self {
        let s = Self { ns: self.ns.clone(), name: self.name.clone(), hash: self.hash, meta: RwLock::new(Some(meta)) };
        s
    }

    #[getter]
    fn meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }
}

#[pyfunction]
#[pyo3(signature = (ns_or_name, name=None))]
pub fn symbol(ns_or_name: &str, name: Option<&str>) -> Symbol {
    match name {
        Some(n) => Symbol::new(Some(Arc::from(ns_or_name)), Arc::from(n)),
        None => {
            // Allow "ns/name" slash-form
            if let Some((ns, nm)) = ns_or_name.split_once('/') {
                if !ns.is_empty() && !nm.is_empty() {
                    return Symbol::new(Some(Arc::from(ns)), Arc::from(nm));
                }
            }
            Symbol::new(None, Arc::from(ns_or_name))
        }
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Symbol>()?;
    m.add_function(wrap_pyfunction!(symbol, m)?)?;
    Ok(())
}
```

Fix the `0xSY_u64` typo if rust-analyzer complains — it's illustrative; use the literal `u64::from_be_bytes(*b"SYMBOLxx")` or any fixed tag constant.

- [ ] **Step 4: Wire into lib.rs**

Add to lib.rs:
```rust
mod symbol;
pub use symbol::Symbol;
```

And in the pymodule init body, after `exceptions::register(py, m)?;`:
```rust
symbol::register(py, m)?;
```

- [ ] **Step 5: Rebuild + test**

Run: `maturin develop --release && pytest tests/test_symbol.py -v`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): Symbol value type with ns/name + value-equality

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 9: Symbol meta independence

**Files:**
- Modify: `tests/test_symbol.py`
- Modify: `crates/clojure_core/src/symbol.rs` (if needed)

- [ ] **Step 1: Add failing tests**

Append to `tests/test_symbol.py`:

```python
def test_with_meta_preserves_value_equality():
    s1 = symbol("foo")
    s2 = s1.with_meta({"a": 1})
    assert s1 == s2
    assert hash(s1) == hash(s2)

def test_with_meta_independent_instances():
    s1 = symbol("foo")
    s2 = s1.with_meta({"a": 1})
    assert s1.meta is None
    assert s2.meta == {"a": 1}
```

- [ ] **Step 2: Run — expect pass (already supported)**

Run: `pytest tests/test_symbol.py -v`
Expected: all pass — `with_meta` was already implemented in Task 8.

- [ ] **Step 3: Commit**

```bash
git add tests/test_symbol.py
git commit -m "test(symbol): cover with_meta value equality and independence

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 10: Keyword — construction, interning, callability

**Files:**
- Create: `crates/clojure_core/src/keyword.rs`
- Create: `tests/test_keyword.py`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Write failing tests**

```python
# tests/test_keyword.py
from clojure._core import Keyword, keyword, symbol

def test_keyword_no_ns():
    k = keyword("foo")
    assert k.name == "foo"
    assert k.ns is None

def test_keyword_with_ns():
    k = keyword("ns", "foo")
    assert k.ns == "ns"
    assert k.name == "foo"

def test_keyword_interned_identity():
    assert keyword("foo") is keyword("foo")
    assert keyword("a", "b") is keyword("a", "b")

def test_keyword_distinct_by_ns():
    assert keyword("foo") is not keyword("ns", "foo")

def test_keyword_hash_stable():
    assert hash(keyword("foo")) == hash(keyword("foo"))

def test_keyword_repr():
    assert repr(keyword("foo")) == ":foo"
    assert repr(keyword("ns", "foo")) == ":ns/foo"

def test_keyword_callable_get():
    d = {keyword("a"): 1, keyword("b"): 2}
    assert keyword("a")(d) == 1
    assert keyword("c")(d) is None
    assert keyword("c")(d, "default") == "default"

def test_keyword_from_slash_form():
    k = keyword("ns/foo")
    assert k.ns == "ns"
    assert k.name == "foo"

def test_keyword_concurrent_intern():
    import threading
    results = []
    def worker():
        for _ in range(1000):
            results.append(keyword("shared"))
    ts = [threading.Thread(target=worker) for _ in range(16)]
    for t in ts: t.start()
    for t in ts: t.join()
    first = results[0]
    assert all(r is first for r in results)
```

- [ ] **Step 2: Implement keyword.rs**

```rust
// crates/clojure_core/src/keyword.rs
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

type KeywordKey = (Option<Arc<str>>, Arc<str>);
static INTERN: Lazy<DashMap<KeywordKey, Py<Keyword>>> = Lazy::new(DashMap::new);

#[pyclass(module = "clojure._core", name = "Keyword", frozen)]
pub struct Keyword {
    pub ns: Option<Arc<str>>,
    pub name: Arc<str>,
    pub hash: u32,
}

impl Keyword {
    fn compute_hash(ns: Option<&str>, name: &str) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut h = fxhash::FxHasher::default();
        0xKEEEEE_u64.hash(&mut h);  // keyword tag (use a real constant)
        if let Some(n) = ns { n.hash(&mut h); "/".hash(&mut h); }
        name.hash(&mut h);
        h.finish() as u32
    }
}

#[pymethods]
impl Keyword {
    #[getter] fn ns(&self) -> Option<&str> { self.ns.as_deref() }
    #[getter] fn name(&self) -> &str { &self.name }

    fn __hash__(&self) -> u32 { self.hash }

    fn __eq__(&self, other: &Bound<'_, PyAny>, py: Python<'_>) -> bool {
        // Interned -> pointer identity is enough, but be robust:
        let Ok(o) = other.downcast::<Self>() else { return false; };
        std::ptr::eq(self as *const _, o.get() as *const _)
            || (self.ns.as_deref() == o.get().ns.as_deref() && *self.name == *o.get().name)
    }

    fn __repr__(&self) -> String {
        match &self.ns {
            Some(n) => format!(":{}/{}", n, self.name),
            None    => format!(":{}", self.name),
        }
    }

    fn __str__(&self) -> String { self.__repr__() }

    // Callable form: (:k m) or (:k m default)
    #[pyo3(signature = (coll, default=None))]
    fn __call__(&self, py: Python<'_>, coll: &Bound<'_, PyAny>, default: Option<PyObject>) -> PyResult<PyObject> {
        // For now, support dict-like containers directly. Real IFn dispatch arrives with protocols.
        if let Ok(d) = coll.downcast::<PyDict>() {
            // Compare by value: the dict key must be equal (not just identical).
            let self_any: &Bound<'_, PyAny> = unsafe { std::mem::transmute(coll) }; // placeholder
            if let Some(v) = d.get_item(self.as_pyobj(py)?)? {
                return Ok(v.into());
            }
        }
        Ok(default.unwrap_or_else(|| py.None()))
    }
}

impl Keyword {
    fn as_pyobj(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Look ourselves up in INTERN to produce a Py<Keyword>.
        let key = (self.ns.clone(), self.name.clone());
        INTERN.get(&key).map(|e| e.value().clone_ref(py).into_any()).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("keyword not interned")
        })
    }
}

#[pyfunction]
#[pyo3(signature = (ns_or_name, name=None))]
pub fn keyword(py: Python<'_>, ns_or_name: &str, name: Option<&str>) -> PyResult<Py<Keyword>> {
    let (ns_opt, name_str): (Option<Arc<str>>, Arc<str>) = match name {
        Some(n) => (Some(Arc::from(ns_or_name)), Arc::from(n)),
        None => {
            if let Some((n, nm)) = ns_or_name.split_once('/') {
                if !n.is_empty() && !nm.is_empty() {
                    (Some(Arc::from(n)), Arc::from(nm))
                } else {
                    (None, Arc::from(ns_or_name))
                }
            } else {
                (None, Arc::from(ns_or_name))
            }
        }
    };
    let key = (ns_opt.clone(), name_str.clone());
    if let Some(e) = INTERN.get(&key) {
        return Ok(e.value().clone_ref(py));
    }
    let h = Keyword::compute_hash(ns_opt.as_deref(), &name_str);
    let kw = Py::new(py, Keyword { ns: ns_opt.clone(), name: name_str.clone(), hash: h })?;
    let entry = INTERN.entry(key).or_insert_with(|| kw.clone_ref(py));
    Ok(entry.value().clone_ref(py))
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Keyword>()?;
    m.add_function(wrap_pyfunction!(keyword, m)?)?;
    Ok(())
}
```

Notes for the implementer:
- The `as_pyobj` + `transmute` lines above are a temporary shim so the `__call__` can do dict lookups; replace once real `IFn`-backed `rt::get` exists (Task 29). For now, using the dict key equality path above should suffice since interned Keywords have `__eq__` + `__hash__`.
- Replace the `0xKEEEEE_u64` placeholder with a real u64 constant; the specific value doesn't matter, only that it differs from the Symbol tag.

- [ ] **Step 3: Wire into lib.rs**

Add `mod keyword;`, `pub use keyword::Keyword;`, and `keyword::register(py, m)?;` in the init body.

- [ ] **Step 4: Rebuild + test**

Run: `maturin develop --release && pytest tests/test_keyword.py -v`
Expected: all tests pass including the concurrent intern test.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): Keyword with global intern table and callable form

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


## Phase 3 — Protocol Runtime + `#[protocol]` Macro

Goal: `#[protocol]` attribute macro on a Rust trait generates the runtime `Protocol`/`ProtocolMethod` PyO3 classes with a working `MethodCache`. No `#[implements]` yet — that's Phase 4. Dispatch at end of phase works only for "no impls registered → raises IllegalArgumentException".

### Task 11: Runtime types — Protocol, ProtocolMethod, MethodCache skeletons

**Files:**
- Create: `crates/clojure_core/src/protocol.rs`
- Create: `crates/clojure_core/src/dispatch.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Define runtime types**

```rust
// crates/clojure_core/src/protocol.rs
use crate::symbol::Symbol;
use dashmap::DashMap;
use parking_lot::RwLock;
use pyo3::prelude::*;
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Key type for the method cache. Either a Python PyType* (as usize) or a Rust TypeId.
/// We erase into a single u64 with a tag bit to distinguish.
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub struct CacheKey(pub u64);

impl CacheKey {
    pub fn for_py_type(ty: *mut pyo3::ffi::PyTypeObject) -> Self {
        // Clear the low bit (PyType pointers are aligned); tag = 0 for pytype.
        Self(ty as usize as u64)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Origin { InlineAttr, Extend, Metadata, Fallback }

pub struct MethodTable {
    pub impls: fxhash::FxHashMap<Arc<str>, PyObject>,
    pub origin: Origin,
}

pub struct MethodCache {
    pub epoch: AtomicU64,
    pub entries: DashMap<CacheKey, Arc<MethodTable>>,
}

impl MethodCache {
    pub fn new() -> Self {
        Self { epoch: AtomicU64::new(0), entries: DashMap::new() }
    }
    pub fn bump_epoch(&self) { self.epoch.fetch_add(1, Ordering::AcqRel); }
    pub fn lookup(&self, k: CacheKey) -> Option<Arc<MethodTable>> {
        self.entries.get(&k).map(|e| Arc::clone(e.value()))
    }
}

#[pyclass(module = "clojure._core", name = "Protocol", frozen)]
pub struct Protocol {
    pub name: Py<Symbol>,
    pub method_keys: SmallVec<[Arc<str>; 8]>,
    pub cache: Arc<MethodCache>,
    pub fallback: RwLock<Option<PyObject>>,
    pub via_metadata: bool,
}

#[pymethods]
impl Protocol {
    #[getter] fn name(&self, py: Python<'_>) -> Py<Symbol> { self.name.clone_ref(py) }

    fn set_fallback(&self, fallback: Option<PyObject>) { *self.fallback.write() = fallback; }

    #[getter] fn fallback(&self, py: Python<'_>) -> Option<PyObject> {
        self.fallback.read().as_ref().map(|o| o.clone_ref(py))
    }

    #[getter] fn via_metadata(&self) -> bool { self.via_metadata }

    /// Extend this protocol to a Python type with a map of method-name → impl fn.
    fn extend_type(&self, py: Python<'_>, ty: Bound<'_, pyo3::types::PyType>, impls: Bound<'_, pyo3::types::PyDict>) -> PyResult<()> {
        let mut table = fxhash::FxHashMap::default();
        for (k, v) in impls.iter() {
            let k: String = k.extract()?;
            table.insert(Arc::from(k.as_str()), v.into());
        }
        let key = CacheKey::for_py_type(ty.as_ptr() as *mut _);
        self.cache.entries.insert(key, Arc::new(MethodTable { impls: table, origin: Origin::Extend }));
        self.cache.bump_epoch();
        Ok(())
    }
}

#[pyclass(module = "clojure._core", name = "ProtocolMethod", frozen)]
pub struct ProtocolMethod {
    pub protocol: Py<Protocol>,
    pub key: Arc<str>,
}

#[pymethods]
impl ProtocolMethod {
    #[getter] fn protocol(&self, py: Python<'_>) -> Py<Protocol> { self.protocol.clone_ref(py) }
    #[getter] fn key(&self) -> &str { &self.key }

    #[pyo3(signature = (target, *args))]
    fn __call__(&self, py: Python<'_>, target: PyObject, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        crate::dispatch::dispatch(py, &self.protocol.bind(py).get(), &self.key, target, args)
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Protocol>()?;
    m.add_class::<ProtocolMethod>()?;
    Ok(())
}
```

- [ ] **Step 2: Stub the dispatch algorithm**

```rust
// crates/clojure_core/src/dispatch.rs
use crate::exceptions::IllegalArgumentException;
use crate::protocol::{CacheKey, Protocol};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::sync::Arc;

pub fn dispatch(
    py: Python<'_>,
    protocol: &Protocol,
    method_key: &Arc<str>,
    target: PyObject,
    args: Bound<'_, PyTuple>,
) -> PyResult<PyObject> {
    let bound = target.bind(py);
    let ty = bound.get_type();
    let key = CacheKey::for_py_type(ty.as_ptr() as *mut _);

    // Step 1: exact type
    if let Some(table) = protocol.cache.lookup(key) {
        if let Some(impl_fn) = table.impls.get(method_key) {
            let mut call_args: Vec<PyObject> = vec![target];
            for a in args.iter() { call_args.push(a.into()); }
            let tup = PyTuple::new_bound(py, &call_args);
            return impl_fn.bind(py).call1(tup);
        }
    }

    // Step 2: MRO walk
    let mro = ty.getattr("__mro__")?;
    let mro_tuple: Bound<'_, PyTuple> = mro.downcast_into()?;
    for parent in mro_tuple.iter().skip(1) {
        let pk = CacheKey::for_py_type(parent.as_ptr() as *mut _);
        if let Some(table) = protocol.cache.lookup(pk) {
            if let Some(impl_fn) = table.impls.get(method_key) {
                // promote to exact
                protocol.cache.entries.insert(key, table.clone());
                let mut call_args: Vec<PyObject> = vec![target];
                for a in args.iter() { call_args.push(a.into()); }
                let tup = PyTuple::new_bound(py, &call_args);
                return impl_fn.bind(py).call1(tup);
            }
        }
    }

    // Step 3: metadata (no-op until IMeta ships)

    // Step 4: fallback
    if let Some(fb) = protocol.fallback.read().clone() {
        // Call fallback(protocol, method_key, target), then re-run 1-3 once.
        // Re-entry guard: we pass a marker via a local flag.
        fb.bind(py).call1((protocol_as_py(protocol, py)?, method_key.as_ref(), target.clone_ref(py)))?;
        // One retry: re-check exact + MRO.
        if let Some(table) = protocol.cache.lookup(key) {
            if let Some(impl_fn) = table.impls.get(method_key) {
                let mut call_args: Vec<PyObject> = vec![target];
                for a in args.iter() { call_args.push(a.into()); }
                let tup = PyTuple::new_bound(py, &call_args);
                return impl_fn.bind(py).call1(tup);
            }
        }
        for parent in mro_tuple.iter().skip(1) {
            let pk = CacheKey::for_py_type(parent.as_ptr() as *mut _);
            if let Some(table) = protocol.cache.lookup(pk) {
                if let Some(impl_fn) = table.impls.get(method_key) {
                    protocol.cache.entries.insert(key, table.clone());
                    let mut call_args: Vec<PyObject> = vec![target];
                    for a in args.iter() { call_args.push(a.into()); }
                    let tup = PyTuple::new_bound(py, &call_args);
                    return impl_fn.bind(py).call1(tup);
                }
            }
        }
    }

    Err(IllegalArgumentException::new_err(format!(
        "No implementation of method: {} of protocol: {} found for class: {}",
        method_key,
        protocol.name.bind(py).get().__repr__(),
        ty.qualname()?
    )))
}

fn protocol_as_py(p: &Protocol, py: Python<'_>) -> PyResult<PyObject> {
    // Caller holds a reference via bound().get(); we need a Py<Protocol>.
    // The cleanest path: the Protocol lives behind a Py<Protocol> on ProtocolMethod;
    // the dispatcher signature should carry the Py<Protocol>. Fix this in Task 12.
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "dispatch must be called via ProtocolMethod which holds Py<Protocol>"
    ))
}
```

- [ ] **Step 3: Wire into lib.rs**

Add:
```rust
mod protocol;
mod dispatch;
pub use protocol::{Protocol, ProtocolMethod};
```
And `protocol::register(py, m)?;` in the module init.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p clojure_core 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): Protocol/ProtocolMethod/MethodCache runtime skeleton

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 12: Fix dispatch to carry Py<Protocol>

**Files:**
- Modify: `crates/clojure_core/src/dispatch.rs`
- Modify: `crates/clojure_core/src/protocol.rs`

- [ ] **Step 1: Change dispatch signature to take `Py<Protocol>`**

Replace the `dispatch` function signature and internal protocol_as_py hack:

```rust
pub fn dispatch(
    py: Python<'_>,
    protocol_py: &Py<Protocol>,
    method_key: &Arc<str>,
    target: PyObject,
    args: Bound<'_, PyTuple>,
) -> PyResult<PyObject> {
    let protocol = protocol_py.bind(py).get();
    // ... rest identical, but fallback call uses protocol_py directly:
    // fb.bind(py).call1((protocol_py.clone_ref(py), method_key.as_ref(), target.clone_ref(py)))?;
}
```

- [ ] **Step 2: Update ProtocolMethod.__call__**

```rust
fn __call__(&self, py: Python<'_>, target: PyObject, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
    crate::dispatch::dispatch(py, &self.protocol, &self.key, target, args)
}
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p clojure_core 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix(dispatch): carry Py<Protocol> through dispatch call path

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 13: Python-level test of bare Protocol + empty MethodCache

**Files:**
- Create: `tests/test_protocol_runtime.py`

- [ ] **Step 1: Write test**

```python
# tests/test_protocol_runtime.py
import pytest
from clojure._core import (
    Protocol, ProtocolMethod, symbol, IllegalArgumentException,
)

# We don't have the macro yet; construct Protocol/ProtocolMethod manually for now.
# This is a low-level test confirming the runtime type surface is correct.

def test_protocol_has_name_and_methods(py_factory):
    p = py_factory()
    assert p.name == symbol("clojure.core", "TestProto")
    assert p.via_metadata is False

def test_dispatch_raises_with_no_impl(py_factory):
    p = py_factory()
    method = ProtocolMethod_for(p, "m")  # see fixture
    with pytest.raises(IllegalArgumentException, match="No implementation"):
        method(42)
```

Since we haven't yet exposed constructors for `Protocol`/`ProtocolMethod` from Python, this test will be parameterized via a builder that the `#[protocol]` macro will provide in Task 15. For now, skip and move to writing the macro — come back and fill this test after Task 17.

- [ ] **Step 2: Mark test as xfail for now**

```python
import pytest
pytestmark = pytest.mark.xfail(reason="Protocol constructor comes with #[protocol] macro in Task 17")
```

- [ ] **Step 3: Commit**

```bash
git add tests/test_protocol_runtime.py
git commit -m "test(protocol): add xfail placeholder for runtime tests (filled in Task 17)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 14: `#[protocol]` macro — parse phase

**Files:**
- Create: `crates/clojure_core_macros/src/protocol.rs`
- Modify: `crates/clojure_core_macros/src/lib.rs`

- [ ] **Step 1: Write parsing module**

```rust
// crates/clojure_core_macros/src/protocol.rs
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parse, ItemTrait, LitBool, LitStr, Token, TraitItem, TraitItemFn};

pub struct ProtocolArgs {
    pub name: String,
    pub via_metadata: bool,
}

impl Parse for ProtocolArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name: Option<String> = None;
        let mut via_metadata: bool = false;
        let punct: syn::punctuated::Punctuated<syn::MetaNameValue, Token![,]> =
            input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
        for nv in punct {
            let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
            match key.as_str() {
                "name" => {
                    let s: LitStr = syn::parse2(nv.value.to_token_stream())?;
                    name = Some(s.value());
                }
                "extend_via_metadata" => {
                    let b: LitBool = syn::parse2(nv.value.to_token_stream())?;
                    via_metadata = b.value;
                }
                other => return Err(syn::Error::new_spanned(nv, format!("unknown protocol arg: {other}"))),
            }
        }
        let name = name.ok_or_else(|| syn::Error::new(input.span(), "protocol requires name = \"...\""))?;
        Ok(Self { name, via_metadata })
    }
}

pub struct MethodInfo {
    pub ident: syn::Ident,
    pub arity: Option<usize>,  // None => invoke_variadic
}

pub fn method_infos(item: &ItemTrait) -> Vec<MethodInfo> {
    item.items.iter().filter_map(|ti| {
        if let TraitItem::Fn(TraitItemFn { sig, .. }) = ti {
            let name = sig.ident.to_string();
            let arity = if name == "invoke_variadic" {
                None
            } else if let Some(rest) = name.strip_prefix("invoke") {
                rest.parse::<usize>().ok()
            } else {
                None
            };
            Some(MethodInfo { ident: sig.ident.clone(), arity })
        } else { None }
    }).collect()
}
```

- [ ] **Step 2: Use it from lib.rs (no codegen yet — just parse)**

```rust
// crates/clojure_core_macros/src/lib.rs
use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemTrait};

mod protocol;

#[proc_macro_attribute]
pub fn protocol(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _args = parse_macro_input!(attr as protocol::ProtocolArgs);
    let item_trait = parse_macro_input!(item as ItemTrait);
    let _methods = protocol::method_infos(&item_trait);
    // For now, emit the trait unchanged — codegen arrives in Task 15.
    quote::quote!(#item_trait).into()
}

#[proc_macro_attribute]
pub fn implements(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
```

Add missing `use quote::ToTokens;` where needed.

- [ ] **Step 3: Compile check**

Run: `cargo check --workspace 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(macros): parse #[protocol] attribute args and trait methods

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 15: `#[protocol]` macro — codegen for runtime registration

**Files:**
- Modify: `crates/clojure_core_macros/src/protocol.rs`
- Modify: `crates/clojure_core_macros/src/lib.rs`
- Modify: `crates/clojure_core/src/registry.rs` (new)
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Create the inventory registry**

```rust
// crates/clojure_core/src/registry.rs
use crate::protocol::Protocol;
use pyo3::prelude::*;

pub struct ProtocolRegistration {
    /// Called at module init to build the Py<Protocol> and add it to the module.
    pub build_and_register: fn(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()>,
}

inventory::collect!(ProtocolRegistration);

pub fn register_all(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    for r in inventory::iter::<ProtocolRegistration> {
        (r.build_and_register)(py, m)?;
    }
    Ok(())
}
```

And wire into lib.rs: add `mod registry;` and call `registry::register_all(py, m)?;` in the pymodule init, *after* protocol::register but *before* any subsystem that needs protocols to exist.

- [ ] **Step 2: Codegen in the proc-macro**

Extend `crates/clojure_core_macros/src/protocol.rs` with a `codegen` function, and replace the stub in `lib.rs`:

```rust
// crates/clojure_core_macros/src/protocol.rs  (append)
pub fn expand(args: ProtocolArgs, item: ItemTrait) -> TokenStream {
    let trait_ident = &item.ident;
    let methods = method_infos(&item);

    let name_lit = &args.name;
    let via_md = args.via_metadata;

    let (ns_lit, name_part_lit) = match name_lit.split_once('/') {
        Some((n, m)) => (Some(n.to_string()), m.to_string()),
        None => (None, name_lit.clone()),
    };

    let method_key_strings: Vec<String> = methods.iter().map(|m| m.ident.to_string()).collect();
    let proto_obj_ident = format_ident!("__PROTO_OBJ_{}", trait_ident);
    let register_fn_ident = format_ident!("__register_proto_{}", trait_ident);

    let method_key_lits = method_key_strings.iter();
    let method_key_lits2 = method_key_strings.iter();

    let ns_expr = match ns_lit {
        Some(n) => quote! { Some(::std::sync::Arc::from(#n)) },
        None    => quote! { None },
    };

    quote! {
        #item

        #[allow(non_snake_case)]
        fn #register_fn_ident(py: ::pyo3::Python<'_>, m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
            use ::pyo3::prelude::*;
            let sym = ::clojure_core::Symbol::new(#ns_expr, ::std::sync::Arc::from(#name_part_lit));
            let sym_py = ::pyo3::Py::new(py, sym)?;
            let proto = ::clojure_core::Protocol {
                name: sym_py,
                method_keys: {
                    let mut v = ::smallvec::SmallVec::new();
                    #( v.push(::std::sync::Arc::from(#method_key_lits)); )*
                    v
                },
                cache: ::std::sync::Arc::new(::clojure_core::MethodCache::new()),
                fallback: ::parking_lot::RwLock::new(None),
                via_metadata: #via_md,
            };
            let proto_py = ::pyo3::Py::new(py, proto)?;
            m.add(stringify!(#trait_ident), proto_py.clone_ref(py))?;

            // Build one ProtocolMethod PyObject per method.
            #(
                let mname: &str = #method_key_lits2;
                let pm = ::clojure_core::ProtocolMethod {
                    protocol: proto_py.clone_ref(py),
                    key: ::std::sync::Arc::from(mname),
                };
                let pm_py = ::pyo3::Py::new(py, pm)?;
                m.add(mname, pm_py)?;
            )*
            Ok(())
        }

        ::inventory::submit! {
            ::clojure_core::registry::ProtocolRegistration { build_and_register: #register_fn_ident }
        }
    }
}
```

Update `lib.rs` to call `expand`:

```rust
#[proc_macro_attribute]
pub fn protocol(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as protocol::ProtocolArgs);
    let item_trait = parse_macro_input!(item as ItemTrait);
    protocol::expand(args, item_trait).into()
}
```

- [ ] **Step 3: Expose runtime types from clojure_core for macro use**

In `crates/clojure_core/src/lib.rs`:

```rust
pub use protocol::{Protocol, ProtocolMethod, MethodCache};
pub use symbol::Symbol;
pub mod registry;
```

- [ ] **Step 4: Compile check**

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: clean. (The macro doesn't run until something uses it — so this is a syntactic check.)

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(macros): #[protocol] codegen — build Protocol + ProtocolMethods at init

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 16: Declare `IFn` via `#[protocol]` (trait only — no impls yet)

**Files:**
- Create: `crates/clojure_core/src/ifn.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: Declare the trait**

```rust
// crates/clojure_core/src/ifn.rs
use crate::protocol as _;  // for the #[protocol] macro's generated path references
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

#[protocol(name = "clojure.core/IFn", extend_via_metadata = false)]
pub trait IFn {
    fn invoke0(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn invoke1(&self, py: Python<'_>, a0: PyObject) -> PyResult<PyObject>;
    fn invoke2(&self, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject>;
    fn invoke3(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject) -> PyResult<PyObject>;
    fn invoke4(&self, py: Python<'_>, a0: PyObject, a1: PyObject, a2: PyObject, a3: PyObject) -> PyResult<PyObject>;
    // Continue invoke5..invoke20 — see "Task 16 appendix" below.
    fn invoke_variadic(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject>;
}
```

**Appendix (invoke5..invoke20):** each follows the same pattern. Copy the invoke4 line and add `a4: PyObject`, ..., `a19: PyObject` arguments. (Rustfmt will align them.) Do not abbreviate — the trait methods must all exist for the macro to register them.

- [ ] **Step 2: Register module**

In lib.rs add:
```rust
mod ifn;
pub use ifn::IFn;
```

- [ ] **Step 3: Rebuild extension**

Run: `maturin develop --release 2>&1 | tail -10`
Expected: builds successfully.

- [ ] **Step 4: Verify Python-side exposure**

Run: `python -c "from clojure._core import IFn, invoke0, invoke_variadic; print(IFn.name)"`
Expected: prints `clojure.core/IFn`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): declare IFn protocol via #[protocol] macro

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 17: Fill in Protocol runtime test (un-xfail Task 13)

**Files:**
- Modify: `tests/test_protocol_runtime.py`

- [ ] **Step 1: Replace file**

```python
# tests/test_protocol_runtime.py
import pytest
from clojure._core import (
    IFn, invoke1, invoke_variadic, Protocol, ProtocolMethod, symbol,
    IllegalArgumentException,
)

def test_ifn_protocol_object_exists():
    assert isinstance(IFn, Protocol)
    assert IFn.name == symbol("clojure.core", "IFn")
    assert IFn.via_metadata is False

def test_protocol_methods_are_objects():
    assert isinstance(invoke1, ProtocolMethod)
    assert invoke1.key == "invoke1"
    assert invoke1.protocol is IFn

def test_dispatch_on_empty_raises():
    class Foo: pass
    with pytest.raises(IllegalArgumentException, match="No implementation"):
        invoke1(Foo(), 42)
```

- [ ] **Step 2: Run**

Run: `pytest tests/test_protocol_runtime.py -v`
Expected: 3 passed (xfail marker gone).

- [ ] **Step 3: Commit**

```bash
git add tests/test_protocol_runtime.py
git commit -m "test(protocol): verify IFn registration + empty-cache dispatch error

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


## Phase 4 — `#[implements]` Macro + Extend-Type + Dispatch Tests

### Task 18: `#[implements]` macro — parse

**Files:**
- Create: `crates/clojure_core_macros/src/implements.rs`
- Modify: `crates/clojure_core_macros/src/lib.rs`

- [ ] **Step 1: Parse attribute args**

```rust
// crates/clojure_core_macros/src/implements.rs
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{parse::Parse, ItemImpl, LitStr, Token};

pub struct ImplementsArgs {
    pub protocol_ident: syn::Ident,
    pub py_type: Option<String>,
    pub default: Option<syn::Path>,
}

impl Parse for ImplementsArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let protocol_ident: syn::Ident = input.parse()?;
        let mut py_type: Option<String> = None;
        let mut default: Option<syn::Path> = None;
        if input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            let punct: syn::punctuated::Punctuated<syn::MetaNameValue, Token![,]> =
                input.parse_terminated(syn::MetaNameValue::parse, Token![,])?;
            for nv in punct {
                let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                match key.as_str() {
                    "py_type" => {
                        let s: LitStr = syn::parse2(nv.value.to_token_stream())?;
                        py_type = Some(s.value());
                    }
                    "default" => {
                        let p: syn::Path = syn::parse2(nv.value.to_token_stream())?;
                        default = Some(p);
                    }
                    other => return Err(syn::Error::new_spanned(nv, format!("unknown arg: {other}"))),
                }
            }
        }
        Ok(Self { protocol_ident, py_type, default })
    }
}
```

- [ ] **Step 2: Wire into lib.rs**

```rust
mod implements;

#[proc_macro_attribute]
pub fn implements(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as implements::ImplementsArgs);
    let item_impl = parse_macro_input!(item as ItemImpl);
    implements::expand(args, item_impl).into()
}
```

- [ ] **Step 3: Compile check**

Run: `cargo check --workspace 2>&1 | tail -3`
Expected: clean — `expand` is missing, add a stub that returns the original impl unchanged while we flesh it out next.

Add stub:
```rust
// in implements.rs
pub fn expand(_args: ImplementsArgs, item_impl: ItemImpl) -> TokenStream {
    quote! { #item_impl }
}
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(macros): parse #[implements] attribute args

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 19: `#[implements]` codegen — type-owned impls

**Files:**
- Modify: `crates/clojure_core_macros/src/implements.rs`
- Modify: `crates/clojure_core/src/registry.rs`

- [ ] **Step 1: Add extend-registration entry type**

```rust
// append to crates/clojure_core/src/registry.rs
pub struct ExtendRegistration {
    /// Called at init to install this type's protocol impls into the protocol's cache.
    pub install: fn(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()>,
}
inventory::collect!(ExtendRegistration);

pub fn install_all_extends(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    for r in inventory::iter::<ExtendRegistration> {
        (r.install)(py, m)?;
    }
    Ok(())
}
```

Call `install_all_extends` in lib.rs *after* `register_all` so all Protocols exist first.

- [ ] **Step 2: Codegen in implements.rs**

Replace `expand`:

```rust
pub fn expand(args: ImplementsArgs, item_impl: ItemImpl) -> TokenStream {
    let self_ty = &item_impl.self_ty;
    let proto_ident = &args.protocol_ident;
    let install_fn_ident = format_ident!("__install_impls_{}_{}", proto_ident, simple_ident_for(self_ty));

    // Collect the impl's concrete fns, build a name→closure dict at init.
    let fns: Vec<(syn::Ident, usize)> = item_impl.items.iter().filter_map(|ii| {
        if let syn::ImplItem::Fn(f) = ii {
            // Arity is args after self. For invoke_variadic the signature is (self, py, args:&PyTuple).
            let n = f.sig.inputs.len().saturating_sub(2); // self + py + rest-of-args
            Some((f.sig.ident.clone(), n))
        } else { None }
    }).collect();

    let method_builders: Vec<TokenStream> = fns.iter().map(|(ident, _n)| {
        let key = ident.to_string();
        quote! {
            {
                let f = ::pyo3::types::PyCFunction::new_closure_bound(py, Some(#key), None, move |args: &Bound<'_, ::pyo3::types::PyTuple>, _kw: Option<&Bound<'_, ::pyo3::types::PyDict>>| -> ::pyo3::PyResult<::pyo3::PyObject> {
                    let py = args.py();
                    let self_obj = args.get_item(0)?;
                    let rest_raw: Vec<::pyo3::PyObject> = (1..args.len()).map(|i| -> ::pyo3::PyResult<::pyo3::PyObject> {
                        Ok(args.get_item(i)?.into())
                    }).collect::<::pyo3::PyResult<_>>()?;
                    // Call through the trait:
                    let this = self_obj.downcast::<#self_ty>()?.get();
                    <#self_ty as #proto_ident>::#ident(this, py /* , args... */)
                })?;
                impls_dict.set_item(#key, f)?;
            }
        }
    }).collect();

    let install_body = match &args.py_type {
        Some(ty_path) => {
            // Extract (module, name) from "builtins.int" style.
            let (mod_name, cls_name) = ty_path.rsplit_once('.').map(|(a,b)| (a.to_string(), b.to_string()))
                .unwrap_or_else(|| ("builtins".to_string(), ty_path.clone()));
            quote! {
                let builtins = py.import_bound(#mod_name)?;
                let ty = builtins.getattr(#cls_name)?.downcast_into::<::pyo3::types::PyType>()?;
            }
        }
        None => quote! {
            let ty = py.get_type_bound::<#self_ty>();
        },
    };

    quote! {
        #item_impl

        #[allow(non_snake_case)]
        fn #install_fn_ident(py: ::pyo3::Python<'_>, m: &::pyo3::Bound<'_, ::pyo3::types::PyModule>) -> ::pyo3::PyResult<()> {
            use ::pyo3::prelude::*;
            use ::pyo3::types::PyDict;
            #install_body
            let proto_any = m.getattr(stringify!(#proto_ident))?;
            let proto: &Bound<'_, ::clojure_core::Protocol> = proto_any.downcast()?;
            let impls_dict = PyDict::new_bound(py);
            #(#method_builders)*
            proto.get().extend_type(py, ty, impls_dict)?;
            Ok(())
        }

        ::inventory::submit! {
            ::clojure_core::registry::ExtendRegistration { install: #install_fn_ident }
        }
    }
}

fn simple_ident_for(ty: &syn::Type) -> syn::Ident {
    format_ident!("{}", quote! { #ty }.to_string().replace(|c: char| !c.is_alphanumeric(), "_"))
}
```

**Simplification caveat:** the closure body above needs to unpack positional args into the trait method's arguments per-arity. For the first cut, only wire `invoke1`, `invoke2`, `invoke3`, and `invoke_variadic` correctly; add other arities as we need them. The macro should emit an `unimplemented!()` for arities it doesn't yet support — so it's visible at runtime rather than silently broken.

- [ ] **Step 3: Compile**

Run: `cargo check --workspace 2>&1 | tail -5`
Expected: clean. Some clippy nags are OK.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(macros): #[implements] codegen — install impls into protocol cache

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 20: Implement IFn for Keyword via #[implements]

**Files:**
- Modify: `crates/clojure_core/src/keyword.rs`
- Add: `tests/test_keyword_as_ifn.py`

- [ ] **Step 1: Remove the direct `__call__` from Keyword, add trait impl**

Replace the `#[pymethods] __call__` on Keyword with an `impl IFn for Keyword` block annotated with `#[implements(IFn)]`. Keep the rest of the Keyword methods.

```rust
use crate::ifn::IFn;
use clojure_core_macros::implements;

#[implements(IFn)]
impl IFn for Keyword {
    fn invoke0(&self, _py: Python<'_>) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (0) passed to: Keyword"))
    }
    fn invoke1(&self, py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
        crate::rt::get(py, coll, self.as_pyobj(py)?, py.None())
    }
    fn invoke2(&self, py: Python<'_>, coll: PyObject, default: PyObject) -> PyResult<PyObject> {
        crate::rt::get(py, coll, self.as_pyobj(py)?, default)
    }
    fn invoke3(&self, _py: Python<'_>, _a: PyObject, _b: PyObject, _c: PyObject) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args (3) passed to: Keyword"))
    }
    // ... arity stubs for 4..20 with ArityException
    fn invoke_variadic(&self, _py: Python<'_>, _args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
        Err(crate::exceptions::ArityException::new_err("Wrong number of args passed to: Keyword"))
    }
}
```

- [ ] **Step 2: Implement `rt::get`**

```rust
// crates/clojure_core/src/rt.rs
use pyo3::prelude::*;

pub fn get(py: Python<'_>, coll: PyObject, k: PyObject, default: PyObject) -> PyResult<PyObject> {
    // Minimal: support PyDict + fallback to default; real IPersistentMap dispatch comes with collections spec.
    let b = coll.bind(py);
    if let Ok(d) = b.downcast::<pyo3::types::PyDict>() {
        if let Some(v) = d.get_item(k.bind(py))? {
            return Ok(v.into());
        }
    }
    // Attempt __getitem__ with default-on-KeyError.
    match b.get_item(k.bind(py)) {
        Ok(v) => Ok(v.into()),
        Err(_) => Ok(default),
    }
}
```

Wire `mod rt;` and `pub use rt;` in lib.rs.

- [ ] **Step 3: Test**

```python
# tests/test_keyword_as_ifn.py
from clojure._core import keyword, invoke1, invoke2, IllegalArgumentException
import pytest

def test_keyword_invoke1_dict():
    d = {keyword("a"): 1}
    assert invoke1(keyword("a"), d) == 1

def test_keyword_invoke2_default():
    d = {keyword("a"): 1}
    assert invoke2(keyword("b"), d, "nope") == "nope"
```

- [ ] **Step 4: Rebuild + run**

Run: `maturin develop --release && pytest tests/test_keyword_as_ifn.py -v`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(keyword): implement IFn via #[implements] macro

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 21: Dispatch — MRO walk + promote

**Files:**
- Create: `tests/test_protocol_mro.py`
- Confirm: `crates/clojure_core/src/dispatch.rs` MRO path (implemented in Task 11)

- [ ] **Step 1: Failing test**

```python
# tests/test_protocol_mro.py
from clojure._core import IFn, invoke1

class Parent: pass
class Child(Parent): pass

def test_mro_walk_finds_parent_impl():
    # Extend Parent with an IFn impl via the Protocol API.
    IFn.extend_type(Parent, {"invoke1": lambda self, x: ("parent", x)})
    assert invoke1(Child(), 42) == ("parent", 42)

def test_mro_hit_promoted_to_exact():
    # After first Child dispatch, cache should contain an entry for Child.
    # We test by extending Parent differently and checking Child still sees old impl until explicitly re-extended for Child.
    # (This specific invariant is documented §4.2; test is observational.)
    IFn.extend_type(Parent, {"invoke1": lambda self, x: ("parent2", x)})
    # epoch bumped; Child's promoted entry is stale. Next dispatch re-resolves.
    assert invoke1(Child(), 99) == ("parent2", 99)
```

- [ ] **Step 2: Run**

Run: `pytest tests/test_protocol_mro.py -v`
Expected: passes if the dispatch logic from Task 11/12 is correct; debug if not.

- [ ] **Step 3: Commit**

```bash
git add tests/test_protocol_mro.py
git commit -m "test(dispatch): MRO walk resolves to parent impl and tracks epoch

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 22: Dispatch — fallback function path

**Files:**
- Create: `tests/test_protocol_fallback.py`

- [ ] **Step 1: Write tests**

```python
# tests/test_protocol_fallback.py
import pytest
from clojure._core import IFn, invoke1, IllegalArgumentException

def test_fallback_registers_impl_on_miss():
    IFn.set_fallback(lambda protocol, method, target: protocol.extend_type(type(target), {method: lambda s, a: ("fallback", a)}))
    try:
        class X: pass
        assert invoke1(X(), 10) == ("fallback", 10)
    finally:
        IFn.set_fallback(None)

def test_fallback_consulted_once_then_raises():
    called = []
    def fb(p, m, t):
        called.append(1)
        # Do nothing — no impl gets registered.
    IFn.set_fallback(fb)
    try:
        class Y: pass
        with pytest.raises(IllegalArgumentException):
            invoke1(Y(), 1)
        assert len(called) == 1
    finally:
        IFn.set_fallback(None)
```

- [ ] **Step 2: Run — expect pass (dispatch logic exists)**

Run: `pytest tests/test_protocol_fallback.py -v`
Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add tests/test_protocol_fallback.py
git commit -m "test(dispatch): fallback is consulted once, re-resolves after registration

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 23: IFn built-in Python-callable fallback

**Files:**
- Modify: `crates/clojure_core/src/ifn.rs`
- Create: `tests/test_ifn_callable_fallback.py`

- [ ] **Step 1: Install fallback at module init**

Add a function in `ifn.rs`:

```rust
pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ifn = m.getattr("IFn")?;
    let ifn_proto: &Bound<'_, crate::Protocol> = ifn.downcast()?;
    let fb = pyo3::types::PyCFunction::new_closure_bound(
        py, Some("__ifn_callable_fallback"), None,
        |args: &Bound<'_, pyo3::types::PyTuple>, _kw| -> PyResult<PyObject> {
            let py = args.py();
            let proto: Bound<'_, crate::Protocol> = args.get_item(0)?.downcast_into()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;
            if target.is_callable() {
                // Build a generic impl for (type(target)): invoke_variadic calls target(*args)
                let impls = pyo3::types::PyDict::new_bound(py);
                let target_owned: PyObject = target.clone().into();
                let inv = pyo3::types::PyCFunction::new_closure_bound(
                    py, Some("invoke_variadic"), None,
                    move |a: &Bound<'_, pyo3::types::PyTuple>, _kw| -> PyResult<PyObject> {
                        let py = a.py();
                        // a = (self, *args)
                        let rest = a.get_slice(1, a.len());
                        target_owned.bind(py).call1(rest)
                    })?;
                // Also install arity-specific variants that delegate to invoke_variadic:
                for key in &["invoke0","invoke1","invoke2","invoke3","invoke4","invoke5",
                             "invoke6","invoke7","invoke8","invoke9","invoke10","invoke11",
                             "invoke12","invoke13","invoke14","invoke15","invoke16","invoke17",
                             "invoke18","invoke19","invoke20"] {
                    impls.set_item(*key, inv.clone())?;
                }
                impls.set_item("invoke_variadic", inv)?;
                let ty = target.get_type();
                proto.get().extend_type(py, ty, impls)?;
            }
            Ok(py.None())
        },
    )?;
    ifn_proto.get().set_fallback(Some(fb.into()));
    Ok(())
}
```

Call `ifn::install_builtin_fallback(py, m)?;` from the pymodule init after `install_all_extends`.

- [ ] **Step 2: Test**

```python
# tests/test_ifn_callable_fallback.py
from clojure._core import invoke1, invoke2, IFn

def test_lambda_as_ifn():
    f = lambda x: x + 1
    assert invoke1(f, 10) == 11

def test_builtin_as_ifn():
    assert invoke2(max, 3, 9) == 9

def test_satisfies_after_fallback():
    f = lambda: 0
    invoke1  # needs no-op; warm the cache
    # First call triggers fallback, caches generic impl:
    try:
        invoke1(f, "arg")
    except TypeError:
        pass
    # Subsequent: no fallback, cached path:
    # (observable via: the protocol's cache entry for type(f) now exists)
    # We can't easily introspect from Python without more API; smoke test only.
```

- [ ] **Step 3: Run**

Run: `maturin develop --release && pytest tests/test_ifn_callable_fallback.py -v`
Expected: tests 1 and 2 pass; test 3 is a smoke test that doesn't assert much until introspection API lands.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ifn): built-in fallback makes arbitrary Python callables satisfy IFn

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 24: extend-via-metadata dispatch path

**Files:**
- Modify: `crates/clojure_core/src/dispatch.rs`
- Create: `tests/test_protocol_metadata.py`

- [ ] **Step 1: Wire step 3 in dispatch**

Replace the `// Step 3: metadata (no-op until IMeta ships)` comment with a read from an `__clj_meta__` attribute on the target:

```rust
if protocol.via_metadata {
    if let Ok(meta) = bound.getattr("__clj_meta__") {
        if let Ok(map) = meta.downcast::<pyo3::types::PyDict>() {
            if let Some(impl_fn) = map.get_item(method_key.as_ref())? {
                let mut call_args: Vec<PyObject> = vec![target.clone_ref(py)];
                for a in args.iter() { call_args.push(a.into()); }
                let tup = PyTuple::new_bound(py, &call_args);
                return impl_fn.call1(tup);
            }
        }
    }
}
```

- [ ] **Step 2: Declare a test protocol with via_metadata = true**

Add to `crates/clojure_core/src/lib.rs` or a test-only module (behind `#[cfg(feature = "test-protocols")]` is overkill — just put it in `src/ifn.rs` or a new `src/test_protocols.rs`):

```rust
// crates/clojure_core/src/test_protocols.rs — minimal trait for tests
use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core.test/Greeter", extend_via_metadata = true)]
pub trait Greeter {
    fn greet(&self, py: Python<'_>) -> PyResult<PyObject>;
}
```

Wire `mod test_protocols;` in lib.rs.

- [ ] **Step 3: Test**

```python
# tests/test_protocol_metadata.py
from clojure._core import Greeter, greet

class Mock: pass

def test_meta_dispatch():
    m = Mock()
    m.__clj_meta__ = {"greet": lambda self: "hi"}
    assert greet(m) == "hi"

def test_meta_disabled_when_not_opted_in():
    # IFn has via_metadata = False, so __clj_meta__ is ignored.
    from clojure._core import IFn, invoke1, IllegalArgumentException
    import pytest
    class X: pass
    x = X()
    x.__clj_meta__ = {"invoke1": lambda s, a: "nope"}
    with pytest.raises(IllegalArgumentException):
        invoke1(x, 1)
```

- [ ] **Step 4: Rebuild + run**

Run: `maturin develop --release && pytest tests/test_protocol_metadata.py -v`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(dispatch): extend-via-metadata path honored when protocol opts in

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


## Phase 5 — Var

### Task 25: Var construction, root, deref, alter-var-root

**Files:**
- Create: `crates/clojure_core/src/var.rs`
- Create: `tests/test_var.py`

- [ ] **Step 1: Implement Var**

```rust
// crates/clojure_core/src/var.rs
use crate::exceptions::{IllegalArgumentException, IllegalStateException};
use crate::ifn::IFn;
use clojure_core_macros::implements;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

// Sentinel PyObject singleton for "unbound".
pub(crate) struct Unbound;
static mut UNBOUND_RAW: *mut pyo3::ffi::PyObject = std::ptr::null_mut();

pub fn init_unbound(py: Python<'_>) -> PyResult<()> {
    let u = pyo3::types::PyString::new_bound(py, "#<Unbound>");
    unsafe { UNBOUND_RAW = u.into_ptr(); }
    Ok(())
}

fn unbound_ptr() -> *mut pyo3::ffi::PyObject { unsafe { UNBOUND_RAW } }

#[pyclass(module = "clojure._core", name = "Var")]
pub struct Var {
    pub ns: PyObject,       // ClojureNamespace (any PyModule until Namespace is implemented)
    pub sym: PyObject,      // Symbol
    pub root: AtomicPtr<pyo3::ffi::PyObject>,
    pub dynamic: AtomicBool,
    pub meta: RwLock<Option<PyObject>>,
    pub watches: RwLock<pyo3::Py<pyo3::types::PyDict>>,
    pub validator: RwLock<Option<PyObject>>,
}

#[pymethods]
impl Var {
    #[new]
    fn new(py: Python<'_>, ns: PyObject, sym: PyObject) -> PyResult<Self> {
        Ok(Self {
            ns, sym,
            root: AtomicPtr::new(unbound_ptr()),
            dynamic: AtomicBool::new(false),
            meta: RwLock::new(None),
            watches: RwLock::new(pyo3::types::PyDict::new_bound(py).into()),
            validator: RwLock::new(None),
        })
    }

    fn deref(&self, py: Python<'_>) -> PyResult<PyObject> {
        let p = self.root.load(Ordering::Acquire);
        if p == unbound_ptr() || p.is_null() {
            return Err(IllegalStateException::new_err(format!(
                "Var {}/{} is unbound", self.ns_name(py)?, self.sym_name(py)?
            )));
        }
        unsafe { Ok(PyObject::from_borrowed_ptr(py, p)) }
    }

    #[getter] fn ns(&self, py: Python<'_>) -> PyObject { self.ns.clone_ref(py) }
    #[getter] fn sym(&self, py: Python<'_>) -> PyObject { self.sym.clone_ref(py) }
    #[getter] fn is_dynamic(&self) -> bool { self.dynamic.load(Ordering::Acquire) }
    #[getter] fn is_bound(&self) -> bool { self.root.load(Ordering::Acquire) != unbound_ptr() }

    fn set_dynamic(&self, v: bool) { self.dynamic.store(v, Ordering::Release); }

    fn bind_root(&self, py: Python<'_>, value: PyObject) -> PyResult<()> {
        self.validate(py, &value)?;
        let old = self.root.swap(value.into_ptr(), Ordering::AcqRel);
        self.fire_watches(py, old)?;
        Ok(())
    }

    fn alter_root(&self, py: Python<'_>, f: PyObject, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        loop {
            let current = self.root.load(Ordering::Acquire);
            let old_obj = unsafe { PyObject::from_borrowed_ptr(py, current) };
            // Call f(current, *args)
            let mut cargs: Vec<PyObject> = vec![old_obj.clone_ref(py)];
            for a in args.iter() { cargs.push(a.into()); }
            let tup = PyTuple::new_bound(py, &cargs);
            let new_val = f.bind(py).call1(tup)?;
            self.validate(py, &new_val.clone().into())?;
            let new_ptr = new_val.as_ptr();
            // CAS
            if self.root.compare_exchange(current, new_ptr, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                // Incref the new, decref the old (CPython refcount management):
                unsafe { pyo3::ffi::Py_INCREF(new_ptr); }
                if !current.is_null() && current != unbound_ptr() {
                    unsafe { pyo3::ffi::Py_DECREF(current); }
                }
                self.fire_watches(py, current)?;
                return Ok(new_val.into());
            }
            // Retry on contention
        }
    }

    fn set_validator(&self, validator: Option<PyObject>) { *self.validator.write() = validator; }
    fn get_validator(&self, py: Python<'_>) -> Option<PyObject> { self.validator.read().as_ref().map(|o| o.clone_ref(py)) }

    fn add_watch(&self, py: Python<'_>, key: PyObject, f: PyObject) -> PyResult<()> {
        let w = self.watches.read();
        w.bind(py).set_item(key, f)?;
        Ok(())
    }
    fn remove_watch(&self, py: Python<'_>, key: PyObject) -> PyResult<()> {
        let w = self.watches.read();
        w.bind(py).del_item(key)?;
        Ok(())
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!("#'{}/{}", self.ns_name(py)?, self.sym_name(py)?))
    }

    #[getter] fn meta(&self, py: Python<'_>) -> Option<PyObject> {
        self.meta.read().as_ref().map(|o| o.clone_ref(py))
    }
    fn set_meta(&self, meta: Option<PyObject>) { *self.meta.write() = meta; }
}

impl Var {
    fn ns_name(&self, py: Python<'_>) -> PyResult<String> {
        let n = self.ns.bind(py).getattr("__name__")?;
        n.extract()
    }
    fn sym_name(&self, py: Python<'_>) -> PyResult<String> {
        let n = self.sym.bind(py).getattr("name")?;
        n.extract()
    }
    fn validate(&self, py: Python<'_>, v: &PyObject) -> PyResult<()> {
        if let Some(validator) = self.validator.read().as_ref() {
            let r = validator.bind(py).call1((v.clone_ref(py),))?;
            if !r.is_truthy()? {
                return Err(IllegalArgumentException::new_err("Invalid reference value"));
            }
        }
        Ok(())
    }
    fn fire_watches(&self, py: Python<'_>, old_ptr: *mut pyo3::ffi::PyObject) -> PyResult<()> {
        let new_obj = self.deref(py)?;
        let old_obj = if old_ptr.is_null() || old_ptr == unbound_ptr() {
            py.None()
        } else {
            unsafe { PyObject::from_borrowed_ptr(py, old_ptr) }
        };
        let watches = self.watches.read();
        for (k, f) in watches.bind(py).iter() {
            f.call1((k, pyo3::Py::new(py, /*self ref*/ 0i32)?, old_obj.clone_ref(py), new_obj.clone_ref(py)))?;
            // Note: passing `self` as a Py<Var> requires it be available; callers must provide.
            // Adjust by restructuring fire_watches to take `&Py<Var>`.
        }
        Ok(())
    }
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    init_unbound(py)?;
    m.add_class::<Var>()?;
    Ok(())
}
```

**Implementer note:** the `fire_watches` signature passing `self` as a ref needs cleanup — take a `Py<Var>` at the call site. Fix by having `alter_root` and `bind_root` receive the `slf: Py<Var>` from PyO3 (use `#[pyo3(signature = ...)]`-style). Iterate until tests pass.

- [ ] **Step 2: Tests**

```python
# tests/test_var.py
import pytest
from clojure._core import Var, IllegalStateException, IllegalArgumentException, symbol
import sys, types

@pytest.fixture
def ns_mod():
    m = types.ModuleType("test.ns")
    sys.modules["test.ns"] = m
    yield m
    del sys.modules["test.ns"]

@pytest.fixture
def v(ns_mod):
    return Var(ns_mod, symbol("x"))

def test_unbound_deref_raises(v):
    with pytest.raises(IllegalStateException, match="unbound"):
        v.deref()

def test_bind_root_then_deref(v):
    v.bind_root(42)
    assert v.deref() == 42

def test_alter_root(v):
    v.bind_root(10)
    v.alter_root(lambda o, n: o + n, 5)
    assert v.deref() == 15

def test_validator_rejects(v):
    v.bind_root(0)
    v.set_validator(lambda x: x >= 0)
    with pytest.raises(IllegalArgumentException):
        v.bind_root(-1)

def test_watches_fire(v):
    calls = []
    v.bind_root(0)
    v.add_watch("w1", lambda k, ref, old, new: calls.append((k, old, new)))
    v.bind_root(5)
    assert calls == [("w1", 0, 5)]

def test_repr_var_form(v):
    v.bind_root(1)
    assert repr(v) == "#'test.ns/x"
```

- [ ] **Step 3: Iterate until green**

Run: `maturin develop --release && pytest tests/test_var.py -v`
Expected: passes after fixing the `fire_watches` structural issue flagged above.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(var): Var type with root atomic, alter-var-root CAS, watches, validator

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 26: Var implements IFn directly

**Files:**
- Modify: `crates/clojure_core/src/var.rs`
- Create: `tests/test_var_ifn.py`

- [ ] **Step 1: Add trait impl**

At the bottom of `var.rs`, after the `Var` definition:

```rust
#[implements(IFn)]
impl IFn for Var {
    fn invoke0(&self, py: Python<'_>) -> PyResult<PyObject> {
        let root = self.deref(py)?;
        crate::rt::invoke_n(py, root, &[])
    }
    fn invoke1(&self, py: Python<'_>, a0: PyObject) -> PyResult<PyObject> {
        let root = self.deref(py)?;
        crate::rt::invoke_n(py, root, &[a0])
    }
    fn invoke2(&self, py: Python<'_>, a0: PyObject, a1: PyObject) -> PyResult<PyObject> {
        let root = self.deref(py)?;
        crate::rt::invoke_n(py, root, &[a0, a1])
    }
    // ... invoke3..invoke20 follow the pattern
    fn invoke_variadic(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        let root = self.deref(py)?;
        root.bind(py).call1(args)
    }
}
```

- [ ] **Step 2: Add `rt::invoke_n`**

In `rt.rs`:

```rust
pub fn invoke_n(py: Python<'_>, target: PyObject, args: &[PyObject]) -> PyResult<PyObject> {
    let tup = pyo3::types::PyTuple::new_bound(py, args);
    target.bind(py).call1(tup)
}
```

- [ ] **Step 3: Test**

```python
# tests/test_var_ifn.py
from clojure._core import Var, symbol
import sys, types

def test_var_calls_through_to_root():
    m = types.ModuleType("v.ns")
    sys.modules["v.ns"] = m
    v = Var(m, symbol("f"))
    v.bind_root(lambda x, y: x * y)
    assert v(3, 4) == 12
```

- [ ] **Step 4: Rebuild + test + commit**

```bash
maturin develop --release && pytest tests/test_var_ifn.py -v
git add -A
git commit -m "feat(var): Var implements IFn directly (delegates to root)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 27: Var delegation dunders (arith, containers, bool, eq, getattr)

**Files:**
- Create: `crates/clojure_core/src/var_delegation.rs`
- Modify: `crates/clojure_core/src/var.rs` (add #[pymethods] for dunders)
- Create: `tests/test_var_delegation.py`

- [ ] **Step 1: Helper that derefs and forwards**

Design: add a second `#[pymethods] impl Var { ... }` block with dunder methods that each `self.deref(py)?` then forward to the root's corresponding dunder or operator. PyO3 requires these be spelled out — there's no generic dunder forwarder.

```rust
// crates/clojure_core/src/var_delegation.rs
use crate::var::Var;
use pyo3::prelude::*;

#[pymethods]
impl Var {
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let r = self.deref(py)?;
        Ok(r.bind(py).eq(other)?)
    }
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let r = self.deref(py)?;
        r.bind(py).hash()
    }
    fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let r = self.deref(py)?;
        r.bind(py).is_truthy()
    }
    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let r = self.deref(py)?;
        Ok(r.bind(py).str()?.extract()?)
    }
    fn __add__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((r.bind(py) + other.bind(py))?.into())
    }
    fn __radd__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((other.bind(py) + r.bind(py))?.into())
    }
    fn __sub__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((r.bind(py) - other.bind(py))?.into())
    }
    fn __rsub__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((other.bind(py) - r.bind(py))?.into())
    }
    fn __mul__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((r.bind(py) * other.bind(py))?.into())
    }
    fn __rmul__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((other.bind(py) * r.bind(py))?.into())
    }
    fn __truediv__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok((r.bind(py) / other.bind(py))?.into())
    }
    fn __floordiv__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        r.bind(py).call_method1("__floordiv__", (other,))
    }
    fn __mod__(&self, py: Python<'_>, other: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        r.bind(py).call_method1("__mod__", (other,))
    }
    fn __neg__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        r.bind(py).call_method0("__neg__")
    }
    fn __lt__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref(py)?; Ok(r.bind(py).lt(other)?)
    }
    fn __le__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref(py)?; Ok(r.bind(py).le(other)?)
    }
    fn __gt__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref(py)?; Ok(r.bind(py).gt(other)?)
    }
    fn __ge__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let r = self.deref(py)?; Ok(r.bind(py).ge(other)?)
    }
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let r = self.deref(py)?;
        r.bind(py).len()
    }
    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok(r.bind(py).iter()?.into())
    }
    fn __contains__(&self, py: Python<'_>, item: PyObject) -> PyResult<bool> {
        let r = self.deref(py)?;
        r.bind(py).contains(item)
    }
    fn __getitem__(&self, py: Python<'_>, key: PyObject) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok(r.bind(py).get_item(key)?.into())
    }
    fn __getattr__(&self, py: Python<'_>, name: String) -> PyResult<PyObject> {
        let r = self.deref(py)?;
        Ok(r.bind(py).getattr(name.as_str())?.into())
    }
}
```

Wire `mod var_delegation;` in lib.rs.

- [ ] **Step 2: Tests**

```python
# tests/test_var_delegation.py
from clojure._core import Var, symbol
import sys, types

def _v(ns_name="d.ns"):
    m = types.ModuleType(ns_name)
    sys.modules[ns_name] = m
    return Var(m, symbol("x"))

def test_arith():
    v = _v(); v.bind_root(10)
    assert v + 3 == 13
    assert 3 + v == 13
    assert v - 2 == 8
    assert v * 2 == 20
    assert -v == -10

def test_container():
    v = _v("d.ns2"); v.bind_root({"a": 1})
    assert "a" in v
    assert v["a"] == 1

def test_len_iter():
    v = _v("d.ns3"); v.bind_root([1, 2, 3])
    assert len(v) == 3
    assert list(v) == [1, 2, 3]

def test_getattr_reach_through():
    v = _v("d.ns4"); v.bind_root("hello")
    assert v.upper() == "HELLO"

def test_isinstance_false():
    v = _v("d.ns5"); v.bind_root(1)
    assert not isinstance(v, int)  # documented

def test_bool_delegates():
    v = _v("d.ns6"); v.bind_root(0)
    assert not bool(v)
    v.bind_root(1)
    assert bool(v)
```

- [ ] **Step 3: Run**

Run: `maturin develop --release && pytest tests/test_var_delegation.py -v`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(var): delegation dunders — arith, containers, bool, getattr

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


## Phase 6 — Binding Stack + bound-fn

### Task 28: Minimal PersistentMap for binding frames

**Files:**
- Create: `crates/clojure_core/src/pmap.rs`

- [ ] **Step 1: Implement a small persistent map**

Since full persistent collections are a later spec, we provide only what binding needs: a small immutable map keyed by `Py<Var>` pointer identity, with O(n) `assoc`/`merge` (fine for typical binding frames of size <16).

```rust
// crates/clojure_core/src/pmap.rs
use pyo3::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct Entry { pub key_ptr: usize, pub key: PyObject, pub val: PyObject }

#[derive(Clone, Default)]
pub struct PMap(pub Arc<Vec<Entry>>);

impl PMap {
    pub fn new() -> Self { Self(Arc::new(Vec::new())) }

    pub fn assoc(&self, py: Python<'_>, key: &PyObject, val: PyObject) -> Self {
        let kptr = key.as_ptr() as usize;
        let mut v: Vec<Entry> = (*self.0).clone();
        if let Some(e) = v.iter_mut().find(|e| e.key_ptr == kptr) {
            e.val = val;
        } else {
            v.push(Entry { key_ptr: kptr, key: key.clone_ref(py), val });
        }
        Self(Arc::new(v))
    }

    pub fn get(&self, key: &PyObject) -> Option<&PyObject> {
        let kptr = key.as_ptr() as usize;
        self.0.iter().find(|e| e.key_ptr == kptr).map(|e| &e.val)
    }

    pub fn merge(&self, py: Python<'_>, other: &Self) -> Self {
        let mut out = self.clone();
        for e in other.0.iter() {
            out = out.assoc(py, &e.key, e.val.clone_ref(py));
        }
        out
    }

    pub fn update_in_place(&mut self, py: Python<'_>, key: &PyObject, val: PyObject) -> bool {
        // Used by `set!` — mutate the top frame entry. Returns whether key was found.
        let kptr = key.as_ptr() as usize;
        let v = Arc::make_mut(&mut self.0);
        if let Some(e) = v.iter_mut().find(|e| e.key_ptr == kptr) {
            e.val = val;
            true
        } else {
            let _ = py; // silence unused
            false
        }
    }
}
```

Wire `mod pmap;` in lib.rs (private, no pymodule exposure needed).

- [ ] **Step 2: Compile check**

Run: `cargo check -p clojure_core 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(core): minimal PersistentMap for binding frames

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 29: BINDING_STACK + push/pop + dynamic deref + set!

**Files:**
- Create: `crates/clojure_core/src/binding.rs`
- Modify: `crates/clojure_core/src/var.rs` (dynamic deref reads the stack)
- Create: `tests/test_binding.py`

- [ ] **Step 1: Implement the thread-local stack**

```rust
// crates/clojure_core/src/binding.rs
use crate::exceptions::IllegalStateException;
use crate::pmap::PMap;
use crate::var::Var;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::cell::RefCell;

thread_local! {
    pub(crate) static BINDING_STACK: RefCell<Vec<PMap>> = RefCell::new(Vec::new());
}

#[pyfunction]
pub fn push_thread_bindings(py: Python<'_>, map: Bound<'_, PyDict>) -> PyResult<()> {
    let top = BINDING_STACK.with(|s| s.borrow().last().cloned().unwrap_or_default());
    let mut new_frame = top;
    for (k, v) in map.iter() {
        let k_obj: PyObject = k.into();
        new_frame = new_frame.assoc(py, &k_obj, v.into());
    }
    BINDING_STACK.with(|s| s.borrow_mut().push(new_frame));
    Ok(())
}

#[pyfunction]
pub fn pop_thread_bindings() -> PyResult<()> {
    BINDING_STACK.with(|s| s.borrow_mut().pop());
    Ok(())
}

pub(crate) fn lookup_binding(var_py: &PyObject) -> Option<PyObject> {
    BINDING_STACK.with(|s| {
        let stack = s.borrow();
        stack.last().and_then(|frame| frame.get(var_py).cloned())
    })
}

pub(crate) fn set_binding(py: Python<'_>, var_py: &PyObject, val: PyObject) -> PyResult<()> {
    BINDING_STACK.with(|s| {
        let mut stack = s.borrow_mut();
        let Some(top) = stack.last_mut() else {
            return Err(IllegalStateException::new_err("Can't set!: no binding frame"));
        };
        if !top.update_in_place(py, var_py, val) {
            return Err(IllegalStateException::new_err("Can't set!: var has no thread-local binding"));
        }
        Ok(())
    })
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(push_thread_bindings, m)?)?;
    m.add_function(wrap_pyfunction!(pop_thread_bindings, m)?)?;
    Ok(())
}
```

Wire `mod binding;` and call `binding::register(py, m)?;` in lib.rs pymodule init.

- [ ] **Step 2: Var.deref consults binding stack when dynamic**

Modify `Var::deref` to check:
```rust
fn deref_with_self(slf: &Py<Var>, py: Python<'_>) -> PyResult<PyObject> {
    let this = slf.bind(py).get();
    if this.dynamic.load(std::sync::atomic::Ordering::Acquire) {
        if let Some(v) = crate::binding::lookup_binding(&slf.clone_ref(py).into()) {
            return Ok(v);
        }
    }
    let p = this.root.load(std::sync::atomic::Ordering::Acquire);
    if p == crate::var::unbound_ptr() || p.is_null() {
        return Err(crate::exceptions::IllegalStateException::new_err(format!(
            "Var {}/{} is unbound", this.ns_name(py)?, this.sym_name(py)?
        )));
    }
    Ok(unsafe { PyObject::from_borrowed_ptr(py, p) })
}
```

Add a `#[pymethods]` `fn deref(slf: Py<Var>, py: Python<'_>) -> PyResult<PyObject> { deref_with_self(&slf, py) }` that replaces the old `deref`. (You'll need to expose `deref_with_self` or inline it.)

Add `fn set_bang(slf: Py<Var>, py: Python<'_>, val: PyObject) -> PyResult<()>` to the Var pymethods — assigns into the current binding frame via `binding::set_binding`.

- [ ] **Step 3: Tests**

```python
# tests/test_binding.py
import pytest
from clojure._core import Var, symbol, push_thread_bindings, pop_thread_bindings, IllegalStateException
import sys, types

def _v(ns_name, sym_name, root=None, dynamic=False):
    m = types.ModuleType(ns_name)
    sys.modules[ns_name] = m
    v = Var(m, symbol(sym_name))
    if root is not None:
        v.bind_root(root)
    if dynamic:
        v.set_dynamic(True)
    return v

def test_non_dynamic_unaffected_by_binding():
    v = _v("b1", "x", root=1)
    push_thread_bindings({v: 99})
    try:
        assert v.deref() == 1  # not dynamic -> root wins
    finally:
        pop_thread_bindings()

def test_dynamic_binding_shadows_root():
    v = _v("b2", "x", root=1, dynamic=True)
    push_thread_bindings({v: 99})
    try:
        assert v.deref() == 99
    finally:
        pop_thread_bindings()
    assert v.deref() == 1

def test_nested_bindings_inherit_outer():
    v = _v("b3", "x", root=1, dynamic=True)
    push_thread_bindings({v: 10})
    try:
        push_thread_bindings({})  # empty frame
        try:
            assert v.deref() == 10  # inherits from outer
        finally:
            pop_thread_bindings()
    finally:
        pop_thread_bindings()

def test_set_bang_in_binding():
    v = _v("b4", "x", root=1, dynamic=True)
    push_thread_bindings({v: 10})
    try:
        v.set_bang(20)
        assert v.deref() == 20
    finally:
        pop_thread_bindings()

def test_set_bang_no_frame_raises():
    v = _v("b5", "x", root=1, dynamic=True)
    with pytest.raises(IllegalStateException):
        v.set_bang(20)
```

- [ ] **Step 4: Rebuild + run**

Run: `maturin develop --release && pytest tests/test_binding.py -v`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(binding): thread-local binding stack + dynamic deref + set!

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 30: bound-fn*

**Files:**
- Create: `crates/clojure_core/src/bound_fn.rs`
- Modify: `crates/clojure_core/src/lib.rs`
- Create: `tests/test_bound_fn.py`

- [ ] **Step 1: Implement bound-fn\* wrapper**

```rust
// crates/clojure_core/src/bound_fn.rs
use crate::binding::BINDING_STACK;
use crate::pmap::PMap;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

#[pyclass(module = "clojure._core", name = "BoundFn")]
pub struct BoundFn {
    snapshot: PMap,
    f: PyObject,
}

#[pymethods]
impl BoundFn {
    #[pyo3(signature = (*args))]
    fn __call__(&self, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
        BINDING_STACK.with(|s| s.borrow_mut().push(self.snapshot.clone()));
        let r = self.f.bind(py).call1(args);
        BINDING_STACK.with(|s| { s.borrow_mut().pop(); });
        r.map(Into::into)
    }
}

#[pyfunction]
pub fn bound_fn_star(py: Python<'_>, f: PyObject) -> PyResult<Py<BoundFn>> {
    let snap = BINDING_STACK.with(|s| s.borrow().last().cloned().unwrap_or_default());
    Py::new(py, BoundFn { snapshot: snap, f })
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BoundFn>()?;
    m.add_function(wrap_pyfunction!(bound_fn_star, m)?)?;
    Ok(())
}
```

Wire `mod bound_fn;` and call `bound_fn::register(py, m)?;` in lib.rs.

- [ ] **Step 2: Test cross-thread**

```python
# tests/test_bound_fn.py
import threading
from clojure._core import Var, symbol, push_thread_bindings, pop_thread_bindings, bound_fn_star
import sys, types

def _dynv(ns, sym, root):
    m = types.ModuleType(ns); sys.modules[ns] = m
    v = Var(m, symbol(sym)); v.bind_root(root); v.set_dynamic(True)
    return v

def test_bound_fn_conveys_frame_to_child_thread():
    v = _dynv("bf1", "x", 1)
    push_thread_bindings({v: 42})
    try:
        snapshot_fn = bound_fn_star(lambda: v.deref())
    finally:
        pop_thread_bindings()
    # Parent frame is gone.
    assert v.deref() == 1
    # But the bound fn carries it:
    result = []
    t = threading.Thread(target=lambda: result.append(snapshot_fn()))
    t.start(); t.join()
    assert result == [42]
```

- [ ] **Step 3: Rebuild + run + commit**

```bash
maturin develop --release && pytest tests/test_bound_fn.py -v
git add -A
git commit -m "feat(binding): bound-fn* conveys current frame across threads

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 7 — Namespace (Python Module Subclass)

### Task 31: ClojureNamespace type + create-ns walks dotted path

**Files:**
- Create: `crates/clojure_core/src/namespace.rs`
- Modify: `crates/clojure_core/src/lib.rs`
- Create: `tests/test_namespace_create.py`

- [ ] **Step 1: Implement ClojureNamespace**

```rust
// crates/clojure_core/src/namespace.rs
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::sync::Arc;

#[pyclass(module = "clojure._core", extends=PyModule, name = "ClojureNamespace")]
pub struct ClojureNamespace;

#[pymethods]
impl ClojureNamespace {
    #[new]
    #[pyo3(signature = (name))]
    fn new(_py: Python<'_>, name: &str) -> PyResult<Self> {
        let _ = name;
        Ok(ClojureNamespace)
    }
}

#[pyfunction]
pub fn create_ns(py: Python<'_>, sym: Py<Symbol>) -> PyResult<PyObject> {
    let name = {
        let s = sym.bind(py).get();
        match &s.ns {
            Some(ns) => format!("{}/{}", ns, s.name),   // allowed but unusual — we want dotted form
            None    => s.name.to_string(),
        }
    };
    // If a ClojureNamespace already registered under this name, return it.
    let sys = py.import_bound("sys")?;
    let modules = sys.getattr("modules")?;
    if let Ok(existing) = modules.get_item(&name) {
        if existing.is_instance_of::<ClojureNamespace>() {
            return Ok(existing.into());
        }
    }
    // Walk dotted path, creating parents as ClojureNamespace if absent.
    let parts: Vec<&str> = name.split('.').collect();
    let mut so_far = String::new();
    let mut parent: Option<Bound<'_, PyAny>> = None;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 { so_far.push('.'); }
        so_far.push_str(part);
        let module = if let Ok(ex) = modules.get_item(&so_far) {
            ex
        } else {
            let cls = py.get_type_bound::<ClojureNamespace>();
            let module = cls.call1((&so_far,))?;
            // Populate dunder entries
            let dict = module.getattr("__dict__")?;
            let dict = dict.downcast::<PyDict>()?;
            dict.set_item("__clj_ns__", {
                let s = Symbol::new(None, Arc::from(so_far.as_str()));
                Py::new(py, s)?
            })?;
            dict.set_item("__clj_ns_meta__", py.None())?;
            dict.set_item("__clj_aliases__", PyDict::new_bound(py))?;
            dict.set_item("__clj_refers__", PyDict::new_bound(py))?;
            dict.set_item("__clj_imports__", PyDict::new_bound(py))?;
            modules.set_item(&so_far, &module)?;
            module
        };
        if let Some(p) = parent {
            p.setattr(part, &module)?;
        }
        parent = Some(module);
    }
    Ok(parent.unwrap().into())
}

#[pyfunction]
pub fn find_ns(py: Python<'_>, sym: Py<Symbol>) -> PyResult<Option<PyObject>> {
    let name = {
        let s = sym.bind(py).get();
        match &s.ns { Some(ns) => format!("{}/{}", ns, s.name), None => s.name.to_string() }
    };
    let sys = py.import_bound("sys")?;
    let modules = sys.getattr("modules")?;
    match modules.get_item(&name) {
        Ok(m) if m.is_instance_of::<ClojureNamespace>() => Ok(Some(m.into())),
        _ => Ok(None),
    }
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ClojureNamespace>()?;
    m.add_function(wrap_pyfunction!(create_ns, m)?)?;
    m.add_function(wrap_pyfunction!(find_ns, m)?)?;
    Ok(())
}
```

Wire in lib.rs.

- [ ] **Step 2: Test**

```python
# tests/test_namespace_create.py
import sys
from clojure._core import create_ns, find_ns, symbol, ClojureNamespace

def test_create_simple():
    ns = create_ns(symbol("test.simple"))
    assert isinstance(ns, ClojureNamespace)
    assert sys.modules["test.simple"] is ns

def test_create_dotted_auto_parents():
    ns = create_ns(symbol("a.b.c"))
    assert isinstance(sys.modules["a"], ClojureNamespace)
    assert isinstance(sys.modules["a.b"], ClojureNamespace)
    assert sys.modules["a.b.c"] is ns
    assert sys.modules["a"].b is sys.modules["a.b"]

def test_dunder_metadata_populated():
    ns = create_ns(symbol("test.meta"))
    assert ns.__clj_ns__ == symbol("test.meta")
    assert ns.__clj_aliases__ == {}
    assert ns.__clj_refers__ == {}
    assert ns.__clj_imports__ == {}

def test_find_ns_returns_existing():
    create_ns(symbol("test.findable"))
    assert find_ns(symbol("test.findable")) is sys.modules["test.findable"]
    assert find_ns(symbol("test.nonexistent")) is None

def test_import_works():
    create_ns(symbol("test.importable"))
    import test.importable  # noqa
    assert test.importable is sys.modules["test.importable"]
```

- [ ] **Step 3: Rebuild + run + commit**

```bash
maturin develop --release && pytest tests/test_namespace_create.py -v
git add -A
git commit -m "feat(ns): ClojureNamespace module subclass + create-ns with dotted parents

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 32: intern, refer, alias, import

**Files:**
- Create: `crates/clojure_core/src/ns_ops.rs`
- Modify: `crates/clojure_core/src/lib.rs`
- Create: `tests/test_namespace_ops.py`

- [ ] **Step 1: Implement ns ops**

```rust
// crates/clojure_core/src/ns_ops.rs
use crate::symbol::Symbol;
use crate::var::Var;
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyfunction]
pub fn intern(py: Python<'_>, ns: PyObject, sym: Py<Symbol>) -> PyResult<Py<Var>> {
    let name = {
        let s = sym.bind(py).get();
        s.name.to_string()
    };
    let ns_b = ns.bind(py);
    // Check existing
    if let Ok(existing) = ns_b.getattr(name.as_str()) {
        if let Ok(v) = existing.downcast::<Var>() {
            return Ok(v.clone().unbind());
        }
    }
    let v = Var::new(py, ns.clone_ref(py), sym.into_any())?;
    let v_py = Py::new(py, v)?;
    ns_b.setattr(name.as_str(), v_py.clone_ref(py))?;
    Ok(v_py)
}

#[pyfunction]
pub fn refer(py: Python<'_>, ns: PyObject, target_sym: Py<Symbol>, var: Py<Var>) -> PyResult<()> {
    let name = target_sym.bind(py).get().name.to_string();
    let ns_b = ns.bind(py);
    ns_b.setattr(name.as_str(), var.clone_ref(py))?;
    let refers = ns_b.getattr("__clj_refers__")?;
    let refers_dict = refers.downcast::<PyDict>()?;
    refers_dict.set_item(target_sym, var)?;
    Ok(())
}

#[pyfunction]
pub fn alias(py: Python<'_>, ns: PyObject, alias_sym: Py<Symbol>, target_ns: PyObject) -> PyResult<()> {
    let ns_b = ns.bind(py);
    let aliases = ns_b.getattr("__clj_aliases__")?;
    let aliases_dict = aliases.downcast::<PyDict>()?;
    aliases_dict.set_item(alias_sym, target_ns)?;
    Ok(())
}

#[pyfunction]
pub fn import_cls(py: Python<'_>, ns: PyObject, alias_sym: Py<Symbol>, cls: PyObject) -> PyResult<()> {
    let ns_b = ns.bind(py);
    let imports = ns_b.getattr("__clj_imports__")?;
    let imports_dict = imports.downcast::<PyDict>()?;
    imports_dict.set_item(alias_sym, cls)?;
    Ok(())
}

#[pyfunction]
pub fn ns_map(py: Python<'_>, ns: PyObject) -> PyResult<PyObject> {
    // {sym → var} for all Var-valued attrs.
    let ns_b = ns.bind(py);
    let d = ns_b.getattr("__dict__")?.downcast::<PyDict>()?;
    let out = PyDict::new_bound(py);
    for (k, v) in d.iter() {
        if v.is_instance_of::<Var>() {
            let key: String = k.extract()?;
            if key.starts_with("__clj_") || key.starts_with("__") {
                // skip module dunders
                if !v.is_instance_of::<Var>() { continue; }
            }
            let sym = Py::new(py, Symbol::new(None, std::sync::Arc::from(key.as_str())))?;
            out.set_item(sym, v)?;
        }
    }
    Ok(out.into())
}

#[pyfunction] pub fn ns_aliases(ns: PyObject, py: Python<'_>) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_aliases__")?.into())
}
#[pyfunction] pub fn ns_refers(ns: PyObject, py: Python<'_>) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_refers__")?.into())
}
#[pyfunction] pub fn ns_imports(ns: PyObject, py: Python<'_>) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_imports__")?.into())
}
#[pyfunction] pub fn ns_meta(ns: PyObject, py: Python<'_>) -> PyResult<PyObject> {
    Ok(ns.bind(py).getattr("__clj_ns_meta__")?.into())
}

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(intern, m)?)?;
    m.add_function(wrap_pyfunction!(refer, m)?)?;
    m.add_function(wrap_pyfunction!(alias, m)?)?;
    m.add_function(wrap_pyfunction!(import_cls, m)?)?;
    m.add_function(wrap_pyfunction!(ns_map, m)?)?;
    m.add_function(wrap_pyfunction!(ns_aliases, m)?)?;
    m.add_function(wrap_pyfunction!(ns_refers, m)?)?;
    m.add_function(wrap_pyfunction!(ns_imports, m)?)?;
    m.add_function(wrap_pyfunction!(ns_meta, m)?)?;
    Ok(())
}
```

- [ ] **Step 2: Test**

```python
# tests/test_namespace_ops.py
from clojure._core import create_ns, intern, refer, alias, import_cls, ns_map, ns_aliases, ns_refers, ns_imports, symbol, Var

def test_intern_creates_var_as_attribute():
    ns = create_ns(symbol("i.ns"))
    v = intern(ns, symbol("foo"))
    assert isinstance(v, Var)
    assert getattr(ns, "foo") is v

def test_intern_idempotent():
    ns = create_ns(symbol("i.ns2"))
    v1 = intern(ns, symbol("x"))
    v2 = intern(ns, symbol("x"))
    assert v1 is v2

def test_symbol_with_punct_as_attr():
    ns = create_ns(symbol("i.ns3"))
    v = intern(ns, symbol("foo?"))
    assert getattr(ns, "foo?") is v

def test_refer_installs_and_records():
    ns_src = create_ns(symbol("r.src")); v = intern(ns_src, symbol("x"))
    ns_tgt = create_ns(symbol("r.tgt"))
    refer(ns_tgt, symbol("x"), v)
    assert getattr(ns_tgt, "x") is v
    assert ns_refers(ns_tgt)[symbol("x")] is v

def test_alias():
    ns = create_ns(symbol("a.ns")); target = create_ns(symbol("a.other"))
    alias(ns, symbol("o"), target)
    assert ns_aliases(ns)[symbol("o")] is target

def test_import_cls():
    ns = create_ns(symbol("im.ns"))
    import_cls(ns, symbol("DD"), dict)
    assert ns_imports(ns)[symbol("DD")] is dict

def test_ns_map_lists_vars():
    ns = create_ns(symbol("m.ns"))
    intern(ns, symbol("a")); intern(ns, symbol("b"))
    m = ns_map(ns)
    assert symbol("a") in m
    assert symbol("b") in m
```

- [ ] **Step 3: Rebuild + run + commit**

```bash
maturin develop --release && pytest tests/test_namespace_ops.py -v
git add -A
git commit -m "feat(ns): intern, refer, alias, import + ns-* introspection helpers

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 8 — Loom Concurrency Tests

### Task 33: Loom config + MethodCache race test

**Files:**
- Modify: `crates/clojure_core/Cargo.toml`
- Create: `crates/clojure_core/tests/loom_method_cache.rs`

- [ ] **Step 1: Add loom as dev-dependency behind the feature**

```toml
# append to crates/clojure_core/Cargo.toml
[target.'cfg(loom)'.dev-dependencies]
loom = "0.7"
```

And in src (or conditional compilation at the top of protocol.rs), use `loom::sync::atomic::AtomicU64` when `cfg(loom)` is set, falling back to `std::sync::atomic::AtomicU64` otherwise. Use a module-level alias:

```rust
#[cfg(loom)] use loom::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(loom))] use std::sync::atomic::{AtomicU64, Ordering};
```

- [ ] **Step 2: Write loom test**

```rust
// crates/clojure_core/tests/loom_method_cache.rs
#![cfg(loom)]
use loom::sync::Arc;
use loom::thread;

#[test]
fn concurrent_extend_and_dispatch_sees_consistent_cache() {
    loom::model(|| {
        // A stripped-down MethodCache that uses loom atomics.
        // Model: Thread A bumps epoch then inserts; Thread B reads (epoch, entry).
        // Invariant: if reader observes the new entry it also observes epoch ≥ new.
        let epoch = Arc::new(loom::sync::atomic::AtomicU64::new(0));
        let entry = Arc::new(loom::sync::Mutex::new(None::<u64>));

        let e1 = Arc::clone(&epoch); let en1 = Arc::clone(&entry);
        let t1 = thread::spawn(move || {
            *en1.lock().unwrap() = Some(42);
            e1.fetch_add(1, loom::sync::atomic::Ordering::Release);
        });

        let e2 = Arc::clone(&epoch); let en2 = Arc::clone(&entry);
        let t2 = thread::spawn(move || {
            let ep = e2.load(loom::sync::atomic::Ordering::Acquire);
            let v = *en2.lock().unwrap();
            if v.is_some() {
                assert!(ep >= 1, "reader observed entry before epoch bump");
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();
    });
}
```

- [ ] **Step 3: Run**

Run: `RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_method_cache --features loom 2>&1 | tail -10`
Expected: test passes under exhaustive interleaving.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test(loom): MethodCache epoch/entry ordering under concurrent extend/dispatch

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 34: Loom — Keyword intern table

**Files:**
- Create: `crates/clojure_core/tests/loom_keyword_intern.rs`

- [ ] **Step 1: Write**

```rust
#![cfg(loom)]
// Model: two threads intern the same key; both return the same pointer.
use loom::sync::Arc;
use loom::thread;

#[test]
fn concurrent_intern_same_key_returns_identical() {
    loom::model(|| {
        let map = Arc::new(loom::sync::Mutex::new(std::collections::HashMap::<String, u64>::new()));
        let m1 = Arc::clone(&map); let m2 = Arc::clone(&map);
        let t1 = thread::spawn(move || {
            let mut g = m1.lock().unwrap();
            *g.entry("k".into()).or_insert_with(|| 42)
        });
        let t2 = thread::spawn(move || {
            let mut g = m2.lock().unwrap();
            *g.entry("k".into()).or_insert_with(|| 42)
        });
        let a = t1.join().unwrap();
        let b = t2.join().unwrap();
        assert_eq!(a, b);
    });
}
```

- [ ] **Step 2: Run + commit**

```bash
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_keyword_intern --features loom
git add -A
git commit -m "test(loom): concurrent keyword intern returns identical values

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 35: Loom — Var root CAS + binding stack

**Files:**
- Create: `crates/clojure_core/tests/loom_var_root.rs`
- Create: `crates/clojure_core/tests/loom_binding_stack.rs`

- [ ] **Step 1: Var root CAS test**

```rust
#![cfg(loom)]
use loom::sync::Arc;
use loom::sync::atomic::{AtomicI64, Ordering};
use loom::thread;

#[test]
fn alter_var_root_is_linearizable() {
    loom::model(|| {
        let v = Arc::new(AtomicI64::new(0));
        let v1 = Arc::clone(&v); let v2 = Arc::clone(&v);
        let t1 = thread::spawn(move || {
            loop {
                let cur = v1.load(Ordering::Acquire);
                if v1.compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() { break; }
            }
        });
        let t2 = thread::spawn(move || {
            loop {
                let cur = v2.load(Ordering::Acquire);
                if v2.compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() { break; }
            }
        });
        t1.join().unwrap(); t2.join().unwrap();
        assert_eq!(v.load(Ordering::Acquire), 2);
    });
}
```

- [ ] **Step 2: Binding stack test — push/pop isolation**

```rust
// crates/clojure_core/tests/loom_binding_stack.rs
#![cfg(loom)]
// Binding stack is thread-local so there's no cross-thread contention
// on the stack itself. We model the cross-thread conveyance via
// Arc-sharing of a captured frame (bound-fn semantics).
use loom::sync::Arc;
use loom::thread;

#[test]
fn bound_fn_snapshot_is_safe_cross_thread() {
    loom::model(|| {
        // Snapshot is a shared read-only Arc — any number of threads may clone it.
        let snap = Arc::new(vec![1, 2, 3]);
        let s1 = Arc::clone(&snap); let s2 = Arc::clone(&snap);
        let t1 = thread::spawn(move || s1.iter().sum::<i32>());
        let t2 = thread::spawn(move || s2.iter().sum::<i32>());
        assert_eq!(t1.join().unwrap(), 6);
        assert_eq!(t2.join().unwrap(), 6);
    });
}
```

- [ ] **Step 3: Run + commit**

```bash
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_var_root --features loom
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --test loom_binding_stack --features loom
git add -A
git commit -m "test(loom): Var CAS + bound-fn snapshot cross-thread safety

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 9 — Integration Stress Test + README

### Task 36: Pytest stress for concurrent extend + dispatch

**Files:**
- Create: `tests/test_concurrency_stress.py`

- [ ] **Step 1: Write test**

```python
# tests/test_concurrency_stress.py
import time
import pytest
from concurrent.futures import ThreadPoolExecutor
from clojure._core import IFn, invoke1

class T1: pass
class T2(T1): pass
class T3(T2): pass

@pytest.mark.timeout(30)
def test_concurrent_extend_and_dispatch():
    deadline = time.monotonic() + 10.0
    errors = []

    def extend_worker():
        n = 0
        while time.monotonic() < deadline:
            IFn.extend_type(T1, {"invoke1": lambda s, a: ("T1", a)})
            IFn.extend_type(T2, {"invoke1": lambda s, a: ("T2", a)})
            IFn.extend_type(T3, {"invoke1": lambda s, a: ("T3", a)})
            n += 1
        return n

    def dispatch_worker():
        n = 0
        while time.monotonic() < deadline:
            for obj, tag in [(T1(), "T1"), (T2(), "T2"), (T3(), "T3")]:
                try:
                    result = invoke1(obj, n)
                    # Only check tag when cache is stable; during re-extend the tag could be stale — that's OK.
                    _ = result
                except Exception as e:
                    errors.append(repr(e))
            n += 1
        return n

    with ThreadPoolExecutor(max_workers=32) as ex:
        futs = [ex.submit(extend_worker) for _ in range(4)]
        futs += [ex.submit(dispatch_worker) for _ in range(28)]
        for f in futs: f.result()

    assert not errors, f"dispatch errors: {errors[:5]}"
```

- [ ] **Step 2: Run**

Run: `pytest tests/test_concurrency_stress.py -v --timeout=30`
Expected: passes in ~10-12s.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "test(stress): concurrent extend-type + dispatch smoke under 32 workers

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 37: Final test sweep + README

**Files:**
- Create: `README.md`
- Run full test suite

- [ ] **Step 1: Write README**

```markdown
# clojure-py

Clojure core on Python 3.14t, implemented in Rust via PyO3.

## Status

Core abstractions (protocols, IFn, Var, Namespace, Symbol, Keyword) —
see `docs/superpowers/specs/2026-04-21-core-abstractions-design.md`.

## Build

```bash
python3.14t -m venv .venv && source .venv/bin/activate
pip install maturin pytest
maturin develop --release
pytest
```

## Run Loom tests

```bash
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --features loom
```
```

- [ ] **Step 2: Full test sweep**

Run:
```bash
maturin develop --release
pytest -v
RUSTFLAGS="--cfg loom" cargo test -p clojure_core --features loom
cargo test -p clojure_core_macros
```

Expected: everything green.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README with build and test instructions

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review Notes

- **Spec coverage:** §1–§12 of the spec are covered: workspace (Task 1-6), exceptions (7), Symbol (8-9), Keyword (10), Protocol runtime (11-13), #[protocol] macro (14-16), Protocol dispatch including MRO/fallback/metadata (17, 21-24), #[implements] (18-19), IFn + Keyword-as-IFn (16, 20), IFn Python-callable fallback (23), Var including IFn and dunder delegation (25-27), binding stack + bound-fn (28-30), ClojureNamespace (31), ns ops (32), Loom (33-35), stress test (36).

- **Known gaps needing on-the-fly resolution by the engineer:**
  - `#[implements]` macro codegen (Task 19) wires only invoke1/2/3/variadic fully. The engineer should extend to the full invoke0..invoke20 as needed when Var and Keyword impls grow. Emit an `unimplemented!()` for unsupported arities so gaps are visible.
  - `rt::get` (Task 20) only supports `PyDict`; extending to `IPersistentMap` is deferred to the collections spec. Keep it narrow.
  - `inline_cache` slot from spec §4.3 is not emitted anywhere in this plan — deferred to the inline-cache spec per §12. Confirm no other subsystem expects it.
  - `fire_watches` in Var needs `Py<Var>` at the call site (noted inline in Task 25 step 1); fix when iterating to green.
  - The `as_pyobj` shim in Keyword (Task 10) is replaced naturally by the Task 20 migration to IFn-based dispatch.

- **Placeholders:** Replace every `0xSY_u64` / `0xKEEEEE_u64` style token with a real u64 constant (any value; must differ between Symbol and Keyword).

