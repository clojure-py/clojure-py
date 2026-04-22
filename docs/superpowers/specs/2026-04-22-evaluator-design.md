# Tree-Walking Evaluator — Design Spec

**Date:** 2026-04-22
**Status:** Draft for implementation
**Scope:** Fourth sub-project of the clojure-py revival. Turns reader-produced forms into running code. Enough Clojure to `(def x 1)` / `(defn f [x] (* x 2))` / `(f 21)` → `42`.

---

## 1. Goal

`eval(read_string("(+ 1 2)"))` → `3`. More importantly:

```python
read_string("(def inc [x] (+ x 1))")  # → some form
eval(form)                             # defines inc in the current ns
eval(read_string("(inc 41)"))          # → 42
```

After this spec, Python users can run Clojure source.

**Core design tenets:**

1. **Tree-walking interpreter** over the form data. No bytecode compilation in this spec (Cranelift JIT is a far-later spec). Iterate the AST directly.
2. **Minimum viable special forms** — what's needed for `defn`, `if`, `let`, lambda, recursion, basic interop. Skip `loop/recur` (add in a polish spec), skip `try/catch/throw` (skip initially; throw if encountered), skip `new`/`.` (Java interop not applicable on PyO3; use `(py/.method obj arg)` style in a later spec).
3. **Lexical closures via environment chains.** Each `fn` captures its local environment as an immutable `PersistentHashMap<Symbol, Var-or-value>`.
4. **Python interop at function call boundary.** Any callable (Python fn, IFn) can be invoked via `rt::invoke_n`.
5. **Macroexpansion.** Built-in macros (`defn`, `when`, `cond`, `or`, `and`) expand to core forms before eval. Start with these hardcoded; user-defined `defmacro` is a later spec.

---

## 2. Scope

### In scope

**Special forms:**
- `quote` — returns the form unevaluated
- `if` — 2- or 3-arity conditional (Clojure truthiness: only `nil`/`false` are falsy)
- `do` — sequential eval; return last value
- `let` — binds pairs then evals body with bindings
- `fn` — creates a closure (single-arity; multi-arity deferred)
- `def` — interns a Var in the current ns
- `var` — resolves a symbol to its Var
- `.` / `new` / `throw` / `try` / `recur` / `loop` — **deferred**, raise if encountered
- `set!` — on a dynamic Var's thread-local binding only (already impl'd in core-abstractions)

**Built-in macros** (hardcoded expansion, not user-extensible yet):
- `defn` → `(def name (fn [args] body...))`
- `when` → `(if cond (do body...) nil)`
- `when-not` → `(if cond nil (do body...))`
- `cond` → chained `if`s
- `or` → short-circuit chain using `let`
- `and` → short-circuit chain using `let`

**Symbol resolution in eval:**
- Locals (let-bound, fn-params) — look up in environment chain
- Namespace-qualified (`foo.bar/x`) — find ns in `sys.modules`, get Var attribute
- Unqualified — look up in current-ns's mappings (intern'd vars) + refers
- Special-form symbols — handled by dispatch

**Invocation:**
- `(f arg1 arg2)` → eval `f`, eval each arg, `rt::invoke_n(f, [args...])`. Works for any IFn-satisfying value (our Fn closures, Keywords, Vars, plain Python callables via the IFn fallback).

**Current namespace:**
- A thread-local `*ns*` var (already exists via binding infrastructure from core-abstractions) — `def` interns into `*ns*`. `clojure.user` as the default ns.

### Deferred

- `loop`/`recur` (tail-recursion optimization)
- `try`/`catch`/`throw` + `finally`
- `.` / `new` (Java/Python interop for methods — a later "host interop" spec)
- Multi-arity `fn` / overloads
- Destructuring in `let` and `fn` params (e.g., `[[a b] c]`)
- User-defined `defmacro`
- Metadata on fns (`^{:doc "..."}`)
- `letfn`
- Tagged literals beyond hardcoded
- Full `reify` / `deftype` / `defrecord` (records come with collections; protocols exist; multimethods are a follow-on)
- `eval` as a user-facing Clojure function (we expose `eval` to Python; Clojure-side wrapping adds in core.clj spec)
- TCO via trampoline

---

## 3. Architecture

### 3.1 File layout

```
crates/clojure_core/src/
  eval/
    mod.rs              # public API: eval, eval_form, py_eval
    env.rs              # Environment — persistent map chain for locals
    special_forms.rs    # quote/if/do/let/fn/def/var dispatch
    invoke.rs           # function application helpers
    macros.rs           # hardcoded macroexpansion (defn, when, cond, or, and)
    fn_value.rs         # Fn pyclass — a closure: captured env + arglist + body
    resolve.rs          # symbol → value resolution (locals, ns, refers)
    errors.rs           # EvalError
```

### 3.2 Public API

```rust
// In eval/mod.rs:

#[pyfunction]
#[pyo3(name = "eval")]
pub fn py_eval(py: Python<'_>, form: PyObject) -> PyResult<PyObject>;

#[pyfunction]
#[pyo3(name = "eval_string")]
pub fn py_eval_string(py: Python<'_>, source: &str) -> PyResult<PyObject>;
```

`eval_string` = `read_string` then `eval`.

### 3.3 Core loop

```rust
pub fn eval(py: Python<'_>, form: PyObject, env: &Env) -> PyResult<PyObject> {
    match form_kind(py, &form)? {
        // Self-evaluating: nil, bool, int, float, string, char, keyword
        Atom => Ok(form),
        // Symbol: look up in env (locals first, then namespace)
        Symbol => resolve::resolve_symbol(py, form, env),
        // List: first element is special-form symbol, macro, or callable
        List => {
            let head = first(form);
            if let Some(macro_fn) = macros::lookup_builtin_macro(&head) {
                let expanded = macro_fn(py, form.clone(), env)?;
                return eval(py, expanded, env);  // re-enter eval on expanded form
            }
            if let Some(special) = special_forms::lookup(&head) {
                return special(py, form, env);
            }
            // Function call: eval head + each arg, invoke.
            invoke::eval_invocation(py, form, env)
        }
        // Vector/Map/Set: eval each element/pair literal value.
        Vector => ...,
        Map => ...,
        Set => ...,
    }
}
```

### 3.4 Environment

```rust
pub struct Env {
    /// Map of symbol → value for locals (let bindings, fn params, loop bindings).
    /// Empty map at top level.
    pub locals: Py<PersistentHashMap>,
    /// The current namespace (a ClojureNamespace).
    pub current_ns: PyObject,
}
```

When entering a `let`, create a new Env with `locals` extended. When entering a `fn`, the fn carries its captured env; calling it makes a fresh Env built on the captured one + the param bindings.

### 3.5 Fn value

```rust
#[pyclass]
pub struct Fn {
    pub captured_env: Py<PersistentHashMap>,   // locals at point of definition
    pub current_ns: PyObject,
    pub params: Py<PersistentVector>,          // vector of Symbol
    pub body: Py<PyAny>,                       // a form (usually a do-list)
    pub name: Option<Py<Symbol>>,              // for (fn name [args] ...) and stacktrace
}

#[implements(IFn)]
impl IFn for Fn {
    fn invoke0(...) / invoke1(...) / ... / invoke_variadic(...) {
        // 1. Build new Env: captured_env extended with params bound to args.
        //    Arity check.
        // 2. eval(body, new_env).
        // 3. Return result.
    }
}
```

### 3.6 Special-form dispatch

A static map `{ Symbol("quote") → quote_eval, Symbol("if") → if_eval, ... }` computed at `eval::init`.

---

## 4. Error Handling

`EvalError` extends `IllegalArgumentException`. Messages include the form being evaluated (via `pr_str`) and, when available, line/col metadata from the form.

Common errors:
- `"Unable to resolve symbol: foo in this context"`
- `"Too few arguments to function (expected 2, got 1)"`
- `"Cannot invoke non-callable value: 42"`
- `"Cannot eval reference to recur/loop/throw/... (not yet supported)"`

---

## 5. Testing Bar

### 5.1 Per-form tests (`tests/test_eval.py`)

- Atoms eval to themselves: `eval_string("nil") == None`, `eval_string("42") == 42`, `eval_string(":k") is keyword("k")`.
- Vector/map/set literals eval to themselves (since contents are atoms).
- `(quote x)` returns the symbol x.
- `(if true :yes :no)` → `:yes`; `(if false :yes :no)` → `:no`; `(if nil :yes :no)` → `:no`; `(if 0 :yes :no)` → `:yes` (Clojure truthiness: only nil/false are false).
- `(do 1 2 3)` → `3`.
- `(let [a 1 b 2] (+ a b))` → `3`.
- `((fn [x] (* x 2)) 21)` → `42`.
- `(def x 99)` then `(eval-string "x")` → `99`. Verify `x` is interned in the current ns.
- `((fn [x y] (+ x y)) 1 2)` → `3`.
- Closures: `(def f (let [x 10] (fn [] x)))` then `(f)` → `10`.
- `defn` macro: `(defn inc [x] (+ x 1))`, `(inc 41)` → `42`.
- `when`/`cond`/`or`/`and` expansion.

### 5.2 Integration tests

- Round-trip: `eval_string("(reduce + (range 10))")` — hmm, requires `reduce`/`range`. Those don't exist until core.clj bootstrap. **Skip this**. Focus on what we can define.
- Recursive fn: `(def fact (fn [n] (if (= n 0) 1 (* n (fact (- n 1))))))`, `(fact 5)` → `120`. This tests self-reference via Var resolution — works because `fact` is a Var that gets looked up fresh each call.

Wait — `=`, `*`, `-` don't exist either until core.clj. **So we expose them from Rust**: simple Python operators wrapped as IFn-satisfying callables in `clojure.core`. Include in this spec's scaffolding:
- `+`, `-`, `*`, `/`, `=`, `<`, `>`, `<=`, `>=`, `not`, `inc`, `dec`, `list`, `vector`, `hash-map`, `hash-set`, `str`, `first`, `rest`, `cons`, `seq`, `count`
  - All trivially wrap Python equivalents or our rt:: helpers.

These core.clj shims make real-world evaluator tests possible without the full core.clj bootstrap.

### 5.3 hypothesis round-trip

- For integer arithmetic expressions `(+ a b c)` / `(* a b)` / `(- a b)`, round-trip via `eval_string(pr_str(read_string(src))) == eval_string(src)`.

---

## 6. Non-Goals / Follow-on Specs

1. **`loop`/`recur`** with tail-recursion optimization (separate phase).
2. **Exception handling** — `try`/`catch`/`throw`/`finally`.
3. **Python interop sugar** — `.method obj args`, `(py.module/fn args)`, `(new Class args)`.
4. **User-defined `defmacro`** — requires a compile/runtime distinction we don't yet have.
5. **Full destructuring** in let/fn (map-destructuring, seq-destructuring).
6. **`letfn`** / mutual recursion.
7. **Multi-arity fn** — `(fn ([x] ...) ([x y] ...))`.
8. **Metadata on fns / tagged literals** used in eval position.
9. **`core.clj` bootstrap** — load a `.clj` file at init with the standard library definitions.
10. **Tail-call optimization** via trampoline for non-`recur` tail calls.
11. **Multimethods** — `defmulti`, `defmethod`.
12. **Records / deftype / reify** — host interop-adjacent.

This spec leaves hooks for (1-5, 7-9) — special-form dispatch is a Rust `match` that adds branches; macro table is a HashMap; env chain can extend into loop frames.
