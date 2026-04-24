# Module Port Audit

Analysis of `.clj` files in the reference Clojure source tree (`~/oss/clojure/src/clj/clojure/`) outside of `core.clj`, sorted by portability and usefulness for clojure-py.

Methodology: LOC count + JVM-interop density (`:import`, `java.*`, `.lang.`, `Thread`, `System`, `StringBuilder`, etc.) cross-referenced against API value for a Clojure-on-Python runtime.

## Tier S — port next, high leverage, low JVM surface

Near-pure Clojure; highest-value public APIs beyond `core`.

| File | LOC | Notes |
|---|---|---|
| `walk.clj` | 131 | Only 2 JVM refs (`IMapEntry`, `MapEntry/create`, `IRecord`) — swap to your protocols. `postwalk`/`prewalk` are ubiquitous. |
| `set.clj` | 181 | Zero JVM. One-to-one port. |
| `zip.clj` | 318 | Pure; only `(new Exception ...)` — map to Python `Exception`. |
| `string.clj` | 377 | Heavy-but-shallow JVM via `java.util.regex.Pattern/Matcher`, `StringBuilder`. Retarget to Python `re` + str. High usefulness. |
| `math.clj` | 523 | Thin wrapper over `java.lang.Math` statics. Mechanical swap to Python `math`/`cmath`. |
| `test.clj` | 830 | Only 2 JVM touches (`StackTraceElement`, `Throwable`). Testing framework — essential for the ecosystem. |
| `template.clj` | 55 | Zero JVM. Tiny; useful for `clojure.test/are`. |

## Tier A — port after Tier S, small but valuable

| File | LOC | Notes |
|---|---|---|
| `uuid.clj` | 20 | Just a `default-data-readers` entry; trivial once you have a Python UUID type. |
| `edn.clj` | 45 | Skeleton that delegates to `EdnReader`; your own Rust reader fills the gap. |
| `stacktrace.clj` | 87 | `Throwable`-centric; rewrite for Python exceptions — short, high user-visibility. |
| `data.clj` | 143 | `diff` extends to `java.util.{Set,List,Map}` — redirect to Python collections + your persistents. |
| `datafy.clj` | 62 | Alpha but small; couples to `core/protocols.clj`. |
| `core/protocols.clj` | 201 | `CollReduce`/`InternalReduce`/`Datafiable`/`IKVReduce`. High perf payoff for `reduce`. Only `java.util.Iterator` coupling → Python iter protocol. |
| `core_print.clj` | 586 | Likely partially absorbed into the printer crate already; what remains is per-type `print-method` dispatch. Moderate reuse value. |

## Tier B — portable but needs real Python rewrites

| File | LOC | Notes |
|---|---|---|
| `instant.clj` | 294 | Date/time parsing against `java.util.{Calendar,Date,TimeZone}`. Swap to `datetime` + `zoneinfo`. Medium effort. |
| `xml.clj` | 150 | Simple SAX-style parser; rewrite on `xml.sax` or `xml.etree`. |
| `test/tap.clj` | 123 | TAP reporter, mostly pure. |
| `test/junit.clj` | 195 | JUnit XML writer — has a `StringWriter` or two; doable. |

## Tier C — large and interesting, but significant rewrite

| File | LOC | Notes |
|---|---|---|
| `pprint.clj` + `pprint/*` | ~3200 total | Huge value, but built on mutable JVM writers. `cl_format.clj` alone is 1949 lines of pure-but-stateful Clojure. Port incrementally (pretty-writer first, then cl-format). |
| `repl.clj` | 289 | `doc`, `apropos`, `demunge`, stack filters. Many pieces depend on compiler naming/metadata. |
| `main.clj` | 676 | `python/clojure/repl/core.py` already owns this; cherry-pick `repl`/`load-script`/arg-parse semantics rather than port. |
| `core/server.clj` | 341 | Socket REPL — rewrite on Python `socket`/`socketserver`, retaining `start-server`/`stop-server` API. |
| `core_deftype.clj` | 919 | Defines `deftype`/`defrecord`/`definterface` macros. Needs reimplementation against Python classes + protocol machinery — high usefulness, not a line-for-line port. |

## Tier D — concept is portable, implementation isn't

| File | LOC | Notes |
|---|---|---|
| `core/reducers.clj` | 334 | Map/filter/cat transformers port cleanly; `fold`'s ForkJoinPool parallelism doesn't (GIL). Ship the reducer fns, drop/stub `fold` (or back by `concurrent.futures`). |
| `reflect.clj` + `reflect/java.clj` | 390 | Rewrite against Python `inspect` as `clojure.python.reflect`; keep the `Reflector` protocol but replace the whole backend. |
| `gvec.clj` | 566 | Primitive-typed vectors (`vector-of :long` …). No direct Python analog; rebuild on `array.array` or numpy, with semantics deliberately narrowed. |

## Unportable — skip entirely

| File | Why |
|---|---|
| `genclass.clj` (753) | Emits JVM bytecode for AOT classes. No Python analog — different mechanism entirely (`type()`, metaclasses). |
| `core_proxy.clj` (443) | Emits JVM proxy classes. Replace with plain Python subclassing semantics via a different macro. |
| `inspector.clj` (189) | Swing `JTree` GUI. Dead. |
| `parallel.clj` (249) | Deprecated even on JVM (JSR-166y pumpkin). |
| `java/io.clj`, `java/shell.clj`, `java/process.clj`, `java/browse*.clj`, `java/javadoc.clj`, `java/basis.clj`, `java/basis/impl.clj` | JVM-native plumbing. Build `clojure.python.{io,shell,process,…}` as parallel namespaces — don't port. |
| `repl/deps.clj`, `tools/deps/interop.clj` | tools.deps/Maven-coupled. |
| `reflect/java.clj` | See reflect above — backend is JVM-specific. |

## Suggested order of attack

1. `walk` → `set` → `template` → `zip` (trivial, unblocks test/data code).
2. `string` + `math` (high user demand, mechanical).
3. `test` + `test/tap` (unlocks ported test suites from the Clojure repo).
4. `core/protocols` + `core/reducers` (no-fold subset) + `datafy` + `data` (protocol-shaped, fits the "protocols-first" strategy).
5. `stacktrace`, `edn`, `uuid`, `instant` (small wins).
6. `core_print` cleanup against what's already in the printer crate.
7. `pprint` (large, do last among the worthwhile ports).
8. Treat `core_deftype`, `core_proxy`, `genclass`, `reflect`, `java/*`, `core/server`, `main`, `repl` as **reimplement-native** rather than port.
