# Embedded Clojure source

`clojure/core.clj` is the clojure-py port of Rich Hickey's canonical
`~/oss/clojure/src/clj/clojure/core.clj`. It is:

- Embedded into the compiled `.so` at build time via `include_str!` from
  `crates/clojure_core/src/eval/load.rs`.
- Loaded at module init, immediately after Rust-side `clojure.core` shims and
  the `clojure.lang.RT` primitive namespace are populated.

Forms are added in source order. Java-specific interop — `clojure.lang.RT/x`,
`clojure.lang.Util/y`, `(.method obj)` on JVM classes — is translated to calls
into our `clojure.lang.RT` namespace (see `eval/rt_ns.rs`).

When editing this file, rebuild the extension (`maturin develop`) to re-embed.
