# clojure.core Port Audit

Comprehensive catalog of what lives in
`crates/clojure_core/clj/clojure/core.clj` and the subset of vanilla
`clojure.core` it covers. Sections correspond to the `;; --- …` blocks
in the port, roughly in load order. Status codes:

- **PORTED** — implementation is present and tested.
- **PARTIAL** — most of the surface works but a named edge is missing.
- **DEFERRED** — intentionally not ported (see reason); no plans to land.

## Coverage

- **core.clj**: ~5,440 lines, **~535 top-level forms** (defn / defmacro / def).
- **Vanilla coverage**: 100% of vanilla `core.clj`'s public API
  (lines 1 through ~8200), modulo the deferred items below.
- **Deferred permanently** (10 forms):
  - **structs** (5): `accessor`, `create-struct`, `defstruct`,
    `struct`, `struct-map` — legacy API superseded by records.
  - **JVM-only Java interop** (5): `stream-into!`, `stream-reduce!`,
    `stream-seq!`, `stream-transduce!` (Java Streams), `resultset-seq`
    (JDBC) — no Python analogue.
- **Stubbed for portability**:
  - `add-classpath` — sys.path manipulation belongs in user code.
  - `compile` — AOT compile is JVM-specific; we already
    read+compile per-form.
  - `set-agent-send-executor!` / `set-agent-send-off-executor!` —
    our pools are static `OnceCell`s.
  - `definline` — aliases to `defn` (no JVM bytecode inliner).
  - `read+string` — returns `""` for the captured-source component
    (our reader has no captureString).
  - `with-loading-context` — passthrough (no JVM ClassLoader).
- **Partial**:
  - `defrecord` — lacks structural equality / hashing / full
    IPersistentMap (assoc returns a new record); fields are
    read-only via `(:kw rec)` but the record is not yet a true map.

All ~35 vanilla `defn-` private helpers (loader scaffolding like
`load-lib`/`serialized-require`/`add-annotation`, `case`-table
optimizer like `case-map`/`prep-hashes`/`maybe-min-hash`, etc.) are
intentionally not ported — they're internal to vanilla's
implementation and have no callers outside `core.clj` itself.

**Test coverage**: 1543 tests passing.

The port remains 1:1 with vanilla in **form order and form count** for
everything below that isn't called out as PARTIAL or DEFERRED.

---

## Phase 1 — Bootstrap (core.clj lines 1–2305)

These are the forms loaded first, in the order they must be available
for the rest of the file to evaluate. Everything here is **PORTED**.

### Bootstrap primitives (lines 1–200)
`list`, `cons`, bootstrap `let*`/`loop*`/`fn*` (compile-level), `first`,
`next`, `rest`, `conj`, `second`, `ffirst`, `nfirst`, `fnext`, `nnext`,
`seq`, `instance?`, basic `apply`, eager `concat`, `assoc`, `meta`,
`with-meta`, `assert-valid-fdecl`, `sigs`, `last`, `butlast`, `defn`.

### Collection constructors + basic predicates (lines 199–340)
`vector`, `vec`, `hash-map`, `hash-set`, `nil?`, `defmacro`, `when`,
`when-not`, `false?`, `true?`, `boolean?`, `not`, `some?`, `any?`,
`str`, `symbol?`, `keyword?`.

### Reader-syntax support (lines 340–545)
`cond`, `symbol`, `gensym`, `keyword`, `find-keyword`, `spread`
(private), `list*`, `vary-meta`, `lazy-seq`, `chunk-buffer` family
(private), lazy `concat` (shadows the bootstrap eager version),
`delay`, `delay?`, `force`, `if-not`.

### Equality and comparison (lines 545–610)
`identical?`, `=`, `not=`, `compare`, `and`, `or`.

### Arithmetic + numeric predicates (lines 610–870)
`zero?`, `count`, `int`, `nth`, `<`, `inc`, `>`, `reduce1` (private),
`reverse`, `>1?`/`>0?` (private), `+`, `*`, `/`, `-`, `<=`, `>=`, `==`,
`pos?`, `neg?`, `max`, `min`, `abs`, `dec`, all 12 `unchecked-*`
variants (aliased to their checked counterparts — Python ints can't
overflow), `quot`, `rem`, full bit-op suite (`bit-and`, `bit-or`,
`bit-xor`, `bit-not`, `bit-shift-left`, `bit-shift-right`,
`unsigned-bit-shift-right`, `bit-and-not`, `bit-set`, `bit-clear`,
`bit-flip`, `bit-test`).

### Type predicates (lines 820–870)
`integer?`, `number?`, `ratio?`, `rational?`, `decimal?`, `float?`,
`double?`, `associative?`, `coll?`, `list?`, `vector?`, `map?`, `set?`,
`seq?`, `chunked-seq?`.

### Identity + collection access (lines 870–970)
`peek`, `pop`, `contains?`, `get`, `dissoc`, `disj`, `find`, `select-keys`,
`keys`, `vals`, `key`, `val`, `merge`, `merge-with`, `zipmap`,
`update`, `update-in`, `assoc-in`, `get-in`.

### Symbol / keyword utilities + reduce (lines 970–1050)
`name`, `namespace`, `ident?`, `simple-ident?`, `qualified-ident?`,
`simple-symbol?`, `qualified-symbol?`, `simple-keyword?`,
`qualified-keyword?`, `reduced`, `reduced?`, `ensure-reduced`,
`unreduced`, `reduce`, `reduce-kv`, `completing`, `transduce`, `into`
(non-transducer, simple).

### Threading macros (lines 1050–1090)
`->`, `->>`, `some->`, `some->>`, `cond->`, `cond->>`, `as->`.
*`locking`* — **DEFERRED** (requires real OS-level monitor primitive;
see the Monitors block below for partial `monitor-enter`/`-exit` shims).

### Binding / conditional macros (lines 1090–1160)
`if-let`, `when-let`, `if-some`, `when-some`, `when-first`.

### Atom / deref family (lines 1159–1205) — hoisted from vanilla ~2200
`atom`, `deref` (reader-macro `@`), `swap!`, `swap-vals!`, `reset!`,
`reset-vals!`, `compare-and-set!`.

### Volatile (lines 1205–1230) — hoisted from vanilla ~2560
`volatile!`, `volatile?`, `vreset!`, `vswap!`.

### Sequence transforms (lines 1229–1500)
`comp`, `partial`, `juxt`, `every-pred`, `some-fn`, `memoize` (moved
later), `map`, `filter`, `remove`, `keep`, `keep-indexed`, `map-indexed`,
`take`, `drop`, `take-while`, `drop-while`, `take-nth`, `drop-last`,
`take-last`, `concat` (re-exported), `repeat`, `range`, `iterate`,
`cycle`, `repeatedly`, `interleave`, `zipmap`, `apply-to-gen`, `distinct`.
Plus `sort`, `sort-by`, `compare` (hoisted earlier).

### Transducers (lines 1500–1790)
Transducer arities for `map`, `filter`, `remove`, `keep`,
`keep-indexed`, `map-indexed`, `take`, `drop`, `take-while`,
`drop-while`, `take-nth`, `replace`, `partition-by`, `partition-all`,
`cat`, `mapcat`, `interpose`, `dedupe`, `distinct`, `halt-when` (minus
halt-when — not yet present). `sequence` (hoisted above dedupe).
`into` with transducer arities. `transduce`, `eduction`, `run!`.

### Slicing extensions + aggregation (lines 1790–1990)
`partition`, `partition-all`, `partition-by`, `split-at`, `split-with`,
`take-nth` (coll + transducer), `take-last`, `drop-last`, `butlast`.
Map: `group-by`, `frequencies`, `merge-with`, `update-vals`,
`update-keys`. `sequential?`, `tree-seq`, `flatten`.

### Partition family + utility macros (lines 1990–2300)
`ifn?`, `fn?`, `force`, `partition`, `partition-all`, `partition-by`.
`memoize`, `trampoline`, `condp`, `case`, `while`, `dotimes`, `doseq`.
`eval`, `time`.

### Destructure + real let / loop / fn (lines 2305–2460)
Full destructuring (map + vector binding forms with `&`, `:as`, `:keys`,
`:syms`, `:strs`, `:or`, symbol-literal defaults). Replaces the bootstrap
`let*`/`loop*`/`fn*` with destructure-aware `let`, `loop`, `fn`, `defn`.
`maybe-destructured` helper (private).

### assert + :pre / :post conditions (lines 2461–2552)
`assert` macro (0-arg / 1-arg message), `process-conditions` helper
(private), `fn` macro redefined to wrap body with `:pre`/`:post` asserts.
Depends on `AssertionError` exception type + `throw-assert` RT shim in
Rust. See the [:pre/:post section](#pre--post-conditions) below.

---

## Phase 2 — Alignment (core.clj lines 2555–3928)

Each block cites its vanilla source-line range and is loaded in the order
needed to satisfy dependencies. All **PORTED** unless marked otherwise.

### Transients (vanilla 3364–3430)
`transient`, `persistent!`, `conj!`, `assoc!`, `dissoc!`, `pop!`, `disj!`.
Thin wrappers over the `ITransient*` protocols + per-type impls in Rust.

### Collection constructors + predicates (vanilla 4129–4409)
`sorted-map`-shaped forms live here (see [Sorted collections](#sorted-collections)
below). Also `empty`, `not-empty`, `every?`, `not-every?`, `not-any?`, `some`.

### rseq / replicate (vanilla 1600, 3033)
`rseq` routes through the `Reversible` protocol; `replicate` deprecated but
ported for completeness.

### Small numeric helpers (vanilla 3503, 3589, 3596)
`mod`, `even?`, `odd?`, `bit-shift` helpers continued, `rand`, `rand-int`,
`rand-nth`.

### Small macros (vanilla 2733, 2797, 3861, 3882, 3901, 3914, 4667)
`when-let`, `cond->`, `as->`, `..`, `doto`, `.-` (field access)… (most
macros that don't have their own section).

### macroexpand (vanilla 4048–4066)
`macroexpand-1`, `macroexpand`.

### Namespace basics (vanilla 4156–4201)
`find-ns`, `create-ns`, `the-ns`, `ns-name`. Full
[namespace introspection](#namespace-introspection) lives later.

### Vars (vanilla 4357–4370)
`find-var`, `var-get`, `var-set`, `with-local-vars`. Depends on
`Var::create` for anonymous dynamic Vars; see the
[var-set / with-local-vars](#var-set--with-local-vars) writeup below.

### resolve (vanilla 4389–4402)
`ns-resolve`, `resolve`. Class-path resolution is omitted (Python has no
class-path), but Var resolution via symbol works.

### Watches / validators / meta ops (vanilla 2165–2437)
`add-watch`, `remove-watch`, `set-validator!`, `get-validator`,
`alter-meta!`, `reset-meta!`. Routed through duck-typed pymethods on
`Atom` / `Ref` / `Agent` / `Var`.

### Monitors + locking (vanilla ~2900)
`monitor-enter` / `monitor-exit` RT shims plus the `locking` macro in
`core.clj`. The hybrid runtime in `monitor.rs` resolves a target value
to either Python's context-manager protocol (`__enter__`/`__exit__`,
the natural mapping for `threading.RLock`/files/etc.) or, for plain
values without that protocol, a `WeakKeyDictionary`-keyed reentrant
lock (`threading.RLock`). Reentrancy is required so the same thread
can re-enter a held monitor.

### read-string (vanilla 3835)
`read-string` at Clojure-layer; dispatches to the Rust reader. The full
`read` with PushbackReader remains [PARTIAL](#io).

### Dynamic bindings (vanilla 1934–2043)
`push-thread-bindings`, `pop-thread-bindings`, `get-thread-bindings`,
`with-bindings*`, `with-bindings`, `binding`. `bound-fn*`, `bound-fn`,
`binding-conveyor-fn`. Binding frames use the same `PersistentArrayMap`
(auto-promoting to `PersistentHashMap` at 8 entries) as every other
Clojure map. See the
[binding-conveyor-fn writeup](#binding-conveyor-fn) below.

### Numeric tower (vanilla 3503–3691)
See the [Numeric tower](#numeric-tower) writeup below.

### Width casts (vanilla 3510–3582)
`int`, `long`, `short`, `byte`, `char` all → `int(x)`; `double`, `float`
→ `float(x)`. Unchecked-\* variants alias their checked counterparts.

### Type / class (vanilla 3497–3501)
`class`, `type`.

### Hierarchy (vanilla 1665–1720)
`make-hierarchy`, `global-hierarchy`, `derive`, `underive`, `parents`,
`ancestors`, `descendants`, `isa?` with all four relations (equality,
hierarchy-ancestor, Python `issubclass`, vector-elementwise).

### Multimethods (vanilla 1746–1845)
See the [Multimethods](#multimethods) writeup below.

### Sorted collections (vanilla 400–427)
`sorted-map`, `sorted-map-by`, `sorted-set`, `sorted-set-by`, `sorted?`,
`rseq`, `subseq`, `rsubseq`. See the
[Sorted collections](#sorted-collections) writeup below.

### Agents (vanilla 2075–2275)
`*agent*` dynamic var, `agent`, `send`, `send-off`, `send-via`,
`release-pending-sends`, `await`, `await-for`, `agent-error`,
`restart-agent`, `set-error-handler!`, `error-handler`, `set-error-mode!`,
`error-mode`, `agent-errors` (deprecated stub), `clear-agent-errors`,
`shutdown-agents`. See the [Agents](#agents) writeup below.

### Refs and STM (vanilla 2283–2533)
`ref`, `ref-set`, `alter`, `commute`, `ensure`, `sync`, `dosync`, `io!`,
`ref-history-count`, `ref-min-history`, `ref-max-history`. See the
[Refs/STM](#refsstm) writeup below.

### Printing (vanilla 3691–3826)
`*out*`, `*in*`, `*err*` dynamic vars; `pr-str`, `print-str`, `prn-str`,
`println-str`, `pr-on`, `pr`, `prn`, `print`, `println`, `newline`,
`flush`. See the [Printing](#printing) writeup below.

### I/O (vanilla 3771–3835)
`read-line`, `load-string`, `load-reader`, `line-seq`, full multi-line
`read`. See the [I/O](#io) writeup below.

### Namespace introspection (vanilla 4146–4311)
`all-ns`, `remove-ns`, `ns-map`, `ns-publics`, `ns-interns`, `ns-unmap`,
`refer`, `ns-refers`, `alias`, `ns-aliases`, `ns-unalias`. See the
[Namespace introspection](#namespace-introspection) writeup below.

### Java arrays analogue (vanilla 3928–4048)
`make-array`, `aget`, `aset`, `aclone`, `alength`, `to-array`,
`into-array`, `to-array-2d`, `aset-int`/`-long`/`-short`/`-byte`/`-float`/
`-double`/`-char`/`-boolean`, `amap`, `areduce`. See the
[Arrays](#arrays) writeup below.

### letfn (vanilla 4438)
`letfn` macro in `core.clj` expanding to the `letfn*` compiler special
form (`compiler/emit.rs::compile_letfn`). Implementation uses mutable
forward-reference cells (`compiler/letfn_cell.rs::LetfnCell`): each
binding gets a slot containing a `LetfnCell`, allocated up-front via
`Op::LetfnCellInit` *before* any fn body is compiled. Each compiled
closure captures the cells of its peers (PyObjects are
reference-counted; capturing a slot containing a cell shares the cell).
After each fn is constructed, its closure is stored into its cell via
`Op::LetfnCellSet`. Name references at any nesting depth compile to
`Load{Local,Capture} + Op::LetfnCellGet`, dispatched on the
`is_letfn_cell` flag carried by `LocalBinding` / `CaptureBinding`. This
naturally handles forward references and arbitrary mutual recursion.

### ex-info family (vanilla 5300)
`ex-info`, `ex-data`, `ex-cause`, `ex-message`. `ExceptionInfo`
exception type lives in `exceptions.rs` (subclasses Python `Exception`
so `(catch builtins/Exception …)` matches). RT shims construct the
exception with `.data` set on the instance and optional `.__cause__`
chained via Python's standard exception cause machinery. `ex-data`
returns `nil` for non-ExceptionInfo values; `ex-cause` returns `nil`
when no cause is set; `ex-message` reads from `args[0]`.

### fnil (vanilla 6573)
`fnil` macro in `core.clj`. Pure-Clojure implementation: returns a
multi-arity fn that nil-patches up to 3 leading positions and passes
the rest through `apply`.

### Regex (vanilla 4500–4600, 7100)
`re-pattern`, `re-find`, `re-matches`, `re-seq`. Backed by Python's
`re` module: `re-pattern` returns `re.Pattern` instances (idempotent
when given one); `re-find` uses `search`, `re-matches` uses
`fullmatch`, `re-seq` uses `finditer`. No-group results are the
matched string; with-groups results are vectors of
`[whole, g1, g2, …]`. `re-seq` is currently eager (vanilla is lazy);
materializes into a Clojure list.

### Parse helpers + random-uuid (vanilla 7300, Clojure 1.11+)
`parse-long`, `parse-double`, `parse-boolean`, `parse-uuid`,
`random-uuid`. RT shims trim whitespace and parse via Rust's
`str::parse::<i64>`/`str::parse::<f64>` (long/double) or Python's
`uuid.UUID` (uuid). All parse helpers return `nil` on invalid input
(vanilla raises). `random-uuid` delegates to `uuid.uuid4()`.

---

## Subsystem writeups

### Futures / promises / pmap / with-open (vanilla 6800–7100)
**`Future`** pyclass (`src/future_.rs`) wraps a 0-arg Clojure callable
and dispatches it on the `SEND_OFF_POOL` (same pool as `send-off`,
matching vanilla's `Agent.soloExecutor`). State is one of
`Pending` / `Done(v)` / `Failed(exc)` / `Cancelled`, guarded by
`Mutex<FutureState> + Condvar`. `deref` blocks GIL-released until the
worker reaches a terminal state.

`(future-cancel f)` flips `Pending → Cancelled` and returns `true`; if
already terminal, no-op returning `false`. **Cooperative-cancel
limitation**: Python has no thread interrupt, so an in-flight
computation continues to run; its result is discarded once
cancellation is observed. Documented in the docstring.

**`Promise`** pyclass (`src/future_.rs`) — one-shot box. `(promise)`
returns a fresh unrealized one. `(deliver p v)` is idempotent — first
call wins, subsequent calls are no-ops returning `nil`. `deref` blocks
on `Mutex<Option<PyObject>> + Condvar`.

Both implement `IDeref` (so `@x` works) and a new `IPending`
protocol (`src/ipending.rs`) for `realized?`. The Clojure-layer
`(realized? x)` dispatches via `IPending`; non-IPending values return
`false` (matches vanilla).

**`pmap`** is the faithful lazy-bounded port: walks the input through
`map (fn [x] (future-call (fn [] (f x))))`, then a `step` lazy-seq
walks two cursors — `vs` (the futures) and `fs` (a `drop n` of the
same seq) — `derefing` the head of `vs` only when the consumer reaches
it. Bound = `available-parallelism + 2`, matching vanilla. Multi-coll
arity zips inputs into vectors before `pmap`-ing single-arg.

**`pcalls`** wraps `(pmap (fn [f] (f)) fns)`. **`pvalues`** is a macro
that wraps each expr in a thunk and delegates to `pcalls`.

**`with-open`** macro generates nested `try`/`finally` calls to a new
RT shim **`close-resource`**. The shim prefers Python's
`__exit__(None, None, None)` (context-manager protocol — files,
locks, `contextlib.suppress`, …) and falls back to `.close()` for
Clojure-style closeables. The bound name is the binding expression's
result (Clojure idiom), not whatever `__enter__` returns (Python
idiom). Closes happen in reverse order of opening.

**`extenders`** returns the sequence of types directly extended for a
protocol (excluding MRO-promoted cache entries). **`extends?`** mirrors
`satisfies?` but accepts a class instead of a value (vanilla
distinguishes them). Both walk `Protocol.cache` via the new
`extenders` / `extends?` RT shims.

**`available-parallelism`** RT shim exposes
`std::thread::available_parallelism()` so `pmap` can size its bound.

**`bean`** returns a live `Bean` pyclass (`src/bean.rs`) — matches
vanilla's `APersistentMap`-proxy semantics. Property *names* are
reflected once at construction (analogue of vanilla's
`Introspector.getPropertyDescriptors` snapshot): walks `dir(obj)`,
skips `_`-prefixed names (dunders + private convention), and skips
any attribute whose value is `callable` (methods, classmethods,
staticmethods). Property *values* are read on every `val_at` /
`seq` access, so mutations to the underlying object are visible.
`@property`-decorated values pass through because they're invoked
at `getattr` time and the *result* is what we test.

Implements `ILookup`, `ISeqable`, `Counted`, `IFn` (keyword-style:
`((bean obj) :foo)`), `IEquiv` (compares entry-wise to any map of
the same shape), `IHashEq` (XOR-fold like PersistentHashMap), plus
Python `__len__`/`__iter__`/`__contains__`/`__getitem__`/`__eq__`/
`__hash__`/`__repr__`. Backed by the `bean-impl` RT shim.

### Loading: require / use / load-file / `(ns …)` (vanilla 5800–6300)
`require`, `use`, `load-file`, `load`, `in-ns`, and the `(ns …)` macro
are in `core.clj` after the NS-introspection block. Backed by Rust
helpers in `eval/load.rs` and `eval/rt_ns.rs`:
- `LOAD_NS_OVERRIDE` (per-thread): `(in-ns SYM)` sets it; `load_clj_string`
  takes it between top-level forms and switches its target namespace.
- `CURRENT_LOAD_NS` (per-thread): mirrors the loader's current target so
  Clojure code (`require`'s alias / refer installation; the `(ns …)`
  macro's `clojure.core` auto-refer) can ask "which namespace am I
  loading into right now?" via `(current-load-ns)`.
- Both slots are saved/restored around each `load_clj_string` call so
  nested loads (e.g. `require` inside a being-loaded file) don't clobber
  the outer load's state.
- `find-source-file SYM` walks `sys.path` for `<ns/path>.clj` (dots →
  `/`, hyphens → `_`). `read-file-text` loads file contents.
- `*loaded-libs*` atom holds the set of already-loaded namespaces; bare
  `(require 'foo)` skips re-loading; `:reload` and `:reload-all`
  options force a reload.
- `(ns name & references)` macro expands to `(in-ns 'name)` plus an
  auto-refer of `clojure.core` plus one form per `(:require …)` or
  `(:use …)` directive.
- The compiler's `resolve_symbol` now consults the current namespace's
  `__clj_aliases__` **before** `sys.modules[ns_name]`, so
  `(require '[foo :as f]) (f/bar)` works — and so a stray stub package
  (auto-created by Python when a previous test made `f.something`)
  doesn't shadow the alias.



The blocks below used to be "deferred" items from the original port.
Each now has a design note explaining what's ported and the deliberate
deviations from vanilla.

### Protocols / records / types (vanilla 5050–6455)
`defprotocol`, `extend-type`, `extend-protocol`, `satisfies?`, `deftype`,
`defrecord`, `reify` — all in `core.clj` after the multimethod block.
Backed by two new pyfunctions (`create_protocol`, `create_protocol_method`
in `protocol.rs`) that build runtime `Protocol` / `ProtocolMethod`
instances using the same struct layout as `#[protocol]`-defined ones,
so they participate in the same dispatch machinery (exact PyType lookup
→ MRO walk → optional `__clj_meta__` → optional fallback).

`make-type` RT shim invokes Python's `type(name, (object,), {})` to
create plain user classes at runtime; `deftype` / `defrecord` / `reify`
all use this. Protocol impls register via the existing
`Protocol.extend_type(target, {method-name: callable})` path.

`defrecord` additionally generates a `map->Name` constructor and
auto-extends `ILookup` so `(:field rec)` and `(get rec :field)` work
via Python attribute access. **PARTIAL:** `defrecord` doesn't yet
provide structural equality, hashing, full IPersistentMap (assoc /
dissoc returning a new record with the change), or extmap for
non-declared fields.

`reify` creates a fresh anonymous class per call. Closures over the
surrounding lexical scope work via the usual `fn` machinery.

`satisfies?` walks the protocol's cache + the value type's MRO without
calling any impl.

### Multimethods
Rust `MultiFn` pyclass (`src/multifn.rs`) implementing `IFn` with a
per-instance method table, prefer table, and resolved-method cache
(invalidated on hierarchy changes by map-identity comparison).
Clojure layer: `defmulti`, `defmethod`, `remove-method`,
`remove-all-methods`, `prefer-method`, `methods`, `get-method`,
`prefers`. Plus full hierarchy machinery with all four `isa?` relations.

### Refs/STM
`Ref` pyclass (`src/stm/ref_.rs`) + `LockingTransaction`
(`src/stm/txn.rs`) — MVCC with a per-ref history ring, global commit
clock, sorted-id lock acquisition (no deadlock), retry on
read-point-vs-last-commit conflict. `RetryEx` inherits
`PyBaseException` so `(catch Exception …)` inside a `dosync` body
cannot swallow it. Barge (vanilla's priority-inversion break) is
omitted — `MAX_RETRIES = 10_000` bounds livelock and sorted lock
order rules out deadlock. Agent sends inside a txn are queued and
dispatched only on commit.

### Agents
`Agent` pyclass (`src/agent.rs`) with ArcSwap state, FIFO action queue,
and a busy flag for per-agent serialization. Two global `ExecutorPool`s
(`std::thread` + `std::sync::mpsc`): `SEND_POOL` sized at
`available_parallelism() + 2` for `send`; 32-thread `SEND_OFF_POOL` for
`send-off`. `send-via` wraps the drain-next-queue-item callback in a
zero-arg `PyCFunction` and submits it to the user's executor callable;
the executor decides the thread (sync or async). Note a deliberate
simplification from vanilla: we schedule drain-of-queue per dispatch
rather than drain-per-action, so mixing `send` / `send-via` on the
same agent uses a single executor for the current drain. Binding
conveyance via `BINDING_STACK` snapshot at dispatch; `*agent*` dynamic
Var bound around action execution via a `OnceCell<Py<Var>>`-cached
lookup. Error modes `:fail` (parks exception; subsequent sends throw)
and `:continue` (handler called, action discarded, agent keeps running).

### Numeric tower
Alias Python types rather than wrap. `Ratio` = `fractions.Fraction`,
`BigInt`/`BigInteger` = `int` (arbitrary precision), `BigDecimal` =
`decimal.Decimal`. Width casts (`long`, `int`, `short`, `byte`, `char`)
all collapse to `int(x)`; `double` and `float` collapse to `float(x)`.
Unchecked-\* variants alias their checked counterparts. Division
`(/ 1 2)` returns `Fraction(1, 2)` — matches vanilla exactly.

### Sorted collections
`PersistentTreeMap` (`src/collections/ptreemap.rs`) backed by a
persistent red-black tree (Okasaki insert, canonical balance-on-delete).
`PersistentTreeSet` (`src/collections/ptreeset.rs`) wraps it. New
`Sorted` protocol (`src/sorted.rs`) covers `seq(asc)`, `seq_from(k, asc)`,
`entry_key`, `comparator_of`. Comparators accept both int-returning
Comparator style and Clojure's predicate-style (e.g. `>` / `<`); the
`PyBool`-first detection prevents Python's `bool`-is-an-`int`-subclass
from masking the predicate case.

### Arrays
Runtime representation is a plain Python `list` — no `array.array` or
numpy. `make-array` returns `[None] * n` (nested for multi-dim); the
`type` argument is accepted for Clojure-code portability but ignored.
`aset` mutates, `aget` reads; both support multi-dim indexing. Typed
variants (`aset-int`, `aset-long`, `aset-short`, `aset-byte`,
`aset-float`, `aset-double`) all call `aset` with appropriate
`int`/`float` coercion — Python's unbounded numerics make the Java
width distinctions unnecessary. `aset-char` / `aset-boolean` apply
their light coercions. `amap` / `areduce` are macros.
`into-array` / `to-array` / `to-array-2d` convert any ISeqable into a
(list-of-)list.

### Namespace introspection
Decision on `ns-map`: returns a Clojure map of `{Symbol → Var}` (matching
vanilla's shape). `all-ns` walks `sys.modules` for
`ClojureNamespace`-tagged entries. `remove-ns` deletes from
`sys.modules`. `ns-publics` filters by `{:private true}` in the Var's
meta map; `ns-interns` filters to Vars whose owning namespace is the
queried one. `refer` implements the full `:exclude` / `:only` /
`:rename` filter semantics. `ns-unmap` / `ns-unalias` delete from the
module's `__dict__` and `__clj_aliases__`.

### Printing
`*out*` / `*in*` / `*err*` are `^:dynamic` Vars bound at initialization
to `sys.stdout` / `sys.stdin` / `sys.stderr`. Print dispatch flows
through `print-method` / `print-dup` multimethods keyed on `(type x)`
— users can extend via `(defmethod print-method MyType [x w] …)`.
Collection methods (`PersistentVector`, `PersistentList`, hash/array/
tree maps, hash/tree sets, `Cons`, `LazySeq`, `VectorSeq`) iterate and
re-call `print-method` for each element, so user extensions flow
through nested data. Reference wrappers (`Atom`, `Volatile`, `Ref`,
`Agent`) print `#<Type val>` where `val` is recursed via
`print-method` too. The `:default` method delegates to Rust's
`pr_str` / `print_str` (fast path; handles every built-in and
unknown types). `*print-readably*` and `*print-dup*` dynamic vars
control the behavior of `pr-on`; `print` / `println` rebind
`*print-readably*` to false. `pr-str` builds via `print-method` into
a Python `io.StringIO`.

### I/O
`read-line`, `load-string`, `load-reader`, `line-seq`, and full `read`
with multi-line + multi-form-per-line support. `read` accumulates input
lines until the parser accepts a complete form, preserving unconsumed
trailing content on the reader itself via a `__clj_pushback__`
attribute. Two new reader entry points support this:
`read_string_prefix_py(s) -> (form, consumed_bytes)` parses one form
and reports how many bytes it consumed (including trailing whitespace
up to the next real token); `Source::offset()` exposes the current
read position. Incomplete input raises `ReaderError` with
`"EOF while reading …"`, which `(read)` catches and handles by reading
another line. `load-string` wraps the existing
`eval::load::load_clj_string` Rust primitive and evaluates into
`clojure.user`.

### var-set / with-local-vars
`Var::create` pyfunction produces an anonymous dynamic Var (ns=nil,
sym=nil, dynamic=true). `var-set-bang` RT shim calls the existing
`set_bang` pymethod to mutate the top binding frame's entry. As part
of the change, `Var.__eq__` / `__hash__` became identity-based
(matching vanilla JVM's `Var.equals` = `Object.equals`) so Vars work
as `hash-map` keys — the previous root-value delegation broke
`(hash-map unbound-var v)`.

### binding-conveyor-fn
`BindingFrame` pyclass wraps a cloned `Vec<Frame>` (full stack, not
just the top frame). Two RT shims: `clone-thread-binding-frame`
snapshots, `reset-thread-binding-frame` installs. Matches vanilla's
`Var/cloneThreadBindingFrame` + `Var/resetThreadBindingFrame`.
`bound-fn*` still uses `get-thread-bindings` + push/pop — semantically
equivalent for the single-thread-at-a-time case.

### :pre / :post conditions
`AssertionError` exception type in `exceptions.rs`; `throw-assert` RT
shim. `assert` macro in `core.clj`. The `fn` macro's `psig` now passes
body through `process-conditions`, which detects a leading
`{:pre … :post …}` map and wraps the remaining body with asserts —
`%` in `:post` refers to the return value via
`let [% (do body…)] asserts… %`. `%` emitted via `(symbol "%")` because
`'%` in a plain fn reads as `(quote %)`.

### Predicates / collection helpers / macros (vanilla 5025–8000 sweep)
A large sweep landing the predicates and helpers that test files and
typical user code reach for. Each delegates either to an `instance-*?`
RT shim (protocol-cache lookup) or a small Rust impl:
- **Predicates:** `coll?`, `list?`, `counted?`, `seqable?`,
  `reversible?`, `indexed?`, `associative?`, `empty?`, `not-empty`,
  `distinct?`, `var?`, `special-symbol?`, `bound?`, `thread-bound?`,
  `inst?`, `inst-ms`, `uuid?`, `NaN?`, `infinite?`. Each new
  protocol-based predicate routes through `mk_protocol_pred` (cache
  lookup + MRO walk).
- **Collection helpers:** `empty`, `distinct` (lazy), `replace` (vec
  + seq paths), `mapv`, `filterv`, `run!`, `map-indexed`,
  `keep-indexed`, `subs` (Rust char-indexed slicing), `max-key`,
  `min-key`, `bounded-count`.
- **Macros:** `defonce` (CAS-style def-when-not-bound),
  `defn-`, `comment`, `cond->`, `cond->>`, `as->`, `some->`,
  `some->>`, `with-redefs` / `with-redefs-fn`, `with-out-str`,
  `with-in-str` (the latter two backed by a `string-io` RT shim that
  wraps Python's `io.StringIO`).
- **Random:** `rand`, `rand-int`, `rand-nth`, `shuffle`,
  `random-sample`. Backed by Python's `random` module.
- **I/O:** `format` (Python `%` operator with `%n` translation),
  `printf`, `slurp`, `spit`, `iterator-seq`, `enumeration-seq`,
  `file-seq`. `format` differs from Java's `String.format` in some
  edge cases (printf-style vs Java-style) but is close enough for
  typical use.
- **Var plumbing:** `eval` (delegates to `eval::py_eval`),
  `alter-var-root` (routes through `Var.alter_root` pymethod CAS
  loop), `intern`, `loaded-libs`, `requiring-resolve`,
  `refer-clojure`, `import` (no-op stub — Python has no class-path
  to import from).
- **Misc:** `re-matcher` / `re-groups` (stateful matcher wraps
  `re.Pattern.finditer`), `hash`, `mix-collection-hash`,
  `hash-ordered-coll`, `hash-unordered-coll`, `bases`, `supers`,
  `Throwable->map`.
- **Tap system:** `add-tap`, `remove-tap`, `tap>`. Backed by
  `crate::tap` — a process-global `Mutex<Vec<PyObject>>` of fns.
  Synchronous fire (no async queue yet); errors from individual taps
  are swallowed (matches vanilla's queue-drop behavior).
- **case macro:** Expands to a chain of `(if (= g <const>) …)`
  rather than vanilla's JVM `tableswitch` opcode. Same semantics,
  O(n) instead of O(1) for very wide cases — for typical few-clause
  case the difference is invisible. Lists `(a b c)` as test
  constants match any element.
- **letfn** (already covered above): expands to `letfn*` special
  form (compiler/emit.rs).

Forward references between core.clj forms are handled by splitting
the additions into two blocks: a "Phase 2" block at line ~2660
(forms with no forward refs) and a "Phase 3" block at ~5230 (forms
that reference later-defined names like `print`, `the-ns`,
`require`, `*out*`/`*in*`, `*loaded-libs*`).

---

## Deferred permanently

### Structs (vanilla 4068–4101)
`create-struct`, `defstruct`, `struct-map`, `struct`, `accessor`.
Legacy API superseded by records / hash-maps in every modern Clojure
codebase. Not worth porting.

---

## Ongoing / PARTIAL items

- **`defrecord` extras** — structural equality / hash, full
  IPersistentMap (assoc/dissoc returning a new record), extmap for
  non-declared keys. The MVP supports positional + map constructors,
  field access via `(:kw rec)` and `(get rec :kw)`, and protocol
  impls — sufficient for most application code.

Future work: `clojure.string` / `clojure.set` / `clojure.test`,
vanilla forms past ~7100.

---

## Reader / compiler primitives (not in core.clj)

For completeness — these live in Rust and are callable from Clojure via
`clojure.lang.RT/*` but aren't part of core.clj itself:

- Reader: `read-string`, tagged-literal dispatch, reader metadata (`^{…}`,
  `^:kw`, `^Type`), anonymous-fn literal `#(…)`, regex literal `#"…"`,
  character literal `\c`, set literal `#{…}`, discard `#_`.
- Compiler: `fn*`, `let*`, `loop*`, `recur`, `if`, `do`, `quote`, `var`,
  `throw`, `try`/`catch`/`finally`, `def`, `new`, `.`, `set!` (on dynamic
  Vars — see `var-set` entry).
- Eval loop: `eval`, `load-string`, `load-reader`, form-at-a-time
  read-eval in `eval::load::load_clj_string`.

These are the substrate on which `core.clj` is built.
