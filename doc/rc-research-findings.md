# Recent Research on Reference Counting: Closing the Gap with Tracing GC

A survey of research directions that bring reference-counting (RC) performance in line with tracing garbage collectors, with a concrete application to a Clojure-like language.

---

## Part 1: The Research Landscape

The last decade has essentially closed the RC-vs-GC performance gap for certain language designs. Four major research threads carry most of the weight.

### The Blackburn/Shahriyar line — "RC is competitive"

The turning point was Shahriyar, Blackburn, and McKinley's work on Jikes RVM, culminating in Shahriyar's 2015 thesis *High Performance Reference Counting and Conservative Garbage Collection*.

Their argument: with the right design choices, RC can match the best tracing collectors. The payoff was **RCImmix**, which replaces RC's traditional free-list heap with Immix's line-and-block structure, adds copying to fight fragmentation, and combines that with a set of RC-specific optimizations:

- Lazy mod-buf insertion
- Born-dead optimizations for nursery-age objects
- Coalescing of adjacent reference updates
- Sticky mark bits for old objects

The claim — backed by benchmarks — is that these optimizations closed the ~30% gap with tracing entirely, and RCImmix actually beat a tuned production generational collector.

**Key papers:**
- Shahriyar, *High Performance Reference Counting and Conservative Garbage Collection* (PhD thesis, 2015)
- Shahriyar, Blackburn, McKinley, "Taking Off the Gloves with Reference Counting Immix" (OOPSLA 2013)
- Shahriyar, Blackburn, Frampton, "Down for the Count? Getting Reference Counting Back in the Ring" (ISMM 2012)

### The Perceus / Koka / Lean 4 line — language-design RC

This is where most of the recent excitement lives. The insight: if you design the *language* around RC — immutable ADTs, explicit control flow, no cycles by construction — you get radically better results than retrofitting RC onto a general-purpose language.

**Perceus** (Reinking, Xie, de Moura, Leijen, PLDI 2021) is the keystone. It emits precise RC instructions such that cycle-free programs are "garbage free" — only live references are retained. This enables reuse analysis with guaranteed in-place updates at runtime. The broader programming model is called FBIP (Functional But In-Place): writing purely functional code that executes with the allocation profile of imperative code.

Lean 4's RC (Ullrich and de Moura, 2019) pioneered the core ideas; Perceus formalizes and extends them.

Key mechanisms:
- **Borrow inference + thread-local unsharing.** Non-atomic ops for un-shared objects, atomic ops (via sign bit on the count) for shared ones; compile-time borrow inference skips dup/drop pairs entirely.
- **Reuse pairing.** A deallocation of a size-N object adjacent to a fresh allocation of size N becomes an in-place mutation if refcount == 1 at runtime.

Follow-on work:
- **FP² / Fully In-Place** (Lorenzen, Leijen, Swierstra, ICFP 2023). A linear calculus where qualifying functions provably need zero allocation and constant stack. Merge sort, quicksort, splay trees, finger trees all fit.
- **Drop-guided / frame-limited reuse** (Lorenzen and Leijen, 2023). Replaces Perceus's original reuse analysis with something more robust to program transformations, with tighter bounds on peak heap usage.
- **First-class constructor contexts** (Lorenzen, Leijen, Swierstra, Lindley, PLDI 2024). Top-down tree algorithms (splay, zip trees) matching hand-written C performance, verified in Iris/Coq.
- **Destination Calculus** (Bagrel and Spiwack, OOPSLA 2025). A linear λ-calculus that bakes in destination-passing style (memory writes as first-class values) rather than relying on reuse analysis to rediscover it.
- **First-Order Laziness** (Lorenzen, Leijen, Swierstra, Lindley, ICFP 2025). Compiling laziness in a Perceus-style RC setting.

### Biased and split-count RC — for general-purpose languages

For languages like Swift where atomics dominate the cost profile:

- **Biased Reference Counting** (Choi, Shull, Torrellas, PACT 2018). Two counters per object; bias each object toward a specific owner thread. Owner updates non-atomically; other threads use an atomic slow-path counter. Measured ~22.5% average speedup on Swift clients and 7.3% throughput gain on servers.
- **Dynamic Atomicity** (Ungar, Grove, Franke, DLS 2017). Similar spirit — dynamically replaces atomic RC ops with non-atomic ones via a store barrier that detects escaping objects.
- The Rust `hybrid_rc` crate implements biased RC, leveraging `!Send` to avoid extra bookkeeping.

A relevant hardware note: on Apple Silicon and Zen 3+, uncontended atomic increments are nearly free, which has shifted the cost model. Modern hardware has quietly eaten a chunk of the atomic-RC overhead.

### Hybrid approaches

- **Ulterior Reference Counting** (Blackburn and McKinley, OOPSLA 2003). Copying nursery + RC on mature heap. Exploits the weak generational hypothesis to avoid RC's pathological behavior on short-lived objects. Still cited as a reference design point.
- **Age-oriented collection** (Paz et al.) and on-the-fly sliding views cycle collectors extend this for concurrent settings.
- Bacon's "A Unified Theory of Garbage Collection" (OOPSLA 2004) remains the best framing: tracing and RC are duals in a design space, and every real collector is a hybrid.

### The broader pattern

To match tracing performance with RC, you need two of three things:

1. A language that statically rules out cycles and helps eliminate count ops (Koka/Lean route)
2. A heap layout that gives you locality and compaction (RCImmix route)
3. Runtime tricks to avoid atomics on the fast path (biased RC route)

The Koka work is the most interesting direction if you care about the interaction between language design and collector, since "free in-place mutation for functional code" is a genuinely new capability rather than just a cost reduction.

---

## Part 2: Application to a Clojure-like Language

A Clojure-like language sits in an interesting spot — further along toward "Perceus-friendly" than Java/C#/Python, but not quite where Koka and Lean live. Cycles being rare but not statically impossible rules out the strongest Perceus guarantees, but most of the machinery still applies.

### What you get almost for free

**RCImmix-style heap layout** is language-agnostic. Line-and-block allocation, copying to fight fragmentation, sticky mark bits. Nothing about Clojure semantics makes this harder. Probably the single biggest throughput win, and orthogonal to everything else.

**Biased RC** is also essentially a no-brainer. The semantics map almost perfectly: most values live on one thread; `atom` / `ref` / `agent` / `promise` / channels are the explicit escape hatches that mark objects as shared. You get a clean static signal for when to flip an object to atomic mode — any write into one of those reference types is a "now shared" event. The Swift BRC numbers (~20% on clients) probably underestimate the win here, because Clojure does more small-object allocation and has cleaner thread-local/shared boundaries than ObjC/Swift's messy heap.

### Perceus-style reuse — where it gets interesting

This is where the Clojure model both helps and hurts.

**It helps:** core data — lists, vectors, maps, sets — are immutable ADTs. Cons cells, HAMT nodes, RRB tree nodes all have fixed layouts, so reuse pairing is structurally trivial. A path-copy on `(assoc m k v)` touches ~log₃₂(n) nodes; if the map is unique, every allocation becomes an in-place mutation. That's the FBIP dream applied to HAMT updates, same shape as Koka's red-black-tree result.

**It hurts:** persistent data structures are literally designed to be shared. The whole point of a HAMT is that ten threads hold references to the same root and each does `assoc` without interfering. Uniqueness is the exception, not the rule. Reuse analysis fires constantly in local scratch code (building up a map in a loop, threading through `reduce`) but rarely in code actually leveraging Clojure's persistent story.

A crucial observation: **Clojure already has a manual version of exactly this — transients.** `(transient m)` / `conj!` / `persistent!` is a programmer-managed uniqueness annotation. An RC-based Clojure could make transients unnecessary — the runtime automatically picks the transient path whenever refcount == 1. That's a genuine language simplification, not just a performance tweak.

### Cycles: the Bacon backstop

Cycles rare but not statically impossible means you can't claim "garbage free" in the Perceus sense. But you can do what Python, PHP, and pre-ARC Objective-C did: run Bacon's trial-deletion cycle collector periodically, traversing only candidate roots (objects whose count was decremented to a nonzero value). When cycles are genuinely rare, this runs almost never and its cost is bounded.

Practical cycle sources to plan for:
- Closures over atoms that reference the closure (callback-into-state pattern)
- Mutually recursive lazy sequences
- User-built graph structures

You'd want weak references as a first-class citizen (not just JVM interop), and the cycle collector tunable — probably age-triggered rather than allocation-triggered, since cycles in this style of code tend to form and persist rather than churn.

### Lazy sequences: RC is a real win

Clojure's lazy seqs are notorious for retention bugs under the JVM GC. RC actually helps here: once the head is consumed and no one holds a reference, it dies immediately instead of waiting for the next GC cycle. "Holding onto the head" becomes less catastrophic — memory pressure shows up quickly and locally rather than as a mystery OOM an hour later.

The Lorenzen/Leijen "First-Order Laziness" paper (ICFP 2025) is directly relevant here.

### Dynamic typing is the real tax

The structural disadvantage versus Koka/Lean: without static types, many Perceus optimizations degrade.

- Can't statically know a value is a Cons and will be freed here, so can't pair its deallocation with an allocation three lines down.
- Can't do borrow inference as aggressively — without precise function signatures, can't prove a parameter is only read and skip dup/drop.

You can recover a lot of this with:
- Type inference / flow analysis at compile time (think Typed Clojure, Jank)
- Shape-based specialization (a Cons is a Cons at the pattern-match point; propagate backward)
- Inline caches / PICs learning representations at runtime, V8/LuaJIT-style, specializing RC ops per call site

Lean 4 pulls this off partly because of dependent types and a disciplined IR; Koka because of its effect system. A dynamic Clojure loses some static wins but makes them back at runtime if you're willing to JIT.

### STM and shared mutable refs

The trickiest piece. Clojure's `ref` + `dosync` assumes tracing — the STM holds versioned snapshots that are casually collected. Under RC:

- Every `ref-set` writes a reference, triggering dup/drop on old/new values
- Every abort-and-retry in an STM transaction does the same

You'd want:
- Ref machinery to batch RC ops across transaction commit rather than per-write
- Refs themselves as "always-shared" (atomic counter) slow path

Not fatal, but the part where you'd have to do real engineering rather than cherry-pick existing research.

---

## Part 3: A Design Sketch

If I were sketching an RC-based Clojure-alike on a napkin:

1. **RCImmix heap** with bump-pointer allocation into lines
2. **Biased RC** with thread-local non-atomic fast path; `atom` / `ref` / channel writes flip to atomic
3. **Perceus-style dup/drop insertion** with borrow inference wherever type inference gives enough info; conservative fallback otherwise
4. **Runtime reuse analysis**: dup-1 drop pairs with matching size → mutate in place. Automatic transients.
5. **Bacon trial-deletion cycle collector**, age-triggered, narrow scope (candidate roots only)
6. **Weak references** as a first-class language feature with obvious syntax
7. **Batched RC ops** across STM transaction commits

This combination steals from every thread and plays to Clojure's strengths (immutable ADTs, clear thread-boundary annotations, rare cycles). The honest weak spot: persistent-with-sharing code won't get the dramatic FBIP wins seen in Koka benchmarks, because the whole point of that code is that it's shared. But scratch/local code and the "fluent builder" idiom (threading through `->` or `reduce`) would benefit enormously — probably 60-70% of real Clojure code by lines.

---

## Reading List

If starting from zero, read in this order:

1. **Shahriyar's thesis** (2015) — most comprehensive treatment of "RC without language-design escape hatches." Closest to the Clojure situation.
2. **Perceus** (Reinking et al., PLDI 2021) — the modern design-centric view.
3. **Bacon's Unified Theory** (OOPSLA 2004) — the framing that makes everything else make sense.
4. **Biased Reference Counting** (Choi et al., PACT 2018) — the atomic-elimination playbook for multi-threaded RC.
5. **FP²** (Lorenzen et al., ICFP 2023) and follow-ups — the current frontier of language-design RC.

Daan Leijen's Microsoft Research page is the single best index for the Koka/Perceus/FBIP thread.
