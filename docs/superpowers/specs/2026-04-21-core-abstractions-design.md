# Clojure-on-Python Core Abstractions — Design Spec

**Date:** 2026-04-21
**Status:** Draft for review
**Scope:** First sub-project of the clojure-py revival. Establishes the foundational abstractions that every later sub-project (reader, evaluator, collections, core.clj, STM, Cranelift JIT) will sit on.

---

## 1. Goal

Deliver the minimum set of primitives that make the statement *"everything else is translation"* true:

1. A **protocol** system with Clojure-fidelity semantics, implemented so Clojure objects are native Python objects and dispatch interoperates transparently with Python's type system.
2. **`IFn`** as the canonical callable protocol, with Python callables satisfying `IFn` and vice-versa.
3. **`Var`** and **`Namespace`** with full Clojure fidelity, where a namespace is literally a Python module.
4. **`Symbol`** and **`Keyword`** as first-class value types.
5. A **Rust attribute-macro DSL** that is the only way new protocols, records, and deftype-like constructs get declared — so all generated boilerplate (PyO3 classes, dispatch registrations, trait impls, method objects) comes from one place.

Explicitly **not** in scope for this spec: reader, printer, evaluator, persistent collections, seq abstraction, `core.clj` bootstrap, concurrency primitives (atom/agent/ref/STM), REPL, JIT. Each becomes its own spec. The design must leave clean hooks for those — specifically an inline-cache slot on protocol dispatch, and a Var representation that supports future watches/validators without rework — but their code is not written here.

---

## 2. Architecture at a Glance

```
 Python 3.14 (free-threaded interpreter)
 ┌───────────────────────────────────────────────────────────────┐
 │ sys.modules                                                    │
 │   "clojure"           ← top-level package (PyInit_clojure)     │
 │   "clojure.core"      ← ClojureNamespace(types.ModuleType)     │
 │   "clojure.user"      ← ClojureNamespace                       │
 │   ...                                                          │
 └───────────────────────────────────────────────────────────────┘
         │             attrs are Vars            dunders are ns meta
         ▼
 ┌───────────────────────────────────────────────────────────────┐
 │  Rust extension crate `clojure_core` (PyO3, maturin-built)     │
 │                                                                │
 │   Macros (proc-macro crate `clojure_core_macros`):             │
 │     #[protocol]    — trait → protocol object + dispatchers     │
 │     #[implements]  — impl block → cache registration           │
 │     #[defrecord]   — struct → PyO3 class + IPersistentMap impl │
 │     #[deftype]     — struct → PyO3 class + declared protocols  │
 │                                                                │
 │   Runtime primitives:                                          │
 │     Protocol, ProtocolMethod, MethodCache (+ epoch)            │
 │     IFn trait (invoke0..invoke20 + invoke_variadic)            │
 │     AFn   (default impls: throw ArityException)                │
 │     Symbol, Keyword (+ global intern table)                    │
 │     Var, Namespace                                             │
 │     Dynamic-binding thread-local frame stack                   │
 └───────────────────────────────────────────────────────────────┘
```

Python package: `clojure` (top-level, registered by the Rust extension).
Rust workspace: two crates.

- `clojure_core` — the PyO3 extension module. Contains runtime types.
- `clojure_core_macros` — `proc-macro` crate used by `clojure_core`. Exports the attribute macros.

Both are built by `maturin`; `clojure_core` is the cdylib that Python loads.

---

## 3. Rust Macro DSL

All public Clojure primitives defined in Rust use attribute macros on Rust items. No function-like macros in this spec (they can come later if ergonomics require). This keeps items first-class to rust-analyzer, rustfmt, rustdoc, and error messages.

### 3.1 `#[protocol]`

Attached to a `trait`. Declares a protocol and generates the runtime machinery.

```rust
#[protocol(
    name = "clojure.core/IFn",
    extend_via_metadata = false,
)]
pub trait IFn {
    fn invoke0(&self, py: Python<'_>) -> PyResult<PyObject>;
    fn invoke1(&self, py: Python<'_>, a: PyObject) -> PyResult<PyObject>;
    fn invoke2(&self, py: Python<'_>, a: PyObject, b: PyObject) -> PyResult<PyObject>;
    // ... invoke0..invoke20
    fn invoke_variadic(&self, py: Python<'_>, args: &Bound<'_, PyTuple>) -> PyResult<PyObject>;
}
```

The macro generates:

- A `PyO3` class `IFnProtocol` with a module-level singleton registered in the `clojure.core` namespace as the Var `#'IFn`.
- One `ProtocolMethod` object per trait method, each itself an `IFn` so it can be called as a function (`(invoke2 f a b)` works).
- A per-protocol `MethodCache` (see §4).
- A `fallback: RwLock<Option<PyObject>>` slot.
- A `dispatch_invokeN` entry point per arity that performs cache lookup and calls the resolved impl.

### 3.2 `#[implements]`

Attached to an `impl Trait for Type` block. Registers the Rust impl into the protocol's method cache at Python-module init.

```rust
#[implements(IFn)]
impl IFn for Keyword {
    fn invoke1(&self, py: Python<'_>, a: PyObject) -> PyResult<PyObject> {
        clojure_core::rt::get(py, a, self.clone_into_py(py)?)
    }
    // ... other arities default to AFn-throwing impls via #[implements(IFn, default = AFn)]
}
```

The macro:

- Emits a `submit!`-style distributed-slice entry so module-init collects every `#[implements]` and registers them in one pass without central listing.
- Wraps each trait method in a thin adapter that converts between the Rust signature and the cache's erased calling convention.
- If `default = AFn` is given, unspecified arities are filled with `AFn`'s arity-exception-throwing defaults.

For extending protocols to Python built-in types that have no corresponding Rust `struct` (e.g. `int`, `str`, `list`, `function`), the target is a zero-sized marker struct:

```rust
pub struct PyLongMarker;

#[implements(IFn, py_type = "builtins.int")]  // resolved at init via PyType lookup
impl IFn for PyLongMarker {
    fn invoke1(&self, py: Python<'_>, a: PyObject) -> PyResult<PyObject> {
        // int is being used as an IFn — treat as (get coll int) via IFn semantics
        clojure_core::rt::nth(py, a, self.value)?
    }
}
```

The `py_type` argument tells the macro to look up the Python type at init time and register against that `PyType*` rather than against `TypeId::of::<PyLongMarker>()`. The marker is never instantiated; only its registration entry matters.

### 3.3 `#[defrecord]` / `#[deftype]`

Out of scope for *this* spec (records belong with the collections spec since they *are* a flavour of persistent map). Reserved for the next sub-project. This spec does not define them.

---

## 4. Protocol System

### 4.1 Data model

```rust
pub struct Protocol {
    pub name:          Py<Symbol>,                     // fully qualified, e.g. clojure.core/IFn
    pub methods:       SmallVec<[Py<ProtocolMethod>; 8]>,
    pub cache:         Arc<MethodCache>,
    pub fallback:      RwLock<Option<PyObject>>,
    pub via_metadata:  bool,
}

pub struct ProtocolMethod {
    pub protocol:    Py<Protocol>,
    pub key:         Arc<str>,                         // method name, e.g. "invoke2"
    pub arity:       Arity,                            // Fixed(n) | Variadic
    // IFn impl: Looks up dispatch target and calls it.
}

pub struct MethodCache {
    pub epoch:   AtomicU64,                            // bumped on every extend-type / unextend
    pub entries: DashMap<TypeId_or_PyTypePtr, Arc<MethodTable>>,
}

pub struct MethodTable {
    pub impls:  FxHashMap<Arc<str>, PyObject>,         // method key → callable
    pub origin: Origin,                                // InlineAttr | Extend | Metadata | Fallback
}
```

Keys in `entries` are CPython `PyType*` pointers (for extension-target types) or Rust `TypeId`s (for our own types that implement via `#[implements]`). Both are `usize`-erased into a `NonZeroUsize` tag union.

### 4.2 Dispatch algorithm

Called as `protocol.dispatch(method_key, target, args)`:

```
1. entry = cache.entries.get(type_of(target))        -- exact type hit
   if entry and method_key in entry.impls: return entry.impls[method_key](target, *args)
2. for parent in MRO(type_of(target))[1:]:
       entry = cache.entries.get(parent)
       if entry and method_key in entry.impls:
           cache.entries.insert(type_of(target), entry)   -- promote MRO hit to exact
           return entry.impls[method_key](target, *args)
3. if via_metadata:
       meta = get_meta(target)
       if meta and method_key in meta:
           return meta[method_key](target, *args)
4. if fallback is set and not already_consulted_this_call:
       fallback(protocol, method_key, target)
       -- re-run steps 1-3 once (MRO walk and metadata are re-checked too),
       -- with already_consulted_this_call = true so step 4 short-circuits.
5. raise IllegalArgumentException("No implementation of method: " + method_key
                                  + " of protocol: " + protocol.name
                                  + " found for class: " + type_of(target))
```

**Guarantees:**

- Every `extend-type` / `extend-protocol` / `unextend` call bumps `cache.epoch`.
- The MRO-promotion in step 2 is safe under concurrent dispatch because `DashMap::insert` is atomic; a concurrent `extend-type` that bumps the epoch may make the promoted entry momentarily stale, which is corrected on the next dispatch.
- The fallback is consulted at most once per dispatch invocation (guarded by a `fallback_consulted: bool` local to the dispatch call frame, so nested dispatches fired from *within* the fallback each get their own independent fallback allowance, while a single call can't recurse into its own fallback).

### 4.3 Inline-cache hook (implementation deferred)

The per-call-site inline cache pattern is out of scope for implementation in this spec, but the `MethodCache` exposes `epoch` as a public atomic and the dispatch API takes an optional `&InlineCacheSlot` that, when present, is consulted before step 1 and filled on hit. Follow-on specs can add `invoke!(...)` macros and AST-node `#[inline_cache]` fields without revisiting the `Protocol` / `MethodCache` surface. Shape sketch:

```rust
struct InlineCacheSlot {
    ty:    AtomicUsize,   // cached PyType* / TypeId
    epoch: AtomicU64,     // epoch when filled
    imp:   AtomicPtr<()>, // erased fn pointer
}
```

This is in the spec so the data structures are designed around it, not implemented.

### 4.4 Fallback function

Per-protocol single-slot. Python callable, set via `Protocol.set_fallback(fn)`, cleared via `None`. Signature: `fn(protocol, method_key, target) -> None`. The fallback may call `extend-type` / `extend-protocol` freely; those writes bump the epoch, but the in-flight resolution is already inside a re-entry window and proceeds to step 1 of the retry.

Built-in fallback installed at init: **`IFn.fallback`** checks `PyCallable_Check(target)` and, if true, installs a generic `invoke_variadic(self, *a) -> self(*a)` impl for `type(target)` into the cache. This is how arbitrary Python callables satisfy `IFn`. After the first call, `satisfies?(IFn, x)` returns true for that type.

### 4.5 `extend-via-metadata`

Opt-in at `#[protocol]` declaration via `extend_via_metadata = true`. When true, step 3 of dispatch reads the target's Clojure meta (IMeta protocol) for a map under the protocol method key. Per-instance, per-call — no caching.

---

## 5. `IFn`

### 5.1 Arity structure

Canonical Clojure-JVM shape: `invoke0`, `invoke1`, ..., `invoke20`, `invoke_variadic(args)` where `args` is a `PyTuple` of arguments 21+ (or all args for purely-variadic fns). Matches Clojure's `IFn` and gives Cranelift-JIT-friendly fixed-arity entry points.

`AFn` is provided as a base: all `invokeN` default to raising `ArityException`. Concrete IFns implement the arities they support; the macro emits `AFn` defaults for the rest unless the user specifies differently.

### 5.2 Python `__call__` bridge

Every PyO3 class generated by `#[implements(IFn)]` has a generated `__call__(*args)` method that routes to the right `invoke{N}` by `len(args)` (or to `invoke_variadic` for `N > 20`). Python users never see `invoke{N}` — they call `obj(...)` as normal.

### 5.3 Python callables as `IFn`

See §4.4. `IFn` has a built-in fallback that treats any `PyCallable_Check`-true object as an `IFn` implementing `invoke_variadic` via `obj(*args)`. Fixed arities are answered by the same variadic impl (the dispatcher forwards `(a,b)` as `args=(a,b)`). After first call on a new callable type, it's cached.

### 5.4 `satisfies?`

```
satisfies?(protocol, x) :=
   type_of(x) in protocol.cache.entries         -- includes MRO-promoted entries
|| (protocol.via_metadata && method_in_meta(x, _))
|| (protocol.fallback is set && probe(protocol, x))  -- probes by calling fallback in read-only mode
```

The "probe" path for fallback is implemented by calling the fallback with a special sentinel method key `:satisfies?/probe`, which the `IFn` built-in fallback interprets as "register if callable". This keeps `satisfies?` in sync with "what a call would do."

---

## 6. `Symbol`

```rust
#[pyclass(frozen)]
pub struct Symbol {
    pub ns:   Option<Arc<str>>,
    pub name: Arc<str>,
    pub hash: u32,                     // cached at construction
    pub meta: RwLock<PyObject>,        // Clojure map or nil
}
```

- **Not interned.** Two calls to `symbol("foo")` produce two distinct Python objects.
- Value-equal on `(ns, name)`. `__hash__` returns `hash`. `__eq__` compares `ns` + `name`.
- `with_meta(new_meta) -> Symbol` produces a new Symbol sharing `Arc<str>` pointers; no name allocation.
- Implements `IMeta`, `IObj`, and (later) `Named`.

---

## 7. `Keyword`

```rust
#[pyclass(frozen)]
pub struct Keyword {
    pub sym:  Py<Symbol>,
    pub hash: u32,                     // precomputed, folds ns/name with a keyword tag
}

// Global interning table.
static KEYWORD_INTERN: Lazy<DashMap<(Option<Arc<str>>, Arc<str>), Py<Keyword>>> = ...;
```

- **Globally interned.** `keyword("foo")` twice returns the same Python object.
- Equality is pointer identity post-intern (plus a value-fallback for paranoia in edge cases during concurrent insertion).
- `hash` is precomputed.
- **Callable:** `(:k m)` = `(get m :k)`, `(:k m default)` = `(get m :k default)` — registered on `Keyword` via `#[implements(IFn)]`.
- `DashMap` gives us concurrent intern under free-threaded 3.14. The rare race where two threads intern the same keyword simultaneously resolves by `entry().or_insert_with()`.

---

## 8. `Var`

```rust
#[pyclass]
pub struct Var {
    pub ns:        Py<Namespace>,
    pub sym:       Py<Symbol>,
    pub root:      AtomicPtr<PyObject>,        // PyObject*, or UNBOUND sentinel
    pub dynamic:   AtomicBool,
    pub meta:      RwLock<PyObject>,
    pub watches:   RwLock<PersistentMap<Keyword, PyObject>>,
    pub validator: RwLock<Option<PyObject>>,
    // No thread-binding state stored on the Var itself — see §8.2.
}
```

### 8.1 Root + mutation

- `deref` on a non-dynamic var: atomic load `root`. If `UNBOUND` sentinel, raise `IllegalStateException: Var X/y is unbound`.
- `alter-var-root(f, ...args)` uses a CAS loop: load → call `f(old, ...args)` → CAS; retry on failure. Runs the validator (if any) on the proposed new value before CAS.
- `bind-root(v)` sets root directly (no CAS-loop; CAS once; no validator); used during bootstrap.
- Setting `dynamic` to true is a one-way toggle during `declare`/`defonce`-ish flows; for simplicity we allow toggling both ways via `set-dynamic!`.

### 8.2 Dynamic binding

Thread-local stack of **frames**, each frame a persistent map `PersistentMap<Py<Var>, Box<PyObject>>`:

```rust
thread_local! {
    static BINDING_STACK: RefCell<Vec<PersistentMap<Py<Var>, Box<PyObject>>>> = RefCell::new(vec![]);
}
```

- `push-thread-bindings(map)` pushes a new frame that is `(top-frame merge map)`.
- `pop-thread-bindings()` pops.
- `deref` on a dynamic var: if binding stack non-empty, look up the var in the top frame; on miss, fall through to root.
- `set!` on a dynamic var mutates the top frame's entry for that var. Raises if the var has no binding in the current frame (must be in a `binding` block).

The stack is per-OS-thread. Free-threaded 3.14 exposes OS threads directly, so `thread_local!` is correct.

### 8.3 Cross-thread binding conveyance

`bound-fn` and `bound-fn*` (in scope for this spec):

```
(bound-fn* f)
  captures: snapshot = current-top-frame (or empty persistent map)
  returns:  a fn that, when invoked on any thread, pushes `snapshot`, calls f, pops.
```

Implemented as an `IFn` wrapper PyO3 class that holds `snapshot: PersistentMap<Py<Var>, Box<PyObject>>` and a target `PyObject`, and in its `invoke*` methods does push/call/pop around the invocation.

### 8.4 Watches / validator

- `add-watch`, `remove-watch`: insert/remove into `watches`. After every root change, call each watch as `(watch-fn key ref old new)`.
- `set-validator!`: install on `validator`. Called before every proposed root change; rejection by returning false or throwing reverts the change.

### 8.5 Var implements `IFn` directly

Var is registered via `#[implements(IFn)]` with every arity. Each `invokeN` implementation loads `root` (atomic) and dispatches `IFn.invokeN` on the root — one extra `IFn` dispatch, then we're in the root's native impl. This is important because Var is `callable()` from Python (it has `__call__`), and without a direct `IFn` impl the Python-callable fallback (§4.4) would register a generic `Var(*args)→__call__` path, leading to a redundant level of indirection on every call. Direct impl avoids that and is symmetric with Clojure-JVM, where `clojure.lang.Var` implements `IFn`.

### 8.6 Delegation surface ("pythonic enough")

`Var` implements, all delegating to the current value of `deref()`:

- `__call__`(`*args`) → `invoke*` through `deref` (via `IFn` dispatch, so this participates in the protocol machinery — no special case).
- `__eq__`, `__hash__`.
- `__repr__` returns `#'ns/sym` (Clojure-style).
- `__str__` → `str(deref())`.
- `__bool__` → `bool(deref())` (Clojure truthiness lives in `Nil`/`Bool`'s own `__bool__`, not here).
- Arithmetic & comparison: `__add__`, `__radd__`, `__sub__`, `__rsub__`, `__mul__`, `__rmul__`, `__truediv__`, `__rtruediv__`, `__floordiv__`, `__rfloordiv__`, `__mod__`, `__rmod__`, `__neg__`, `__pos__`, `__abs__`, `__lt__`, `__le__`, `__gt__`, `__ge__`.
- Containers: `__len__`, `__iter__`, `__contains__`, `__getitem__`.
- `__getattr__` (last resort, after Var's own attrs): delegate to `deref()`.

Every delegation re-reads `root` (atomic load); no caching. `alter-var-root` is visible on the next access.

Var-native attrs that are never delegated: `.meta`, `.deref`, `.bindRoot`, `.alterRoot`, `.isDynamic`, `.isBound`, `.watches`, `.ns`, `.sym`, `.addWatch`, `.removeWatch`, `.setValidator`, `.getValidator`, `.push_thread_bindings`, `.identity_eq`.

### 8.7 Known visible edge

`isinstance(ns.N, int)` is `False` when `N`'s root is an `int`, because `ns.N` is a `Var`. Documented. Escape hatch: `ns.N.deref()` returns the raw `int`.

---

## 9. `Namespace`

### 9.1 Namespaces ARE modules

```rust
#[pyclass(extends=PyModule, name="ClojureNamespace")]
pub struct ClojureNamespace {
    // No Rust-side mappings field: module's __dict__ IS the var mapping.
    // No Rust-side aliases field: lives in __clj_aliases__ on the module.
}
```

A `ClojureNamespace` is a subclass of `types.ModuleType`. When Clojure creates the namespace `my.pkg.ns`, we:

1. Walk the dotted path, creating `ClojureNamespace` instances for `my`, `my.pkg`, `my.pkg.ns` as needed.
2. Each gets registered in `sys.modules` under its full dotted name.
3. Parent modules get the child installed as an attribute so `my.pkg.ns` resolves via attribute walk from `my`.
4. Dunder metadata is populated on the new namespace.

### 9.2 Metadata dunders

Every `ClojureNamespace` carries these dunder entries in its `__dict__`:

| Entry              | Type                             | Purpose                                                                 |
|--------------------|----------------------------------|-------------------------------------------------------------------------|
| `__clj_ns__`       | `Symbol`                         | The namespace's fully-qualified name as a Symbol.                       |
| `__clj_ns_meta__`  | Clojure map or nil               | Namespace metadata (docstring, `^{:author ...}` etc.).                  |
| `__clj_aliases__`  | `{Symbol → ClojureNamespace}`    | Alias table (e.g. `set/union` → `clojure.set` aliased as `set`).        |
| `__clj_refers__`   | `{Symbol → Var}`                 | Provenance of refer-imported vars (vars also attached directly).        |
| `__clj_imports__`  | `{Symbol → PyType}`              | `(import ...)` mappings for Python/Java-style class imports.            |
| `__clj_lock__`     | `PyCapsule<Mutex<()>>`           | Guards compound mutations on this ns (intern+refer, alias+import). Not for public use. |

Programmatic access goes through `clojure.core/ns-map`, `ns-aliases`, `ns-refers`, `ns-imports`, `ns-meta` (all spec'd here as thin wrappers over module attribute reads).

### 9.3 Vars as module attributes

`intern(ns, sym) -> Var`:

1. If `sym.name` already in `ns.__dict__` as a `Var`, return it.
2. Else create `Var(ns, sym, UNBOUND)`, store under `sym.name` in `ns.__dict__`, return.

`refer` copies another namespace's `Var` into this namespace's `__dict__` under the target symbol's name, and records `{target_sym → var}` in `__clj_refers__`.

Var attribute names are the Clojure symbol names **un-munged**: `(defn foo? [x] ...)` stores the var at `ns.__dict__["foo?"]`. Python users reach it with `getattr(ns, "foo?")`. Documented.

### 9.4 Namespace registry

`sys.modules` is the registry. `find-ns(sym)` is `sys.modules.get(str(sym))` with a type check that it's a `ClojureNamespace`. `create-ns(sym)` walks the dotted path as in §9.1.

### 9.5 Interaction with Python `import`

- `import clojure.user` loads the extension, which on its init hook creates `clojure` and `clojure.user` namespaces. Works.
- `from clojure.user import foo` resolves via normal Python attribute lookup on the module; returns the `Var` (because that's what's stored).
- Importing a Clojure file (`.cljpy` or similar) is out of scope until the reader/evaluator spec. This spec only supports programmatic `create-ns`.

---

## 10. Concurrency Under Free-Threaded 3.14

Hot-path invariants:

- **Keyword interning**: `DashMap`. Concurrent insert races resolve via `entry().or_insert_with(new_keyword)`.
- **Method cache**: `DashMap` for entries + `AtomicU64` epoch. `extend-type` takes a per-protocol write-intent lock only around the entry mutation, not across the Python callable invocation.
- **Var root**: `AtomicPtr<PyObject>`; `alter-var-root` is a CAS loop.
- **Namespace mappings** (`ns.__dict__`): this is CPython's dict, so atomicity under 3.14t is governed by CPython. We rely on dict operations being individually atomic; compound mutations (intern-and-refer, alias+import) are guarded by a per-namespace `Mutex` we hang off the module as a dunder `__clj_lock__` (a PyCapsule wrapping a Rust `Mutex<()>`).
- **Binding stack**: `thread_local!`, so no cross-thread contention. `bound-fn*` captures a `PersistentMap` by Arc-clone, which is lock-free.

No GIL assumptions anywhere. Every shared-mutable piece is either atomic, a concurrent map, or behind an explicit Rust lock.

---

## 11. Testing & Correctness Bar

The spec is "done" when all of the following pass on CPython 3.14t:

1. **Macro expansion** (`trybuild` + unit tests on the `clojure_core_macros` crate): a `#[protocol]` trait compiles to the expected PyO3 classes with the expected method names; an `#[implements]` block registers correctly at init; attribute macros produce useful compile errors for malformed input.

2. **Protocol dispatch goldens** (Python-level tests via pytest, loading the compiled extension):
   - Cache hit on exact type.
   - MRO walk resolves to parent impl, and promotes the entry for the subtype.
   - `extend-via-metadata` opt-in is honored; disabled by default.
   - Fallback is called once on miss; fallback-registered impl is used on retry.
   - Fallback guard prevents infinite recursion.
   - `extend-type` bumps the epoch; existing `MethodTable` references see the new impl via cache re-read on next dispatch.

3. **IFn round-trip**:
   - PyO3-defined `IFn` is callable from Python (`obj(1, 2, 3)`), routes to right `invokeN`.
   - `invoke_variadic` dispatched for `N > 20`.
   - `lambda`, `def`, `functools.partial`, `builtins.max`, a bound method, and a `type` object all usable as IFn from the Rust side (`rt::invoke(py_callable, args)`).
   - `satisfies?(IFn, lambda: 0)` is true after first call; pointer-identical result on repeat calls (cache hit).

4. **Namespace-as-module**:
   - `create_ns("clojure.user")` results in `sys.modules["clojure.user"]` being a `ClojureNamespace`.
   - Parent namespaces auto-created.
   - `import clojure.user` works and returns the same object.
   - `intern(ns, sym("foo"))` results in `getattr(ns, "foo")` being a `Var`.
   - `refer`, `alias`, `import` correctly populate `__clj_refers__`, `__clj_aliases__`, `__clj_imports__` and install refer'd Vars as direct attrs.
   - `ns-map`, `ns-aliases`, `ns-refers`, `ns-imports`, `ns-meta` return expected data.
   - Symbol with non-identifier chars (`foo?`, `bar!`, `*baz*`) stored un-munged and reachable via `getattr`.

5. **Var semantics**:
   - Root load / `alter-var-root` CAS under concurrency (two threads race to increment; sum is correct).
   - `binding` push/pop correctness; nested `binding` frames.
   - `set!` in `binding` scope mutates frame; `set!` outside scope raises.
   - `bound-fn*` correctly conveys top frame to another thread.
   - Watches fire with `(key ref old new)` after every root change.
   - Validator rejects changes that return false or throw.
   - Unbound Var deref raises `IllegalStateException`.
   - `__repr__` is `#'ns/sym`; `__call__` delegates; `ns.V + 1` works when root is an int (via `__add__` delegation); `isinstance(ns.V, int)` is False (documented edge).

6. **Keyword interning**:
   - `keyword("foo") is keyword("foo")`.
   - `keyword("a", "b") is keyword("a", "b")`.
   - Cross-thread intern race: 32 threads each interning 10k keywords from a shared pool, all return pointer-identical instances per key.
   - `(:k {:k 1})` returns `1`.

7. **Symbol value equality & meta independence**:
   - `symbol("foo") == symbol("foo")` but `symbol("foo") is not symbol("foo")`.
   - `(with-meta (symbol "foo") {:a 1})` equals `symbol("foo")` (meta is not part of equality).
   - Meta mutation on one doesn't affect the other.

8. **Concurrency correctness of Rust-only primitives** (Loom model-checking, via `cargo test --features loom`): exhaustive interleaving coverage for `MethodCache` (concurrent `insert` + dispatch + epoch bump), the `Keyword` intern table (concurrent `or_insert_with`), the `Var` root atomic (concurrent `alter-var-root` CAS loop), and the `BINDING_STACK` push/pop under a cross-thread `bound-fn` snapshot. Each primitive gets a Loom test that validates no torn reads, no lost updates, and no race where a live dispatch sees a half-constructed `MethodTable`.

9. **Integration stress test** (Python-level, pytest): the concurrent `extend-type` + dispatch scenario from bullet 2 runs under `ThreadPoolExecutor(max_workers=32)` for a fixed wall-clock budget (~10s), asserting no exceptions, no inconsistent dispatch results, and no deadlocks. This is a smoke test — it doesn't catch rare races but catches gross ones.

---

## 12. Non-Goals / Follow-on Specs

Each of these is a separate spec, in roughly this order:

1. **Persistent collections** — list, vector, map, set, chunked seqs, transients. Implements `ISeq`, `IPersistentCollection`, etc. via `#[implements]`. Also defines `#[defrecord]` as a flavour of persistent map.
2. **Reader + printer** — `read`, `read-string`, `pr`, `pr-str`, metadata reader literals, tagged literals.
3. **Tree-walking evaluator** — special forms, macroexpansion plumbing, `eval`.
4. **Macros + `core.clj` bootstrap** — syntax-quote, `defmacro`, then porting the Clojure `core.clj` prelude.
5. **Concurrency primitives** — `atom`, `agent`, `ref`/STM leveraging free-threaded 3.14.
6. **Inline-cache implementation** — `invoke!` macro, AST-node `#[inline_cache]`, seqlock read pattern using the epoch already present on `MethodCache`.
7. **REPL + packaging**.
8. **Cranelift JIT**.

This spec leaves explicit hooks for #1 (`#[defrecord]` reserved), #6 (`InlineCacheSlot` shape in §4.3), and #5 (thread-safe Var + binding stack already correct for STM integration).

**Deferred infrastructure: TSan CI.** A TSan-instrumented CPython 3.14t + TSan-instrumented `clojure_core` nightly CI job is the intended long-term concurrency safety net, but is out of scope for the first spec's blocking test bar. Reason: requires a custom CPython build with `--with-thread-sanitizer`, nightly Rust, and a dedicated CI job; the infrastructure work would dominate the spec. Loom (§11 bullet 8) + pytest stress (§11 bullet 9) are the blocking correctness bar; TSan is a follow-on nightly job to be stood up once the project has enough code under it to justify the setup cost.
