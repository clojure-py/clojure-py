# Evaluator Implementation Plan

**Goal:** Tree-walking evaluator that runs Clojure forms from the reader.

**Architecture:** Single `eval(form, env)` function dispatches on form kind; special-form table + hardcoded macro table + plain-invocation fallback. Closures capture env.

**Tech:** Existing collections, reader, protocols, IFn. Adds `Fn` pyclass + core arithmetic/equality/core-seq wrappers.

**Spec:** `docs/superpowers/specs/2026-04-22-evaluator-design.md`

---

## Phase E1: Scaffolding + atom eval + quote + if + do + let

`eval` + `eval_string` pyfunctions. Atoms self-evaluate. `quote/if/do/let` special forms. Symbol resolution for locals only (ns resolution in E3). No invocation yet.

**Files:**
- Create: `crates/clojure_core/src/eval/{mod.rs, env.rs, special_forms.rs, errors.rs, resolve.rs}`
- Modify: `crates/clojure_core/src/lib.rs` — add `mod eval;` + `eval::register(py, m)?;`
- Create: `tests/test_eval_basic.py`

**Deliverables:** `eval_string("42") == 42`, `eval_string("(if true 1 2)") == 1`, `eval_string("(let [a 1] a)") == 1`.

---

## Phase E2: Invocation + Fn pyclass + closures + symbol resolution via ns

`fn` special form creates Fn pyclass. Function call: eval head + args, invoke via rt::invoke_n. Symbol resolution extended to look up in current_ns after locals miss.

**Files:**
- Create: `crates/clojure_core/src/eval/{fn_value.rs, invoke.rs}`
- Modify: `eval/special_forms.rs` (fn), `eval/resolve.rs` (ns fallback), `eval/mod.rs`
- Create: `tests/test_eval_fn.py`

**Deliverables:** `((fn [x] (* x 2)) 21) == 42`, closures capture env.

Also need: expose `*` / `+` / `-` / `=` / `<` etc. as Python callables in `clojure.core` ns so tests can use them.

---

## Phase E3: `def` + Var resolution + core.clj shims

`def` interns in current-ns. Symbol resolution finds the var, derefs its root. Create a `clojure.core` ns at init with: `+ - * / = < > <= >= not list vector hash-map hash-set str first rest cons seq count inc dec nil?`. Each wraps Python or rt:: helpers as an IFn.

**Files:**
- Modify: eval/special_forms.rs (def), resolve.rs (ns resolution)
- Create: `crates/clojure_core/src/eval/core_shims.rs` — creates clojure.core ns + interns the shim fns
- Create: `tests/test_eval_def.py`

**Deliverables:** `eval_string("(def x 42)")`, `eval_string("x")` → `42`, recursive fns via Var resolution.

---

## Phase E4: Hardcoded macros (defn, when, cond, or, and)

Macroexpansion layer runs before special-form dispatch.

**Files:**
- Create: `crates/clojure_core/src/eval/macros.rs`
- Modify: `eval/mod.rs` (check macro table first)
- Create: `tests/test_eval_macros.py`

**Deliverables:** `(defn inc [x] (+ x 1))`, `(when cond body)`, `(cond a b c d)`, `(or a b)`, `(and a b)` all work.

---

## Phase E5: Integration + hypothesis fuzz

**Files:**
- Create: `tests/test_eval_integration.py` (multi-line programs: fact, fib)
- Create: `tests/test_eval_fuzz.py` (hypothesis — arithmetic expression round-trip)

**Deliverables:** `(def fact (fn [n] (if (= n 0) 1 (* n (fact (- n 1))))))`, `(fact 10) == 3628800`. Fuzz: `eval_string(pr_str(read_string(src)))` idempotent for generated arithmetic.

---

## Execution Status

All 5 phases landed. **Full test suite passing.**

| Phase | Description | Commit |
|---|---|---|
| E1 | Scaffolding + atom eval + quote/if/do/let | `466a5b4` |
| E2 | Fn + invocation + closures + ns resolution | `fbcaff0` |
| E3 | def + var + clojure.core shims | `d3246ca` |
| E4 | Hardcoded macros (defn/when/when-not/cond/or/and) + reader-fuzz fix | `aa41a77` |
| E5 | Integration tests + hypothesis fuzz + README | (this commit) |

**Deferred** (explicit non-goals — future specs):

- `loop`/`recur` with TCO
- `try`/`catch`/`throw`/`finally`
- Python interop (`.method`, `(new Class)`, `(py/fn args)`)
- Multi-arity `fn` / overloads
- Destructuring in `let` / `fn` params
- User-defined `defmacro`
- `letfn`
- Trampoline TCO
- Multimethods (`defmulti`, `defmethod`)
- Records / `deftype` / `reify`
- `core.clj` bootstrap (full Clojure stdlib on top of the shims)

**What works end-to-end:**

- Recursive function definitions (factorial, fibonacci) via Var resolution
- Closures capturing enclosing env
- Higher-order functions (compose, make-adder)
- Data manipulation (vectors, maps, keyword-as-fn, seq ops)
- All hardcoded macros nest correctly within each other
