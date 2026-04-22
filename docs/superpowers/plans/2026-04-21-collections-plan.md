# Persistent Collections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the main-line Clojure persistent collections — `PersistentList`, `PersistentVector` (HAMT), `PersistentArrayMap`, `PersistentHashMap` (HAMT), `PersistentHashSet` — plus seq types (`Cons`, `LazySeq`, `ChunkedCons`, `IteratorSeq`) and transient variants. All equality/hashing/iteration routed through protocols.

**Architecture:** Hand-rolled 32-way bit-partitioned HAMTs ported from Clojure-JVM. Protocol-routed `rt::equiv`/`rt::hash_eq` at every leaf. Structural sharing via `Arc` on internal nodes. Transient variants with `alive: AtomicBool` + owner-thread safety. Fuzzing via Python `hypothesis` and Rust `proptest`.

**Tech Stack:** Same as core-abstractions — pyo3 0.28, maturin, Rust 1.85, CPython 3.14t. Adds `hypothesis` (Python), `proptest` (Rust).

**Spec:** `docs/superpowers/specs/2026-04-21-collections-design.md`

---

## Accumulated pyo3 0.28 idioms (from core-abstractions)

Apply throughout:

1. **`PyObject` alias.** Add `type PyObject = Py<PyAny>;` at the top of any Rust file that needs it (pyo3 0.28 removed the crate-root re-export).
2. **`Bound<'_, PyAny>` → `Py<PyAny>`** via `.unbind()`, not `.into()` (ambiguous in 0.28).
3. **`Py<T>.clone()`** doesn't exist; use `.clone_ref(py)`.
4. **`.downcast::<T>()`** is deprecated but works; consistent with in-tree code.
5. **`#[pyclass(frozen)]`** required when `Bound::<T>::get()` is used (pyo3 0.28 needs Frozen = True for `.get()` on Bound).
6. **`PyTuple::new(py, ...)`** is fallible: `PyTuple::new(py, &args)?`.
7. **`PyCFunction::new_closure(py, None, None, ...)`** — pass `None` for name (pyo3 0.28's `&'static CStr` requirement doesn't admit temporaries).
8. **Workspace already has `multiple-pymethods` pyo3 feature.** OK to split `#[pymethods] impl X` blocks by concern.
9. **Call site for `#[pymethods]` methods from Rust.** They default to module-private. For macro-generated code that calls them, mark `pub fn` explicitly (we did this for `Protocol::extend_type`, `Protocol::set_fallback`, `Var::new`).
10. **Macro-generated paths use `crate::`** (external consumers not supported yet — documented limitation).

## NEW protocol-method signature convention (applies throughout this spec)

Starting with this spec, every protocol trait method's first parameter is **`this: Py<Self>`**, not `&self`. This gives trait bodies access to the full Python handle so they can produce self-by-reference results (`ISeq::cons` putting self as tail of a new list, `ISeqable::seq` returning self) without extra allocations or TLS hacks.

Inside a trait method body:
- Need read-only `&Self`? `let s: &Self = this.bind(py).get();`
- Need to pass self by handle? use `this` directly (or `this.clone_ref(py)` if you'll use it again).
- Need to convert to `Py<PyAny>`? `this.into_any()` or `this.clone_ref(py).into_any()`.

Phase 0 migrates existing IFn + impls (Keyword, Var) + the `#[implements]` macro to this convention. All subsequent phases assume it.

---

## File Structure (from §2.1 of spec)

```
crates/clojure_core/src/
  # Protocol declarations (one per file)
  iseq.rs, iseqable.rs, counted.rs
  iequiv.rs, ihasheq.rs, imeta.rs
  sequential.rs, indexed.rs, associative.rs
  ipersistent_collection.rs, ipersistent_list.rs, ipersistent_vector.rs,
  ipersistent_map.rs, ipersistent_set.rs, ipersistent_stack.rs
  ieditable_collection.rs
  itransient_collection.rs, itransient_associative.rs,
  itransient_vector.rs, itransient_map.rs, itransient_set.rs

  # Collections
  collections/
    mod.rs
    plist.rs                     # PersistentList + EmptyList
    pvector.rs                   # PersistentVector (HAMT + tail) + TransientVector
    pvector_node.rs              # VNode
    parraymap.rs                 # PersistentArrayMap + TransientArrayMap
    phashmap.rs                  # PersistentHashMap + TransientHashMap
    phashmap_node.rs             # MNode, BitmapIndexedNode, ArrayNode, HashCollisionNode
    phashset.rs                  # PersistentHashSet + TransientHashSet
    map_entry.rs                 # MapEntry

  # Seq types
  seqs/
    mod.rs
    cons.rs, lazy_seq.rs, chunked_cons.rs, iterator_seq.rs

  # Extended rt
  rt.rs                          # EXTENDED: equiv, hash_eq, seq, first, next, rest,
                                 # count, conj, assoc, dissoc, nth, contains, empty,
                                 # transient, persistent_bang, conj_bang, assoc_bang,
                                 # dissoc_bang, disj_bang, pop_bang

  # Renamed
  binding_pmap.rs                # RENAMED from pmap.rs
```

Test files:
- `tests/test_plist.py`, `test_pvector.py`, `test_parraymap.py`, `test_phashmap.py`, `test_phashset.py`
- `tests/test_seqs.py`
- `tests/test_transients.py`
- `tests/test_iseq.py`, `test_iequiv.py`, `test_ihasheq.py`, `test_imeta.py`
- `tests/test_collections_fuzz.py`
- `tests/test_collections_stress.py`
- `crates/clojure_core/tests/proptest_hamt.rs`
- `crates/clojure_core/tests/loom_transient_edit.rs`

---

## Commit conventions

All commits use Conventional Commits, end with:

```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Phase 0 — Protocol Method Signature Migration

Change all protocol trait methods' first parameter from `&self` to `this: Py<Self>`. Update the `#[implements]` macro codegen to pass `Py<Self>` instead of downcasting to `&Self`. Migrate existing IFn impls on Keyword and Var.

This is a purely internal refactor — all 143 existing tests must still pass after it lands.

### Task 0.1: Update `#[implements]` macro codegen

**Files:**
- Modify: `crates/clojure_core_macros/src/implements.rs`

- [ ] **Step 1: Locate the wrapper-closure generator in `expand()`**

In `implements.rs`, the current codegen emits (for fixed-arity methods):

```rust
|args: &Bound<'_, PyTuple>, _kw| -> PyResult<Py<PyAny>> {
    let py = args.py();
    let self_any = args.get_item(0)?;
    let self_bound = self_any.downcast::<#self_ty>()?;
    let this: &#self_ty = self_bound.get();
    #(#arg_extractions)*
    <#self_ty as #proto_ident>::#ident(this, py #(, #arg_idents)*)
}
```

Change to:

```rust
|args: &Bound<'_, PyTuple>, _kw| -> PyResult<Py<PyAny>> {
    let py = args.py();
    let self_any = args.get_item(0)?;
    let self_bound = self_any.downcast::<#self_ty>()?;
    let this: ::pyo3::Py<#self_ty> = self_bound.clone().unbind();
    #(#arg_extractions)*
    <#self_ty as #proto_ident>::#ident(this, py #(, #arg_idents)*)
}
```

Same change in the variadic branch.

- [ ] **Step 2: Commit**

```bash
git add crates/clojure_core_macros/
git commit -m "refactor(macros): #[implements] passes Py<Self> instead of &Self

Trait method bodies now receive the full Python handle and can produce
self-referential results (e.g., seq returning self, cons putting self
as tail). Inside a method body, get &Self via this.bind(py).get().

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 0.2: Migrate IFn trait declaration

**Files:**
- Modify: `crates/clojure_core/src/ifn.rs`

- [ ] **Step 1: Rewrite the IFn trait declaration**

Replace every `&self` with `this: Py<Self>` across all 22 invoke arities + invoke_variadic. Keep all other arg types identical. The full trait block has 22 method signatures identical in shape:

```rust
#[protocol(name = "clojure.core/IFn", extend_via_metadata = false)]
pub trait IFn {
    fn invoke0(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn invoke1(this: Py<Self>, py: Python<'_>, a0: PyObject) -> PyResult<PyObject>;
    // ... through invoke20 ...
    fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject>;
}
```

### Task 0.3: Migrate Keyword's IFn impl

**Files:**
- Modify: `crates/clojure_core/src/keyword.rs`

- [ ] **Step 1: Rewrite every invoke method signature**

`fn invokeN(&self, py, args...)` becomes `fn invokeN(this: Py<Self>, py, args...)`. For methods that previously used `self.<x>`, prepend `let self_ref: &Keyword = this.bind(py).get();` and rename `self.` → `self_ref.`. For arity-stub methods that only raise ArityException, just rename the parameter (unused).

For `invoke_variadic`, recursive calls to `Self::invokeN` pass `this` (which we own) or `this.clone_ref(py)` (if we'll re-use). Example:

```rust
fn invoke_variadic(this: Py<Self>, py: Python<'_>, args: Bound<'_, pyo3::types::PyTuple>) -> PyResult<PyObject> {
    match args.len() {
        1 => {
            let coll = args.get_item(0)?.unbind();
            Self::invoke1(this, py, coll)
        }
        2 => {
            let coll = args.get_item(0)?.unbind();
            let default = args.get_item(1)?.unbind();
            Self::invoke2(this, py, coll, default)
        }
        n => Err(crate::exceptions::ArityException::new_err(format!(
            "Wrong number of args ({n}) passed to: Keyword"
        ))),
    }
}
```

### Task 0.4: Migrate Var's IFn impl

**Files:**
- Modify: `crates/clojure_core/src/var.rs`

- [ ] **Step 1: Rewrite each invoke method**

Each `invokeN(&self, py, a0, ..., aN-1)` becomes:

```rust
fn invokeN(this: Py<Self>, py: Python<'_>, a0: PyObject, ..., aN-1: PyObject) -> PyResult<PyObject> {
    let self_ref: &Var = this.bind(py).get();
    let root = self_ref.deref_raw(py)?;
    crate::rt::invoke_n(py, root, &[a0, ..., aN-1])
}
```

`invoke_variadic` follows the same pattern, using `args.iter()` to build the arg slice.

### Task 0.5: Verify + commit Phase 0

- [ ] **Step 1: Full rebuild + test**

```bash
cd /home/tbaldrid/oss/clojure-py
cargo check --workspace 2>&1 | tail -5
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/ -q 2>&1 | tail -3
```
Expected: 143 tests pass — this is a pure internal refactor.

- [ ] **Step 2: Commit Tasks 0.2 + 0.3 + 0.4 as one atomic migration**

```bash
git add -A
git commit -m "refactor(protocols): IFn + Keyword/Var impls use this: Py<Self>

Protocol method convention shift — first argument is now Py<Self>
instead of &self. Trait bodies get &Self via this.bind(py).get() when
needed; otherwise pass this along as a Py handle. Enables the
self-referential impls that upcoming collection types need (ISeq::cons
putting self as tail, ISeqable::seq returning self).

No behavior change; all 143 existing tests still pass.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 1 — Protocol Declarations

Declare all new protocols via `#[protocol]`. They need to exist before any collection `#[implements]` references them. Each file is a single trait declaration; the macro handles registration.

**All new protocols use the `this: Py<Self>` convention from the outset** (as spec'd in Phase 0).

### Task 1: Seq protocols

**Files:**
- Create: `crates/clojure_core/src/iseq.rs`
- Create: `crates/clojure_core/src/iseqable.rs`
- Create: `crates/clojure_core/src/counted.rs`
- Modify: `crates/clojure_core/src/lib.rs` — add `mod iseq; mod iseqable; mod counted;`

- [ ] **Step 1: Write `iseq.rs`**

```rust
//! ISeq — Clojure's seq abstraction: a value with a first element and a rest-seq.

use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ISeq", extend_via_metadata = false)]
pub trait ISeq {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 2: Write `iseqable.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ISeqable", extend_via_metadata = false)]
pub trait ISeqable {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
```

- [ ] **Step 3: Write `counted.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core/Counted", extend_via_metadata = false)]
pub trait Counted {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
}
```

- [ ] **Step 4: Wire into `lib.rs`**

Add the three `mod` declarations alongside other protocol modules. No `pub use` needed — the macro registers the protocol objects on the Python module via inventory. Order within lib.rs doesn't matter; keep it alphabetical for readability.

- [ ] **Step 5: Verify**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
python -c "from clojure._core import ISeq, ISeqable, Counted, first, seq, count; print(ISeq.name, ISeqable.name, Counted.name)"
pytest tests/ -q 2>&1 | tail -3
```
Expected: clean build; Python print shows `clojure.core/ISeq clojure.core/ISeqable clojure.core/Counted`; 143 existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(protocols): ISeq, ISeqable, Counted

Seq-layer protocols declared via #[protocol]. No impls yet — collection
types in later phases will #[implements] them.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 2: Equality, hash, meta protocols

**Files:**
- Create: `crates/clojure_core/src/iequiv.rs`
- Create: `crates/clojure_core/src/ihasheq.rs`
- Create: `crates/clojure_core/src/imeta.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: `iequiv.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IEquiv", extend_via_metadata = false)]
pub trait IEquiv {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}
```

- [ ] **Step 2: `ihasheq.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core/IHashEq", extend_via_metadata = false)]
pub trait IHashEq {
    fn hash_eq(this: Py<Self>, py: Python<'_>) -> PyResult<i64>;
}
```

- [ ] **Step 3: `imeta.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IMeta", extend_via_metadata = false)]
pub trait IMeta {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 4: Wire into `lib.rs`**

Add `mod iequiv; mod ihasheq; mod imeta;`.

- [ ] **Step 5: Verify**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
python -c "from clojure._core import IEquiv, IHashEq, IMeta; print(IEquiv, IHashEq, IMeta)"
pytest tests/ -q 2>&1 | tail -3
```
Expected: three Protocol reprs printed; 143 tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(protocols): IEquiv, IHashEq, IMeta

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3: Access + marker protocols

**Files:**
- Create: `crates/clojure_core/src/sequential.rs`
- Create: `crates/clojure_core/src/indexed.rs`
- Create: `crates/clojure_core/src/associative.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: `sequential.rs`** (marker — no methods)

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core/Sequential", extend_via_metadata = false)]
pub trait Sequential {}
```

- [ ] **Step 2: `indexed.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Indexed", extend_via_metadata = false)]
pub trait Indexed {
    fn nth(this: Py<Self>, py: Python<'_>, i: usize) -> PyResult<PyObject>;
    fn nth_or_default(this: Py<Self>, py: Python<'_>, i: usize, default: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 3: `associative.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/Associative", extend_via_metadata = false)]
pub trait Associative {
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 4: Wire into `lib.rs`**

Add `mod sequential; mod indexed; mod associative;`.

- [ ] **Step 5: Verify + commit**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/ -q 2>&1 | tail -1
git add -A && git commit -m "feat(protocols): Sequential, Indexed, Associative

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 4: Persistent collection protocols

**Files:**
- Create: `crates/clojure_core/src/ipersistent_collection.rs`
- Create: `crates/clojure_core/src/ipersistent_list.rs`
- Create: `crates/clojure_core/src/ipersistent_vector.rs`
- Create: `crates/clojure_core/src/ipersistent_map.rs`
- Create: `crates/clojure_core/src/ipersistent_set.rs`
- Create: `crates/clojure_core/src/ipersistent_stack.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: `ipersistent_collection.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentCollection", extend_via_metadata = false)]
pub trait IPersistentCollection {
    fn count(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
    fn empty(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool>;
}
```

- [ ] **Step 2: `ipersistent_list.rs`** (marker)

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;

#[protocol(name = "clojure.core/IPersistentList", extend_via_metadata = false)]
pub trait IPersistentList {}
```

- [ ] **Step 3: `ipersistent_vector.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentVector", extend_via_metadata = false)]
pub trait IPersistentVector {
    fn length(this: Py<Self>, py: Python<'_>) -> PyResult<usize>;
    fn assoc_n(this: Py<Self>, py: Python<'_>, i: usize, x: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 4: `ipersistent_map.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentMap", extend_via_metadata = false)]
pub trait IPersistentMap {
    fn assoc(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
    fn without(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_key(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn entry_at(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 5: `ipersistent_set.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentSet", extend_via_metadata = false)]
pub trait IPersistentSet {
    fn disjoin(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 6: `ipersistent_stack.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IPersistentStack", extend_via_metadata = false)]
pub trait IPersistentStack {
    fn peek(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
    fn pop(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
```

- [ ] **Step 7: Wire into `lib.rs`**

Add the six `mod` declarations.

- [ ] **Step 8: Verify + commit**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/ -q 2>&1 | tail -1
git add -A && git commit -m "feat(protocols): IPersistentCollection/List/Vector/Map/Set/Stack

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5: Transient protocols

**Files:**
- Create: `crates/clojure_core/src/ieditable_collection.rs`
- Create: `crates/clojure_core/src/itransient_collection.rs`
- Create: `crates/clojure_core/src/itransient_associative.rs`
- Create: `crates/clojure_core/src/itransient_vector.rs`
- Create: `crates/clojure_core/src/itransient_map.rs`
- Create: `crates/clojure_core/src/itransient_set.rs`
- Modify: `crates/clojure_core/src/lib.rs`

- [ ] **Step 1: `ieditable_collection.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/IEditableCollection", extend_via_metadata = false)]
pub trait IEditableCollection {
    fn as_transient(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
```

- [ ] **Step 2: `itransient_collection.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientCollection", extend_via_metadata = false)]
pub trait ITransientCollection {
    fn conj_bang(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject>;
    fn persistent_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
```

- [ ] **Step 3: `itransient_associative.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientAssociative", extend_via_metadata = false)]
pub trait ITransientAssociative {
    fn assoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject, v: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 4: `itransient_vector.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientVector", extend_via_metadata = false)]
pub trait ITransientVector {
    fn pop_bang(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject>;
}
```

- [ ] **Step 5: `itransient_map.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientMap", extend_via_metadata = false)]
pub trait ITransientMap {
    fn dissoc_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 6: `itransient_set.rs`**

```rust
use clojure_core_macros::protocol;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

#[protocol(name = "clojure.core/ITransientSet", extend_via_metadata = false)]
pub trait ITransientSet {
    fn disj_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
    fn contains_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<bool>;
    fn get_bang(this: Py<Self>, py: Python<'_>, k: PyObject) -> PyResult<PyObject>;
}
```

- [ ] **Step 7: Wire into `lib.rs`**

Add the six `mod` declarations.

- [ ] **Step 8: Verify + commit**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
python -c "from clojure._core import IEditableCollection, ITransientCollection, ITransientAssociative, ITransientVector, ITransientMap, ITransientSet; print('ok')"
pytest tests/ -q 2>&1 | tail -1
git add -A && git commit -m "feat(protocols): transient protocols — IEditableCollection + ITransient*

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


## Phase 2 — Built-in Fallbacks for IEquiv + IHashEq

Install module-init fallbacks so `rt::equiv` and `rt::hash_eq` work on Python built-ins (ints, strings, etc.) before any collection type implements these protocols.

### Task 6: IEquiv + IHashEq fallbacks

**Files:**
- Modify: `crates/clojure_core/src/iequiv.rs` — add `install_builtin_fallback`
- Modify: `crates/clojure_core/src/ihasheq.rs` — add `install_builtin_fallback`
- Modify: `crates/clojure_core/src/lib.rs` — call both fallbacks in init, after `install_all_extends`
- Create: `tests/test_iequiv_ihasheq.py`

- [ ] **Step 1: Failing test**

```python
# tests/test_iequiv_ihasheq.py
from clojure._core import IEquiv, IHashEq, Protocol, ProtocolMethod
import pytest


def test_iequiv_is_registered():
    assert isinstance(IEquiv, Protocol)


def test_ihasheq_is_registered():
    assert isinstance(IHashEq, Protocol)


def test_equiv_via_rt_on_ints():
    """Fallback should dispatch to Python ==. Accessed through the protocol method."""
    from clojure._core import equiv
    assert equiv(1, 1) is True
    assert equiv(1, 2) is False
    assert equiv("a", "a") is True
    assert equiv("a", "b") is False


def test_hash_eq_via_rt_on_ints():
    from clojure._core import hash_eq
    assert hash_eq(42) == hash(42)
    assert hash_eq("hello") == hash("hello")


def test_equiv_nil_and_booleans():
    from clojure._core import equiv
    assert equiv(None, None) is True
    assert equiv(True, True) is True
    assert equiv(False, False) is True
    assert equiv(None, False) is False  # Python: None != False
```

- [ ] **Step 2: Run — expect failure** (protocols registered but `equiv`/`hash_eq` ProtocolMethods raise `IllegalArgumentException` on cache miss)

Run: `source .venv/bin/activate && pytest tests/test_iequiv_ihasheq.py -v`
Expected: the registration tests pass, the `equiv`/`hash_eq` tests fail with `IllegalArgumentException: No implementation...`

- [ ] **Step 3: `iequiv.rs` — add install_builtin_fallback**

Append to `iequiv.rs`:

```rust
use pyo3::types::{PyCFunction, PyDict, PyTuple};

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let iequiv_any = m.getattr("IEquiv")?;
    let iequiv_proto: &Bound<'_, crate::Protocol> = iequiv_any.downcast()?;

    let fallback = PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = args.py();
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.downcast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            // Register a generic `equiv(self, other)` wrapper for this PyType:
            // implementation is `self == other` via Python equality.
            let eq_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    let other = inner.get_item(1)?;
                    let eq_result = this.eq(other)?;
                    Ok(pyo3::types::PyBool::new(py, eq_result).to_owned().unbind().into_any())
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("equiv", &eq_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    iequiv_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
```

- [ ] **Step 4: `ihasheq.rs` — add install_builtin_fallback**

Append to `ihasheq.rs`:

```rust
use pyo3::types::{PyCFunction, PyDict, PyTuple};

pub(crate) fn install_builtin_fallback(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ihasheq_any = m.getattr("IHashEq")?;
    let ihasheq_proto: &Bound<'_, crate::Protocol> = ihasheq_any.downcast()?;

    let fallback = PyCFunction::new_closure(
        py,
        None,
        None,
        |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
            let py = args.py();
            let proto_any = args.get_item(0)?;
            let proto: &Bound<'_, crate::Protocol> = proto_any.downcast()?;
            let _method_key: String = args.get_item(1)?.extract()?;
            let target = args.get_item(2)?;

            let hash_wrapper = PyCFunction::new_closure(
                py,
                None,
                None,
                |inner: &Bound<'_, PyTuple>, _: Option<&Bound<'_, PyDict>>| -> PyResult<Py<PyAny>> {
                    let py = inner.py();
                    let this = inner.get_item(0)?;
                    let h: isize = this.hash()?;
                    Ok((h as i64).into_pyobject(py)?.unbind().into_any())
                },
            )?;

            let impls = PyDict::new(py);
            impls.set_item("hash_eq", &hash_wrapper)?;
            let ty = target.get_type();
            proto.get().extend_type(py, ty, impls)?;

            Ok(py.None())
        },
    )?;

    ihasheq_proto.call_method1("set_fallback", (fallback.unbind().into_any(),))?;
    Ok(())
}
```

- [ ] **Step 5: Wire into `lib.rs`**

In the pymodule init body, **after** `registry::install_all_extends(py, m)?;` and **before** `ifn::install_builtin_fallback(py, m)?;`, add:

```rust
iequiv::install_builtin_fallback(py, m)?;
ihasheq::install_builtin_fallback(py, m)?;
```

- [ ] **Step 6: Run — expect pass**

```bash
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/test_iequiv_ihasheq.py -v
pytest tests/ -q 2>&1 | tail -3
```
Expected: all 7 new tests pass; full suite 150 passed.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(protocols): IEquiv + IHashEq built-in Python fallbacks

Installs module-init fallbacks on both protocols so rt::equiv and
rt::hash_eq work on Python built-ins (ints, strings, bytes, etc.)
via Python == and hash(). Our own types will override these with
structural impls in later tasks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 3 — rt:: Helper Extensions

Add rt helpers that dispatch through the new protocols. Each is cached in a `OnceCell<Py<Protocol>>` populated by `rt::init`.

### Task 7: rt::equiv, rt::hash_eq

**Files:**
- Modify: `crates/clojure_core/src/rt.rs`

- [ ] **Step 1: Add cached protocol handles**

At the top of `rt.rs`, alongside existing `ILOOKUP_PROTO` / `IFN_PROTO`:

```rust
static IEQUIV_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static IHASHEQ_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();

static EQUIV_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("equiv"));
static HASH_EQ_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("hash_eq"));
```

- [ ] **Step 2: Extend `rt::init` to cache them**

```rust
pub(crate) fn init(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let ilookup = m.getattr("ILookup")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = ILOOKUP_PROTO.set(ilookup);

    let ifn = m.getattr("IFn")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IFN_PROTO.set(ifn);

    let iequiv = m.getattr("IEquiv")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IEQUIV_PROTO.set(iequiv);

    let ihasheq = m.getattr("IHashEq")?.downcast::<crate::Protocol>()?.clone().unbind();
    let _ = IHASHEQ_PROTO.set(ihasheq);

    let _ = py;
    Ok(())
}
```

- [ ] **Step 3: Add rt::equiv and rt::hash_eq**

Below `rt::invoke_n`, add:

```rust
pub fn equiv(py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<bool> {
    let proto = IEQUIV_PROTO
        .get()
        .expect("rt::equiv called before rt::init");
    let args = PyTuple::new(py, &[b])?;
    let result: Py<PyAny> = crate::dispatch::dispatch(py, proto, &EQUIV_KEY, a, args)?;
    result.bind(py).extract::<bool>()
}

pub fn hash_eq(py: Python<'_>, x: PyObject) -> PyResult<i64> {
    let proto = IHASHEQ_PROTO
        .get()
        .expect("rt::hash_eq called before rt::init");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    let result: Py<PyAny> = crate::dispatch::dispatch(py, proto, &HASH_EQ_KEY, x, args)?;
    result.bind(py).extract::<i64>()
}
```

- [ ] **Step 4: Verify**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/ -q 2>&1 | tail -3
```
Expected: clean build; 150 tests still pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(rt): equiv + hash_eq helpers routed through IEquiv / IHashEq

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 8: rt::seq, rt::first, rt::next, rt::rest, rt::count, rt::empty

**Files:**
- Modify: `crates/clojure_core/src/rt.rs`

- [ ] **Step 1: Add cached protocol handles and keys**

Add alongside existing:

```rust
static ISEQ_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static ISEQABLE_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static COUNTED_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();
static IPC_PROTO: OnceCell<Py<crate::Protocol>> = OnceCell::new();  // IPersistentCollection

static SEQ_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("seq"));
static FIRST_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("first"));
static NEXT_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("next"));
static MORE_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("more"));
static COUNT_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("count"));
static EMPTY_KEY: Lazy<Arc<str>> = Lazy::new(|| Arc::from("empty"));
```

Extend `rt::init` to populate each (identical pattern: `m.getattr("ISeq")?.downcast::<crate::Protocol>()?.clone().unbind()` etc.).

- [ ] **Step 2: Add the helpers**

```rust
pub fn seq(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    // nil → nil
    if coll.is_none(py) {
        return Ok(py.None());
    }
    let proto = ISEQABLE_PROTO.get().expect("rt not initialized");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &SEQ_KEY, coll, args)
}

pub fn first(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    // (first nil) = nil; (first coll) seqs first then asks ISeq.first
    let s = seq(py, coll)?;
    if s.is_none(py) { return Ok(py.None()); }
    let proto = ISEQ_PROTO.get().expect("rt not initialized");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &FIRST_KEY, s, args)
}

pub fn next_(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    let s = seq(py, coll)?;
    if s.is_none(py) { return Ok(py.None()); }
    let proto = ISEQ_PROTO.get().expect("rt not initialized");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &NEXT_KEY, s, args)
}

pub fn rest(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    // (rest nil) = (). Will need EMPTY_LIST — for now, if nil, return nil and
    // let callers handle. We'll refine when plist lands.
    let s = seq(py, coll.clone_ref(py))?;
    if s.is_none(py) { return Ok(py.None()); }
    let proto = ISEQ_PROTO.get().expect("rt not initialized");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &MORE_KEY, s, args)
}

pub fn count(py: Python<'_>, coll: PyObject) -> PyResult<usize> {
    if coll.is_none(py) { return Ok(0); }
    let proto = COUNTED_PROTO.get().expect("rt not initialized");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    let result: Py<PyAny> = crate::dispatch::dispatch(py, proto, &COUNT_KEY, coll, args)?;
    result.bind(py).extract::<usize>()
}

pub fn empty(py: Python<'_>, coll: PyObject) -> PyResult<PyObject> {
    if coll.is_none(py) { return Ok(py.None()); }
    let proto = IPC_PROTO.get().expect("rt not initialized");
    let args = PyTuple::new(py, &[] as &[PyObject])?;
    crate::dispatch::dispatch(py, proto, &EMPTY_KEY, coll, args)
}
```

Function name `next_` (trailing underscore) to avoid clashing with Rust's iterator `.next()`.

- [ ] **Step 3: Verify**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/ -q 2>&1 | tail -3
```
Expected: clean; 150 tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(rt): seq/first/next/rest/count/empty helpers via protocols

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4 — Rename binding pmap

Housekeeping before we introduce the real `pmap.rs` for the HAMT hash-map. The existing `pmap.rs` is the binding-frame internal map.

### Task 9: Rename pmap.rs → binding_pmap.rs

**Files:**
- Rename: `crates/clojure_core/src/pmap.rs` → `crates/clojure_core/src/binding_pmap.rs`
- Modify: `crates/clojure_core/src/lib.rs` — `pub(crate) mod pmap;` → `pub(crate) mod binding_pmap;`
- Modify: `crates/clojure_core/src/binding.rs` — `use crate::pmap::PMap;` → `use crate::binding_pmap::PMap;`
- Modify: `crates/clojure_core/src/bound_fn.rs` — same import update

- [ ] **Step 1: Perform the rename**

```bash
cd /home/tbaldrid/oss/clojure-py
git mv crates/clojure_core/src/pmap.rs crates/clojure_core/src/binding_pmap.rs
```

- [ ] **Step 2: Update the three references**

Run these edits:
- `crates/clojure_core/src/lib.rs`: replace `pub(crate) mod pmap;` with `pub(crate) mod binding_pmap;`.
- `crates/clojure_core/src/binding.rs`: replace `use crate::pmap::PMap;` with `use crate::binding_pmap::PMap;`.
- `crates/clojure_core/src/bound_fn.rs`: replace `use crate::pmap::PMap;` with `use crate::binding_pmap::PMap;`.

- [ ] **Step 3: Verify**

```bash
cargo check --workspace 2>&1 | tail -3
source .venv/bin/activate && maturin develop --release 2>&1 | tail -3
pytest tests/ -q 2>&1 | tail -3
```
Expected: clean; 150 tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: rename pmap.rs to binding_pmap.rs

Disambiguates from the upcoming pmap/phashmap collections.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


## Phase 5 — PersistentList + EmptyList

The simplest collection type. Cons-cell with O(1) count + a singleton empty list. No HAMT complexity; good first port.

### Task 10: PersistentList + EmptyList structs + collections module

**Files:**
- Create: `crates/clojure_core/src/collections/mod.rs`
- Create: `crates/clojure_core/src/collections/plist.rs`
- Modify: `crates/clojure_core/src/lib.rs` — add `mod collections;` and `pub use collections::{PersistentList, EmptyList};`
- Create: `tests/test_plist.py`

- [ ] **Step 1: `collections/mod.rs`**

```rust
//! Persistent collection types.

pub mod plist;

pub use plist::{EmptyList, PersistentList};

use pyo3::prelude::*;

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    plist::register(py, m)?;
    Ok(())
}
```

- [ ] **Step 2: Failing test**

```python
# tests/test_plist.py
import pytest
from clojure._core import PersistentList, EmptyList, list_


def test_empty_list_constructor_returns_singleton():
    e1 = list_()
    e2 = list_()
    assert isinstance(e1, EmptyList)
    assert e1 is e2


def test_list_of_one():
    lst = list_(1)
    assert isinstance(lst, PersistentList)
    assert lst.first == 1
    assert isinstance(lst.rest, EmptyList)


def test_list_of_three():
    lst = list_(1, 2, 3)
    assert lst.first == 1
    assert lst.rest.first == 2
    assert lst.rest.rest.first == 3
    assert isinstance(lst.rest.rest.rest, EmptyList)


def test_count():
    assert len(list_()) == 0
    assert len(list_(1)) == 1
    assert len(list_(1, 2, 3)) == 3


def test_iteration():
    assert list(list_()) == []
    assert list(list_(1, 2, 3)) == [1, 2, 3]


def test_equality():
    assert list_() == list_()
    assert list_(1, 2) == list_(1, 2)
    assert list_(1, 2) != list_(1, 2, 3)
    assert list_(1, 2) != list_(2, 1)


def test_hash_stable():
    assert hash(list_(1, 2)) == hash(list_(1, 2))


def test_bool_empty_false():
    assert not bool(list_())
    assert bool(list_(1))


def test_cons():
    from clojure._core import conj
    c = conj(list_(), 1)
    assert c.first == 1
    c2 = conj(c, 2)
    assert c2.first == 2
    assert c2.rest.first == 1
```

- [ ] **Step 3: Run, expect FAIL** (ImportError on PersistentList/EmptyList/list_)

Run: `source .venv/bin/activate && pytest tests/test_plist.py -v 2>&1 | tail -3`

- [ ] **Step 4: Write `crates/clojure_core/src/collections/plist.rs`**

```rust
//! PersistentList — cons-cell linked list. EmptyList is a singleton.

use crate::counted::Counted;
use crate::iequiv::IEquiv;
use crate::ihasheq::IHashEq;
use crate::imeta::IMeta;
use crate::ipersistent_collection::IPersistentCollection;
use crate::ipersistent_list::IPersistentList;
use crate::ipersistent_stack::IPersistentStack;
use crate::iseq::ISeq;
use crate::iseqable::ISeqable;
use crate::sequential::Sequential;
use crate::exceptions::IllegalStateException;
use clojure_core_macros::implements;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple};

type PyObject = Py<PyAny>;

// --- EmptyList ---

#[pyclass(module = "clojure._core", name = "EmptyList", frozen)]
pub struct EmptyList {
    meta: RwLock<Option<PyObject>>,
}

static EMPTY_LIST: OnceCell<Py<EmptyList>> = OnceCell::new();

pub fn empty_list(py: Python<'_>) -> Py<EmptyList> {
    EMPTY_LIST.get().expect("plist::init not called").clone_ref(py)
}

#[pymethods]
impl EmptyList {
    fn __len__(&self) -> usize { 0 }
    fn __bool__(&self) -> bool { false }
    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<EmptyListIter>> {
        let _ = slf;
        Py::new(py, EmptyListIter)
    }
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        // Empty list equals only another empty list (or an empty PersistentList).
        if other.downcast::<EmptyList>().is_ok() {
            return Ok(true);
        }
        if let Ok(pl) = other.downcast::<PersistentList>() {
            // An empty PersistentList shouldn't exist (constructor produces EmptyList),
            // but be defensive:
            return Ok(pl.get().count == 0);
        }
        let _ = py;
        Ok(false)
    }
    fn __hash__(&self) -> i64 {
        // Clojure-JVM empty-list hash is 1; we match.
        1
    }
    fn __repr__(&self) -> String { "()".to_string() }
    fn __str__(&self) -> String { "()".to_string() }

    #[getter] fn meta(&self, py: Python<'_>) -> PyObject {
        self.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None())
    }
}

#[pyclass(module = "clojure._core", name = "EmptyListIter")]
pub struct EmptyListIter;

#[pymethods]
impl EmptyListIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> { slf }
    fn __next__(&self) -> PyResult<PyObject> {
        Err(pyo3::exceptions::PyStopIteration::new_err(()))
    }
}

#[implements(ISeq)]
impl ISeq for EmptyList {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        // Returns itself — more-of-empty is empty.
        Ok(empty_list(py).into_any())
    }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        let new = PersistentList {
            head: x,
            tail: empty_list(py).into_any(),
            count: 1,
            meta: RwLock::new(None),
        };
        Ok(Py::new(py, new)?.into_any())
    }
}

#[implements(ISeqable)]
impl ISeqable for EmptyList {
    fn seq(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
}

#[implements(Counted)]
impl Counted for EmptyList {
    fn count(&self, _py: Python<'_>) -> PyResult<usize> { Ok(0) }
}

#[implements(IEquiv)]
impl IEquiv for EmptyList {
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        let b = other.bind(py);
        if b.downcast::<EmptyList>().is_ok() { return Ok(true); }
        if let Ok(pl) = b.downcast::<PersistentList>() { return Ok(pl.get().count == 0); }
        Ok(false)
    }
}

#[implements(IHashEq)]
impl IHashEq for EmptyList {
    fn hash_eq(&self, _py: Python<'_>) -> PyResult<i64> { Ok(1) }
}

#[implements(IMeta)]
impl IMeta for EmptyList {
    fn meta(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.meta.read().as_ref().map(|o| o.clone_ref(py)).unwrap_or_else(|| py.None()))
    }
    fn with_meta(this: Py<Self>, py: Python<'_>, meta: PyObject) -> PyResult<PyObject> {
        let m = if meta.is_none(py) { None } else { Some(meta) };
        let e = EmptyList { meta: RwLock::new(m) };
        Ok(Py::new(py, e)?.into_any())
    }
}

#[implements(IPersistentCollection)]
impl IPersistentCollection for EmptyList {
    fn count(&self, _py: Python<'_>) -> PyResult<usize> { Ok(0) }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        <EmptyList as ISeq>::cons(self, py, x)
    }
    fn empty(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        Ok(empty_list(py).into_any())
    }
    fn equiv(this: Py<Self>, py: Python<'_>, other: PyObject) -> PyResult<bool> {
        <EmptyList as IEquiv>::equiv(self, py, other)
    }
}

#[implements(IPersistentList)]
impl IPersistentList for EmptyList {}

#[implements(IPersistentStack)]
impl IPersistentStack for EmptyList {
    fn peek(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(py.None()) }
    fn pop(&self, _py: Python<'_>) -> PyResult<PyObject> {
        Err(IllegalStateException::new_err("Can't pop empty list"))
    }
}

#[implements(Sequential)]
impl Sequential for EmptyList {}

// --- PersistentList ---

#[pyclass(module = "clojure._core", name = "PersistentList", frozen)]
pub struct PersistentList {
    pub head: PyObject,
    pub tail: PyObject,  // another PersistentList or EmptyList
    pub count: u32,
    pub meta: RwLock<Option<PyObject>>,
}

#[pymethods]
impl PersistentList {
    #[getter] fn first(&self, py: Python<'_>) -> PyObject { self.head.clone_ref(py) }
    #[getter] fn rest(&self, py: Python<'_>) -> PyObject { self.tail.clone_ref(py) }

    fn __len__(&self) -> usize { self.count as usize }
    fn __bool__(&self) -> bool { true }

    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<Py<PersistentListIter>> {
        Py::new(py, PersistentListIter { current: slf.into_any() })
    }

    fn __eq__(slf: Py<Self>, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        crate::rt::equiv(py, slf.into_any(), other.clone().unbind())
    }

    fn __hash__(slf: Py<Self>, py: Python<'_>) -> PyResult<i64> {
        crate::rt::hash_eq(py, slf.into_any())
    }

    fn __repr__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        let this = slf.bind(py).get();
        let mut parts: Vec<String> = Vec::with_capacity(this.count as usize);
        let mut cur: PyObject = slf.clone_ref(py).into_any();
        loop {
            let b = cur.bind(py);
            if b.downcast::<EmptyList>().is_ok() { break; }
            if let Ok(pl) = b.downcast::<PersistentList>() {
                let r = pl.get().head.bind(py).repr()?.extract::<String>()?;
                parts.push(r);
                cur = pl.get().tail.clone_ref(py);
                continue;
            }
            break;
        }
        Ok(format!("({})", parts.join(" ")))
    }
    fn __str__(slf: Py<Self>, py: Python<'_>) -> PyResult<String> {
        Self::__repr__(slf, py)
    }
}

#[pyclass(module = "clojure._core", name = "PersistentListIter")]
pub struct PersistentListIter {
    current: PyObject,
}

#[pymethods]
impl PersistentListIter {
    fn __iter__(slf: Py<Self>) -> Py<Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let b = self.current.bind(py);
        if b.downcast::<EmptyList>().is_ok() {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        if let Ok(pl) = b.downcast::<PersistentList>() {
            let h = pl.get().head.clone_ref(py);
            self.current = pl.get().tail.clone_ref(py);
            return Ok(h);
        }
        Err(pyo3::exceptions::PyStopIteration::new_err(()))
    }
}

#[implements(ISeq)]
impl ISeq for PersistentList {
    fn first(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(self.head.clone_ref(py)) }
    fn next(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> {
        let b = self.tail.bind(py);
        if b.downcast::<EmptyList>().is_ok() { return Ok(py.None()); }
        Ok(self.tail.clone_ref(py))
    }
    fn more(this: Py<Self>, py: Python<'_>) -> PyResult<PyObject> { Ok(self.tail.clone_ref(py)) }
    fn cons(this: Py<Self>, py: Python<'_>, x: PyObject) -> PyResult<PyObject> {
        // We don't have `Py<Self>` here, so build a PersistentList whose tail references
        // *a fresh clone* of self. Because we can't get slf: Py<Self>, we reconstruct
        // the tail by wrapping `x` onto the seq: produce the new list with `self` as tail
        // via the lookup-from-intern pattern. Simpler: use IPersistentCollection.cons path.
        //
        // For IFn cons dispatch via rt, use rt::conj which wraps whatever target is.
        // Here we just allocate a new PersistentList whose tail is a *new* PyObject
        // referencing the same data. We cheat: call cons via Python-level.
        //
        // Cleanest: take `slf: Py<Self>` at the pymethods-path (see __iter__ pattern).
        // For the trait method, reconstruct by asking the caller to wrap — but here we
        // just need to produce a new list. Allocate from a freshly-cloned tail PyObject.
        //
        // Pragmatic: use unsafe { Py::from_borrowed_ptr } to re-handle self.
        let self_ptr = self as *const Self as *const pyo3::ffi::PyObject;
        // The above trick is unsafe and pyo3-version-dependent; instead we can take the
        // impl-side-only approach: require cons to be called via the `PersistentList_cons`
        // #[pymethods] helper that has Py<Self>. For now we just build with self.tail as
        // the tail — this is WRONG for building proper chains; the correct method is on
        // the pymethods side.
        //
        // Simplify: the IPersistentCollection.cons trait method is rarely called
        // directly (users go through rt::conj which dispatches through
        // IPersistentCollection.cons on the target). For rt::conj to work correctly, we
        // need the self pointer. Use the `inline_cons` approach: build via a helper.
        let tail = slf_ptr_to_pyobject(py, self);
        let new = PersistentList {
            head: x,
            tail,
            count: self.count + 1,
            meta: RwLock::new(None),
        };
        Ok(Py::new(py, new)?.into_any())
    }
}

// See implementer note in Task 10 on the `self`→`Py<Self>` gap. For this first port,
// use a helper that unsafely reconstructs the Py<PersistentList> from &Self:
fn slf_ptr_to_pyobject(py: Python<'_>, slf: &PersistentList) -> PyObject {
    // PyO3 lays out a pyclass as [PyObject header][padding][Rust struct]. Given
    // &Rust-struct, the PyObject* sits at a fixed negative offset — but that offset
    // is pyo3-version-dependent and not part of stable API. For correctness, we
    // instead require that callers go through the pymethods path for cons, and
    // flag the trait-method path as a known limitation (same gap as Var's IFn impl
    // in core-abstractions). See Task 10 FOLLOW-UP below.
    //
    // For now we return a nil to avoid undefined behavior; the dispatcher will
    // use the pymethods __call__ or a future corrected path.
    let _ = slf;
    py.None()
}

#[implements(ISeqable)]
impl ISeqable for PersistentList {
    fn seq(&self, _py: Python<'_>) -> PyResult<PyObject> {
        // seq on a non-empty list returns itself. But we can't produce Py<Self>
        // from &self. Return via a helper that the pymethods path wires up.
        // This is another manifestation of the same gap. For the default trait
        // method, return nil as a safe fallback — pymethods path below does the real work.
        Err(pyo3::exceptions::PyNotImplementedError::new_err("use Py<Self> path"))
    }
}

#[implements(Counted)]
impl Counted for PersistentList {
    fn count(&self, _py: Python<'_>) -> PyResult<usize> { Ok(self.count as usize) }
}

// Remaining protocol impls (IEquiv, IHashEq, IMeta, IPersistentCollection, IPersistentList,
// IPersistentStack, Sequential) follow the same pattern — see FOLLOW-UP note.

#[implements(IPersistentList)]
impl IPersistentList for PersistentList {}

#[implements(Sequential)]
impl Sequential for PersistentList {}

// Python-side constructor. Variadic.
#[pyfunction]
#[pyo3(signature = (*args))]
pub fn list_(py: Python<'_>, args: Bound<'_, PyTuple>) -> PyResult<PyObject> {
    if args.is_empty() {
        return Ok(empty_list(py).into_any());
    }
    // Build right-to-left: tail is empty_list, head is last element; then prepend each earlier element.
    let mut tail: PyObject = empty_list(py).into_any();
    let mut count: u32 = 0;
    for i in (0..args.len()).rev() {
        let item = args.get_item(i)?.unbind();
        count += 1;
        let node = PersistentList {
            head: item,
            tail,
            count,
            meta: RwLock::new(None),
        };
        tail = Py::new(py, node)?.into_any();
    }
    Ok(tail)
}

pub(crate) fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EmptyList>()?;
    m.add_class::<EmptyListIter>()?;
    m.add_class::<PersistentList>()?;
    m.add_class::<PersistentListIter>()?;
    m.add_function(wrap_pyfunction!(list_, m)?)?;

    // Initialize the EmptyList singleton.
    let el = Py::new(py, EmptyList { meta: RwLock::new(None) })?;
    let _ = EMPTY_LIST.set(el);
    Ok(())
}
```

### **FOLLOW-UP for Task 10 implementer**

The `ISeq for PersistentList`, `ISeqable for PersistentList`, `IPersistentCollection for PersistentList` trait methods need access to `Py<Self>` (not just `&Self`) to produce correct results (e.g., `cons` needs to put `self` into the tail of a new list; `seq` on a non-empty list returns self). We faced the same issue with Var in core-abstractions.

**Pragmatic workaround for this task:** use **pymethods only** for the user-facing surface. Implement `cons` / `seq` / `next` / `__iter__` as `#[pymethods]` with `slf: Py<Self>` signatures. Route the trait-method `ISeq` impls through simple cases that don't need self-by-handle:
- `first` → return `self.head.clone_ref(py)` (doesn't need Py<Self>)
- `next` → look at `self.tail`: if EmptyList, return nil; else return `self.tail.clone_ref(py)`
- `more` → return `self.tail.clone_ref(py)`
- `cons` → use the TLS-stashing trick the `#[implements]` codegen wrapper is about to set up (the macro expansion passes `self_bound: Bound<'_, Self>` to a thread-local right before calling the trait method; the trait method reads it to get `Py<Self>`).

For the cleanest path in this spec, **extend `#[implements]` codegen (in `clojure_core_macros/src/implements.rs`) to stash the `Bound<'_, Self>` in a TLS cell before calling the trait method. Trait method helpers can then read it via a `crate::current_self<T>()` helper.** Do this in Task 10.5 (between 10 and 11) if needed.

**Alternative, simpler:** add a `#[pymethods] fn seq(slf: Py<Self>, py) -> PyResult<PyObject>` etc. that shadows the trait-method registration for ISeqable. Then the pymethods path is what user-facing dispatch hits. The trait method remains "internal" and returns `PyNotImplementedError`.

This is a trade-off: the clean path is the macro extension. The quick path is the per-pymethods shadow. **Ask the controller which path to take before proceeding with Step 5.**

- [ ] **Step 5: Rebuild + run**

```bash
source .venv/bin/activate && maturin develop --release 2>&1 | tail -5
pytest tests/test_plist.py -v 2>&1 | tail -15
```
If tests fail due to the trait-method self gap, STOP and report NEEDS_CONTEXT — this requires a design decision on how to handle self-by-handle in trait methods.

- [ ] **Step 6: Commit**

Once tests pass:
```bash
git add -A
git commit -m "feat(collections): PersistentList + EmptyList singleton

Cons-cell list with O(1) count, IPersistentList + IPersistentStack +
ISeq + ISeqable + Counted + IEquiv + IHashEq + IMeta + Sequential.
EmptyList is a module-init singleton — (list) always returns the same
instance.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---


---

## Phases 6–15 (to be drafted just-in-time during execution)

**Phases 6–15 are not yet written in detail.** The scope is captured in the spec (§2–§7) and summarized below — each phase's bite-sized tasks will be drafted as a subagent picks up the implementation and we confirm the shape of the prior phase landed cleanly.

- **Phase 6: PersistentVector + HAMT nodes + TransientVector.** Port `PersistentVector.java` + `TransientVector.java`. Files: `collections/pvector.rs`, `collections/pvector_node.rs`. Tests: `tests/test_pvector.py`.
- **Phase 7: MapEntry.** Simple 2-tuple pyclass.
- **Phase 8: PersistentHashMap + HAMT nodes + TransientHashMap.** Port `PersistentHashMap.java` + nested Node classes. Files: `collections/phashmap.rs`, `collections/phashmap_node.rs`. Tests: `tests/test_phashmap.py`.
- **Phase 9: PersistentArrayMap + TransientArrayMap.** Flat-array small-map with auto-promotion to PHashMap at threshold. File: `collections/parraymap.rs`. Tests: `tests/test_parraymap.py`.
- **Phase 10: PersistentHashSet + TransientHashSet.** Thin wrapper over PHashMap. File: `collections/phashset.rs`. Tests: `tests/test_phashset.py`.
- **Phase 11: Additional rt helpers.** `rt::conj`, `rt::assoc`, `rt::dissoc`, `rt::nth`, `rt::contains`, `rt::transient`, `rt::persistent_bang`, `rt::conj_bang`, `rt::assoc_bang`, `rt::dissoc_bang`, `rt::disj_bang`, `rt::pop_bang`. Each routed through its protocol. Modify: `rt.rs`.
- **Phase 12: Seq types.** `Cons`, `LazySeq`, `ChunkedCons`/`ChunkedSeq`, `IteratorSeq`. Files: `seqs/cons.rs`, `seqs/lazy_seq.rs`, `seqs/chunked_cons.rs`, `seqs/iterator_seq.rs`. Tests: `tests/test_seqs.py`.
- **Phase 13: Property-based fuzzing (`hypothesis`).** `tests/test_collections_fuzz.py` — random op sequences vs Python built-in references (`list` / `dict` / `set`). 500 cases per property on CI. Covers vector/hash-map/array-map/hash-set + transient roundtrip + structural sharing integrity.
- **Phase 14: Rust proptest for HAMT node invariants.** `crates/clojure_core/tests/proptest_hamt.rs` — bitmap popcount == array.len, ArrayNode slot count == count field, depth bound.
- **Phase 15: Loom for transient edit token + stress test + README update.** `crates/clojure_core/tests/loom_transient_edit.rs`, `tests/test_collections_stress.py`, update `README.md`.

Each phase's detailed tasks will be written before the phase is dispatched to a subagent, using the same pattern as Phases 0–5: file list, bite-sized steps with complete code, verification commands, commit template.

---
