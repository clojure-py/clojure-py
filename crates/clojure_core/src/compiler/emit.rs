//! Single-pass analyzer + emitter. Walks a read form and emits Op codes
//! into a growing `Vec<Op>`, updating a `PoolBuilder` for constants / vars.
//!
//! The `Compiler` holds a stack of `FnCtx`s — one per `fn*` nesting level.
//! All write methods (`emit`, `alloc_slot`, `push_local`, …) operate on the
//! top ctx. Symbol resolution walks the stack from top-1 downward to detect
//! closure captures (chaining captures across multiple levels when needed).

use crate::collections::parraymap::PersistentArrayMap;
use crate::collections::phashmap::PersistentHashMap;
use crate::collections::phashset::PersistentHashSet;
use crate::collections::plist::{EmptyList, PersistentList};
use crate::collections::pvector::PersistentVector;
use crate::compiler::method::{CaptureSource, CompiledMethod, FnTemplate};
use crate::compiler::op::Op;
use crate::compiler::pool::{FnPool, PoolBuilder};
use crate::eval::errors;
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyFloat, PyInt, PyString};
use std::sync::Arc;

type PyObject = Py<PyAny>;

/// Post-emit pass that collapses `[Deref(ix), …N single-push ops…, Invoke(N)]`
/// patterns into a single `InvokeVar(ix, N)` op. Runs once per compiled method,
/// after all `emit` calls (so jump-patching has already happened on the original
/// indices). Remaps jump targets via a deletion prefix-sum so control flow is
/// preserved.
///
/// Safety rules for fusing a candidate:
/// - The `N` ops between the `Deref` and the `Invoke` must each be a
///   single-value push (`PushConst` / `LoadLocal` / `LoadCapture` / `LoadSelf`
///   / `Deref` / `LoadVar`). Compound expressions as args aren't fused.
/// - No jump target may land inside the arg-push span (`deref_idx+1
///   ..=invoke_idx`). A target at `deref_idx` itself is fine — in the fused
///   form it remaps to "start emitting args", which has the same effect
///   because `InvokeVar` does the deref internally.
fn fuse_deref_invoke_pass(code: &mut Vec<Op>, pool: &mut PoolBuilder) {
    let code_len = code.len();
    if code_len < 3 { return; }

    // Gather every jump target in one pass so the safety check is O(1) per
    // candidate rather than re-scanning the code vector each time.
    let mut jump_targets: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for op in code.iter() {
        match op {
            Op::Jump(t) | Op::JumpIfFalsy(t) => { jump_targets.insert(*t); }
            Op::PushHandler(t, _) => { jump_targets.insert(*t); }
            _ => {}
        }
    }

    // Identify all fuseable (deref_idx, invoke_idx, var_ix, nargs) tuples.
    let mut fusions: Vec<(usize, usize, u16, u8)> = Vec::new();
    for invoke_idx in 0..code_len {
        let nargs = match code[invoke_idx] {
            Op::Invoke(n) => n,
            _ => continue,
        };
        let n = nargs as usize;
        if invoke_idx < n + 1 { continue; }

        let mut ok = true;
        for i in 1..=n {
            match &code[invoke_idx - i] {
                Op::PushConst(_) | Op::LoadLocal(_) | Op::LoadCapture(_)
                | Op::LoadSelf | Op::Deref(_) | Op::LoadVar(_) => {}
                _ => { ok = false; break; }
            }
        }
        if !ok { continue; }

        let deref_idx = invoke_idx - n - 1;
        let var_ix = match &code[deref_idx] {
            Op::Deref(ix) => *ix,
            _ => continue,
        };

        // Block fusion if any jump lands in (deref_idx, invoke_idx]. Mid-arg
        // targets would have mismatched stack state; target == invoke_idx
        // would have counted on the target already being on the stack, which
        // isn't true post-fusion. Targets at deref_idx are safe — see the
        // doc comment above.
        let mut blocked = false;
        for t in (deref_idx + 1)..=invoke_idx {
            if jump_targets.contains(&(t as u32)) { blocked = true; break; }
        }
        if blocked { continue; }

        fusions.push((deref_idx, invoke_idx, var_ix, nargs));
    }

    if fusions.is_empty() { return; }

    // Apply in reverse order so earlier fusions' indices stay valid.
    for (deref_idx, invoke_idx, var_ix, nargs) in fusions.iter().rev() {
        let ic_slot = pool.alloc_ic_slot();
        code[*invoke_idx] = Op::InvokeVar(*var_ix, *nargs, ic_slot);
        code.remove(*deref_idx);
    }

    // Remap jump targets: each target shifts down by the number of deletions
    // at strictly-lower positions (prefix sum).
    let mut sorted_deletes: Vec<usize> = fusions.iter().map(|(d, _, _, _)| *d).collect();
    sorted_deletes.sort();
    let mut deletions_before = vec![0u32; code_len + 1];
    let mut cum = 0u32;
    let mut di = 0;
    for i in 0..=code_len {
        while di < sorted_deletes.len() && sorted_deletes[di] < i {
            cum += 1;
            di += 1;
        }
        deletions_before[i] = cum;
    }
    for op in code.iter_mut() {
        match op {
            Op::Jump(t) | Op::JumpIfFalsy(t) => {
                let old = *t as usize;
                *t = (old as u32) - deletions_before[old];
            }
            Op::PushHandler(t, _) => {
                let old = *t as usize;
                *t = (old as u32) - deletions_before[old];
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
pub struct LocalBinding {
    pub name: Arc<str>,
    pub slot: u16,
    /// `true` for names introduced by `letfn*`. The slot holds a
    /// `LetfnCell`; references compile to `LoadLocal + LetfnCellGet`,
    /// captures propagate the flag so inner fns also unbox.
    pub is_letfn_cell: bool,
}

/// A single closure capture: where, in the immediately enclosing fn's
/// frame, the captured value comes from.
#[derive(Clone)]
pub struct CaptureBinding {
    pub name: Arc<str>,
    pub source: CaptureSource,
    /// Mirrors `LocalBinding::is_letfn_cell`. Set when the captured
    /// outer binding (or transitively-captured outer-outer) is a
    /// letfn cell. References compile to `LoadCapture + LetfnCellGet`.
    pub is_letfn_cell: bool,
}

pub struct LoopInfo {
    pub top: u32,
    pub slots: Vec<u16>,
}

/// One fn being compiled. The topmost ctx on `Compiler::fns` is the
/// current compilation target.
pub struct FnCtx {
    pub name: Option<String>,
    pub pool: PoolBuilder,
    pub locals: Vec<LocalBinding>,
    pub next_slot: u16,
    pub code: Vec<Op>,
    pub loop_target: Option<LoopInfo>,
    pub captures: Vec<CaptureBinding>,
    /// True when we're at the tail of the fn body — controls where `recur`
    /// is accepted.
    pub tail: bool,
    /// Per-slot counter used by the locals-clearing liveness pass: how many
    /// more outer-scope `LoadLocal(slot)` / `LoadCapture` emissions we
    /// expect for this local before it's dead. When a LoadLocal is emitted
    /// and the remaining count hits 0, the emitter also emits `ClearLocal`.
    pub remaining_uses: std::collections::HashMap<u16, usize>,
    /// Loop slots never get mid-body cleared (they must survive a back-edge).
    pub no_clear_slots: std::collections::HashSet<u16>,
    /// Depth of enclosing `loop*` forms within this fn. When > 0, `LoadLocal`
    /// must NOT auto-emit `ClearLocal` — loop iteration re-executes the same
    /// lexical occurrence multiple times at runtime, but the liveness pass
    /// only sees the single static occurrence.
    pub loop_depth: u32,
}

impl FnCtx {
    fn new(py: Python<'_>, name: Option<String>) -> Self {
        Self {
            name,
            pool: PoolBuilder::new(py),
            locals: Vec::new(),
            next_slot: 0,
            code: Vec::new(),
            loop_target: None,
            captures: Vec::new(),
            tail: true,
            remaining_uses: std::collections::HashMap::new(),
            no_clear_slots: std::collections::HashSet::new(),
            loop_depth: 0,
        }
    }
}

/// Resolution outcome for a Symbol reference, relative to the current fn ctx.
pub enum Resolved {
    /// `(slot, is_letfn_cell)`. When the flag is set, emit
    /// `LoadLocal + LetfnCellGet`.
    Local(u16, bool),
    /// `(capture_ix, is_letfn_cell)`. When the flag is set, emit
    /// `LoadCapture + LetfnCellGet`.
    Capture(u16, bool),
    Var(u16),
    /// A direct Python value (e.g. an exception class) resolved via a
    /// qualified symbol whose attribute isn't a Var. Interned as a constant.
    Const(u16),
    /// The symbol names the currently-executing fn (via `(fn name [...] ...)`).
    /// Emitted as `Op::LoadSelf`.
    SelfRef,
}

pub struct Compiler {
    pub current_ns: PyObject,
    pub fns: Vec<FnCtx>,
}

impl Compiler {
    pub fn new(py: Python<'_>, current_ns: PyObject) -> Self {
        Self {
            current_ns,
            fns: vec![FnCtx::new(py, None)],
        }
    }

    // --- Access helpers ---

    fn cur(&self) -> &FnCtx { self.fns.last().unwrap() }
    fn cur_mut(&mut self) -> &mut FnCtx { self.fns.last_mut().unwrap() }

    pub fn emit(&mut self, op: Op) -> u32 {
        let code = &mut self.cur_mut().code;
        let ix = code.len() as u32;
        code.push(op);
        ix
    }

    /// Emit `LoadLocal(slot)`, then — if liveness tracking says this was the
    /// last load we'll emit for that slot — also emit `ClearLocal(slot)` so
    /// the frame slot stops retaining the value. Head-retention for lazy
    /// seqs hinges on this: the value sits on the operand stack for the
    /// immediately-following Invoke, but no longer in `frame.locals`.
    pub fn emit_load_local(&mut self, slot: u16) {
        self.emit(Op::LoadLocal(slot));
        let should_clear = {
            let ctx = self.cur_mut();
            if ctx.no_clear_slots.contains(&slot) {
                false
            } else if ctx.loop_depth > 0 {
                // Inside a loop — lexical occurrence count is unreliable;
                // never emit ClearLocal while we might iterate.
                false
            } else if let Some(count) = ctx.remaining_uses.get_mut(&slot) {
                if *count > 0 {
                    *count -= 1;
                }
                *count == 0
            } else {
                false
            }
        };
        if should_clear {
            self.emit(Op::ClearLocal(slot));
            // Don't double-clear: remove from tracking.
            self.cur_mut().remaining_uses.remove(&slot);
        }
    }

    pub fn here(&self) -> u32 { self.cur().code.len() as u32 }

    pub fn alloc_slot(&mut self) -> u16 {
        let ctx = self.cur_mut();
        let s = ctx.next_slot;
        ctx.next_slot += 1;
        s
    }

    pub fn push_local(&mut self, name: Arc<str>, slot: u16) {
        self.cur_mut().locals.push(LocalBinding { name, slot, is_letfn_cell: false });
    }

    pub fn push_letfn_local(&mut self, name: Arc<str>, slot: u16) {
        self.cur_mut().locals.push(LocalBinding { name, slot, is_letfn_cell: true });
    }

    pub fn pop_locals_to(&mut self, len: usize) {
        self.cur_mut().locals.truncate(len);
    }

    pub fn locals_len(&self) -> usize { self.cur().locals.len() }

    // --- Symbol resolution (with capture chaining) ---

    /// Resolve an unqualified symbol by name. Walks the fn stack from top
    /// downward; if found in an outer ctx's locals or captures, installs
    /// capture bindings in every intermediate ctx and returns a
    /// `Resolved::Capture(ix)` for the current ctx.
    fn resolve_local_or_capture(&mut self, name: &str) -> Option<Resolved> {
        // Look in current ctx first.
        if let Some(lb) = self.cur().locals.iter().rev()
            .find(|lb| lb.name.as_ref() == name)
        {
            return Some(Resolved::Local(lb.slot, lb.is_letfn_cell));
        }
        // Current ctx's own self-name (e.g. `(fn walk [x] (walk x))`).
        if let Some(n) = self.cur().name.as_deref() {
            if n == name {
                return Some(Resolved::SelfRef);
            }
        }
        // Walk upward looking for a local, capture, or outer-fn's self-name.
        let mut found: Option<(usize, CaptureSource, bool)> = None;
        for (i, ctx) in self.fns.iter().enumerate().rev().skip(1) {
            if let Some(lb) = ctx.locals.iter().rev()
                .find(|lb| lb.name.as_ref() == name)
            {
                found = Some((i, CaptureSource::Local(lb.slot), lb.is_letfn_cell));
                break;
            }
            if let Some((ix, cb)) = ctx.captures.iter().enumerate().find(|(_, cb)| cb.name.as_ref() == name) {
                found = Some((i, CaptureSource::Capture(ix as u16), cb.is_letfn_cell));
                break;
            }
            // Outer fn's self-name: an inner fn references an outer named fn.
            // Capture chain flows with CaptureSource::SelfRef at that level.
            if let Some(n) = ctx.name.as_deref() {
                if n == name {
                    found = Some((i, CaptureSource::SelfRef, false));
                    break;
                }
            }
        }
        let (found_at, initial_source, is_letfn_cell) = found?;
        // Install capture bindings from `found_at + 1` up to current (top).
        // The first intermediate's source is what we actually found.
        // Subsequent ones reference the previous ctx's fresh capture.
        let mut source = initial_source;
        for ctx_ix in (found_at + 1)..self.fns.len() {
            let ctx = &mut self.fns[ctx_ix];
            // Dedup: if this ctx already captured this name, reuse its index.
            let cap_ix = if let Some(ix) = ctx.captures.iter().position(|cb| cb.name.as_ref() == name) {
                ix as u16
            } else {
                let ix = ctx.captures.len() as u16;
                ctx.captures.push(CaptureBinding {
                    name: Arc::from(name),
                    source,
                    is_letfn_cell,
                });
                ix
            };
            source = CaptureSource::Capture(cap_ix);
        }
        // The current ctx's own capture index:
        match source {
            CaptureSource::Capture(ix) => Some(Resolved::Capture(ix, is_letfn_cell)),
            CaptureSource::Local(_) | CaptureSource::SelfRef => unreachable!(),
        }
    }

    pub fn resolve_symbol(&mut self, py: Python<'_>, sym: &Symbol) -> PyResult<Resolved> {
        if let Some(ns_name) = sym.ns.as_deref() {
            let sys = py.import("sys")?;
            let modules = sys.getattr("modules")?;
            // Aliases take precedence over `sys.modules`. Otherwise an
            // unrelated package stub (e.g. `i` created as the parent of
            // `i.a` from a previous test) would shadow a same-named alias
            // installed by `(require '[foo :as i])`.
            let alias_hit: Option<Bound<'_, pyo3::types::PyAny>> = {
                let current = self.current_ns.bind(py);
                if let Ok(aliases_obj) = current.getattr("__clj_aliases__") {
                    if let Ok(aliases) = aliases_obj.cast::<pyo3::types::PyDict>() {
                        let alias_sym = Py::new(
                            py,
                            crate::symbol::Symbol::new(None, std::sync::Arc::from(ns_name)),
                        )?;
                        aliases.get_item(alias_sym)?
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            let target_ns = match alias_hit {
                Some(n) => n,
                None => modules.get_item(ns_name).map_err(|_| {
                    errors::err(format!("No namespace: {}", ns_name))
                })?,
            };
            let attr = target_ns.getattr(sym.name.as_ref()).map_err(|_| {
                errors::err(format!("Unable to resolve: {}/{}", ns_name, sym.name))
            })?;
            if let Ok(var) = attr.cast::<crate::var::Var>() {
                let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
                return Ok(Resolved::Var(ix));
            }
            // Not a Var — treat as a direct Python value (class, fn, etc.)
            // and intern as a constant. Enables referencing classes like
            // `clojure.lang.IllegalArgumentException` from a catch clause.
            let ix = self.cur_mut().pool.intern_const(attr.unbind());
            return Ok(Resolved::Const(ix));
        }

        if let Some(r) = self.resolve_local_or_capture(sym.name.as_ref()) {
            return Ok(r);
        }

        let current = self.current_ns.bind(py);
        if let Ok(attr) = current.getattr(sym.name.as_ref()) {
            if let Ok(var) = attr.cast::<crate::var::Var>() {
                let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
                return Ok(Resolved::Var(ix));
            }
        }

        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        if let Ok(core_ns) = modules.get_item("clojure.core") {
            if let Ok(attr) = core_ns.getattr(sym.name.as_ref()) {
                if let Ok(var) = attr.cast::<crate::var::Var>() {
                    let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
                    return Ok(Resolved::Var(ix));
                }
            }
        }

        // Dotted-module-path fallback: e.g. `clojure.lang.IllegalArgumentException`
        // → sys.modules["clojure.lang"].IllegalArgumentException. Enables
        // catch clauses to name Python classes by dotted path, parallel to
        // vanilla's JVM class-name resolution.
        let name = sym.name.as_ref();
        if let Some(dot_ix) = name.rfind('.') {
            let mod_path = &name[..dot_ix];
            let attr = &name[dot_ix + 1..];
            if let Ok(m) = modules.get_item(mod_path) {
                if let Ok(a) = m.getattr(attr) {
                    let ix = self.cur_mut().pool.intern_const(a.unbind());
                    return Ok(Resolved::Const(ix));
                }
            }
            if let Ok(m) = py.import(mod_path) {
                if let Ok(a) = m.getattr(attr) {
                    let ix = self.cur_mut().pool.intern_const(a.unbind());
                    return Ok(Resolved::Const(ix));
                }
            }
        }
        Err(errors::err(format!(
            "Unable to resolve symbol: {} in this context",
            sym.name
        )))
    }

    /// Public entry point to the compiler's macroexpansion logic. Used by
    /// `clojure.core/macroexpand-1`. Empty-env + empty-locals caller context.
    pub fn try_macroexpand_user_public(
        &mut self,
        py: Python<'_>,
        list_py: &PyObject,
        head: &PyObject,
    ) -> PyResult<Option<PyObject>> {
        self.try_macroexpand_user(py, list_py, head)
    }

    /// For `def` — interns a fresh Var in current-ns and adds it to the
    /// pool. Does NOT deref; returns the pool index and the Var.
    pub fn intern_def_target(&mut self, py: Python<'_>, sym: &Symbol) -> PyResult<(u16, Py<crate::var::Var>)> {
        if sym.ns.is_some() {
            return Err(errors::err(format!(
                "Can't def a qualified symbol: {}/{}",
                sym.ns.as_deref().unwrap(),
                sym.name
            )));
        }
        let fresh_sym = Symbol::new(None, Arc::clone(&sym.name));
        let fresh_sym_py = Py::new(py, fresh_sym)?;
        let var = crate::ns_ops::intern(py, self.current_ns.clone_ref(py), fresh_sym_py)?;
        let ix = self.cur_mut().pool.intern_var(py, var.clone_ref(py));
        Ok((ix, var))
    }

    /// If the list's head is a Symbol that resolves to a Var tagged with
    /// `:macro`, call the Var's fn with `(form &env args...)` and return
    /// the expanded form. `None` means "not a macro call — proceed normally".
    ///
    /// `&env` is passed as an empty PersistentArrayMap for now. Full env
    /// support (symbol → LocalBinding) is out of scope for the first cut.
    fn try_macroexpand_user(
        &mut self,
        py: Python<'_>,
        list_py: &PyObject,
        head: &PyObject,
    ) -> PyResult<Option<PyObject>> {
        let hb = head.bind(py);
        let sym_ref = match hb.cast::<Symbol>() {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        let s = sym_ref.get();
        // Shadowing by a local disables macro dispatch.
        if s.ns.is_none() {
            if self.cur().locals.iter().any(|lb| lb.name.as_ref() == s.name.as_ref()) {
                return Ok(None);
            }
            // Outer-frame locals also shadow (if captured, they're values, not macros).
            for ctx in self.fns.iter().rev().skip(1) {
                if ctx.locals.iter().any(|lb| lb.name.as_ref() == s.name.as_ref()) {
                    return Ok(None);
                }
            }
        }
        // Find the Var without interning it yet (and without derefing).
        let var_py: Option<Py<crate::var::Var>> = find_var(py, s, &self.current_ns)?;
        let var = match var_py {
            Some(v) => v,
            None => return Ok(None),
        };
        if !var.bind(py).get().is_macro(py) {
            return Ok(None);
        }
        let fn_val = var.bind(py).call_method0("deref")?.unbind();
        let user_args = list_rest(py, list_py)?;
        // Build call args: form + env + user_args.
        let env_map: PyObject = crate::collections::parraymap::array_map(
            py,
            pyo3::types::PyTuple::empty(py),
        )?;
        let mut call_args: Vec<PyObject> = Vec::with_capacity(2 + user_args.len());
        call_args.push(list_py.clone_ref(py));
        call_args.push(env_map);
        call_args.extend(user_args.into_iter());
        let expanded = crate::rt::invoke_n(py, fn_val, &call_args)?;
        Ok(Some(expanded))
    }

    /// Resolve an unqualified name to a Var index, with no local/capture
    /// shadowing check. Used for internal references like `_make-closure`
    /// or `bind-root` where we don't want user-space shadowing to break
    /// compilation.
    fn resolve_core_var(&mut self, py: Python<'_>, name: &str) -> PyResult<u16> {
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let core_ns = modules.get_item("clojure.core").map_err(|_| {
            errors::err("clojure.core namespace not found")
        })?;
        let attr = core_ns.getattr(name).map_err(|_| {
            errors::err(format!("Missing core Var: {}", name))
        })?;
        let var = attr.cast::<crate::var::Var>().map_err(|_| {
            errors::err(format!("{} is not a Var", name))
        })?;
        Ok(self.cur_mut().pool.intern_var(py, var.clone().unbind()))
    }

    // --- Top-level form dispatch ---

    pub fn compile_form(&mut self, py: Python<'_>, form: PyObject) -> PyResult<()> {
        let b = form.bind(py);

        if form.is_none(py) {
            let ix = self.cur().pool.nil_ix();
            self.emit(Op::PushConst(ix));
            return Ok(());
        }
        if b.cast::<PyBool>().is_ok()
            || b.cast::<PyInt>().is_ok()
            || b.cast::<PyFloat>().is_ok()
            || b.cast::<PyString>().is_ok()
            || b.cast::<crate::keyword::Keyword>().is_ok()
        {
            let ix = self.cur_mut().pool.intern_const(form);
            self.emit(Op::PushConst(ix));
            return Ok(());
        }

        if let Ok(sym_ref) = b.cast::<Symbol>() {
            match self.resolve_symbol(py, sym_ref.get())? {
                Resolved::Local(slot, is_letfn) => {
                    self.emit_load_local(slot);
                    if is_letfn { self.emit(Op::LetfnCellGet); }
                }
                Resolved::Capture(ix, is_letfn) => {
                    self.emit(Op::LoadCapture(ix));
                    if is_letfn { self.emit(Op::LetfnCellGet); }
                }
                Resolved::Var(ix) => { self.emit(Op::Deref(ix)); }
                Resolved::Const(ix) => { self.emit(Op::PushConst(ix)); }
                Resolved::SelfRef => { self.emit(Op::LoadSelf); }
            }
            return Ok(());
        }

        if let Ok(pl) = b.cast::<PersistentList>() {
            let head = pl.get().head.clone_ref(py);
            return self.compile_list_form(py, form.clone_ref(py), head);
        }
        if b.cast::<EmptyList>().is_ok() {
            let ix = self.cur_mut().pool.intern_const(form);
            self.emit(Op::PushConst(ix));
            return Ok(());
        }
        // Non-list seq types (Cons, LazySeq, VectorSeq, …). Macroexpansions
        // frequently return these (e.g. `(cons 'do body)` → Cons).
        if is_non_list_seq(py, &form)? {
            let items = collect_seq(py, &form)?;
            if items.is_empty() {
                let ix = self.cur_mut().pool.intern_const(form);
                self.emit(Op::PushConst(ix));
                return Ok(());
            }
            let normalized = make_plist(py, &items)?;
            let head = items[0].clone_ref(py);
            return self.compile_list_form(py, normalized, head);
        }

        if let Ok(pv) = b.cast::<PersistentVector>() {
            return self.compile_collection_literal(py, form.clone_ref(py), pv.get().cnt as usize);
        }
        if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
            return self.compile_map_literal(py, form);
        }
        if b.cast::<PersistentHashSet>().is_ok() {
            return self.compile_set_literal(py, form);
        }

        let ix = self.cur_mut().pool.intern_const(form);
        self.emit(Op::PushConst(ix));
        Ok(())
    }

    fn compile_list_form(
        &mut self,
        py: Python<'_>,
        list_py: PyObject,
        head: PyObject,
    ) -> PyResult<()> {
        // User-defined macroexpansion: head resolves to a Var with :macro meta.
        // All macros — including the former hardcoded defn/defmacro/when/
        // when-not/cond/and/or — live in clojure.core as Clojure source now.
        if let Some(expanded) = self.try_macroexpand_user(py, &list_py, &head)? {
            return self.compile_form(py, expanded);
        }

        let hb = head.bind(py);
        if let Ok(sym_ref) = hb.cast::<Symbol>() {
            let s = sym_ref.get();
            if s.ns.is_none() {
                let n = s.name.as_ref();
                match n {
                    "quote" => return self.compile_quote(py, list_py),
                    "if" => return self.compile_if(py, list_py),
                    "do" => return self.compile_do(py, list_py),
                    "let*" => return self.compile_let(py, list_py, false),
                    "loop*" => return self.compile_let(py, list_py, true),
                    "letfn*" => return self.compile_letfn(py, list_py),
                    "recur" => return self.compile_recur(py, list_py),
                    "var" => return self.compile_var_form(py, list_py),
                    "fn*" => return self.compile_fn_form(py, list_py),
                    "def" => return self.compile_def(py, list_py),
                    "set!" => return self.compile_set_bang(py, list_py),
                    "throw" => return self.compile_throw(py, list_py),
                    "try" => return self.compile_try(py, list_py),
                    "." => return self.compile_dot_legacy(py, list_py),
                    _ => {
                        // .-attr sugar → GetAttr
                        if n.starts_with(".-") && n.len() > 2 {
                            return self.compile_get_attr_sugar(py, list_py, &n[2..]);
                        }
                        // .method sugar → CallMethod
                        if n.starts_with('.') && n.len() > 1 && !n.starts_with(".-") {
                            return self.compile_call_method_sugar(py, list_py, &n[1..]);
                        }
                    }
                }
            }
        }
        self.compile_invoke(py, list_py)
    }

    // --- Special forms ---

    fn compile_quote(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() != 1 {
            return Err(errors::err(format!(
                "quote requires 1 argument (got {})",
                args.len()
            )));
        }
        let ix = self.cur_mut().pool.intern_const(args[0].clone_ref(py));
        self.emit(Op::PushConst(ix));
        Ok(())
    }

    fn compile_var_form(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() != 1 {
            return Err(errors::err(format!(
                "var requires 1 argument (got {})",
                args.len()
            )));
        }
        let b = args[0].bind(py);
        let sym_ref = b.cast::<Symbol>().map_err(|_| {
            errors::err("var's argument must be a Symbol")
        })?;
        let s = sym_ref.get();
        // Resolution order for `(var sym)`:
        //   1. If sym is qualified, look up that ns directly.
        //   2. Else try the current ns.
        //   3. Else fall through to clojure.core (mirrors symbol resolution).
        let attr: Bound<'_, PyAny> = match s.ns.as_deref() {
            Some(n) => {
                let sys = py.import("sys")?;
                let modules = sys.getattr("modules")?;
                let target = modules.get_item(n).map_err(|_| {
                    errors::err(format!("No namespace: {} found in (var ...)", n))
                })?;
                target.getattr(s.name.as_ref()).map_err(|_| {
                    errors::err(format!("Unable to resolve var: {}/{}", n, s.name))
                })?
            }
            None => {
                let cur = self.current_ns.bind(py).clone();
                match cur.getattr(s.name.as_ref()) {
                    Ok(a) => a,
                    Err(_) => {
                        let sys = py.import("sys")?;
                        let modules = sys.getattr("modules")?;
                        let core = modules.get_item("clojure.core").map_err(|_| {
                            errors::err("clojure.core not loaded")
                        })?;
                        core.getattr(s.name.as_ref()).map_err(|_| {
                            errors::err(format!("Unable to resolve var: {}", s.name))
                        })?
                    }
                }
            }
        };
        let var = attr.cast::<crate::var::Var>().map_err(|_| {
            errors::err(format!("Symbol {} does not resolve to a Var", s.name))
        })?;
        let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
        self.emit(Op::LoadVar(ix));
        Ok(())
    }

    fn compile_if(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() != 2 && args.len() != 3 {
            return Err(errors::err(format!(
                "if requires 2 or 3 arguments (got {})",
                args.len()
            )));
        }
        let saved_tail = self.cur().tail;
        // Test is never in tail position.
        self.cur_mut().tail = false;
        self.compile_form(py, args[0].clone_ref(py))?;
        self.cur_mut().tail = saved_tail;

        let else_jump = self.emit(Op::JumpIfFalsy(0));
        self.compile_form(py, args[1].clone_ref(py))?;
        let end_jump = self.emit(Op::Jump(0));

        let else_target = self.here();
        self.cur_mut().code[else_jump as usize] = Op::JumpIfFalsy(else_target);

        if args.len() == 3 {
            self.compile_form(py, args[2].clone_ref(py))?;
        } else {
            let nil = self.cur().pool.nil_ix();
            self.emit(Op::PushConst(nil));
        }
        let end_target = self.here();
        self.cur_mut().code[end_jump as usize] = Op::Jump(end_target);
        Ok(())
    }

    fn compile_do(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() {
            let nil = self.cur().pool.nil_ix();
            self.emit(Op::PushConst(nil));
            return Ok(());
        }
        let last = args.len() - 1;
        let saved_tail = self.cur().tail;
        for (i, form) in args.iter().enumerate() {
            self.cur_mut().tail = saved_tail && i == last;
            self.compile_form(py, form.clone_ref(py))?;
            if i != last { self.emit(Op::Pop); }
        }
        self.cur_mut().tail = saved_tail;
        Ok(())
    }

    /// `let*` (no recur target) or `loop*` (pushes a recur target).
    fn compile_let(&mut self, py: Python<'_>, list_py: PyObject, is_loop: bool) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() {
            let what = if is_loop { "loop" } else { "let" };
            return Err(errors::err(format!("{} requires a binding vector", what)));
        }
        let bindings = args[0].bind(py);
        let bv = bindings.cast::<PersistentVector>().map_err(|_| {
            let what = if is_loop { "loop" } else { "let" };
            errors::err(format!("{}: first argument must be a vector of bindings", what))
        })?;
        let bv_ref = bv.get();
        if bv_ref.cnt % 2 != 0 {
            return Err(errors::err("binding vector must have even length"));
        }
        let saved_locals = self.locals_len();
        let mut slot_list: Vec<u16> = Vec::new();
        let mut new_names_slots: Vec<(Arc<str>, u16)> = Vec::new();
        let n = bv_ref.cnt as usize;
        let mut i = 0;
        let saved_tail = self.cur().tail;
        while i < n {
            let name_form = bv_ref.nth_internal_pub(py, i)?;
            let name_b = name_form.bind(py);
            let name_sym = name_b.cast::<Symbol>().map_err(|_| {
                errors::err("binding name must be a Symbol")
            })?;
            let val_form = bv_ref.nth_internal_pub(py, i + 1)?;
            self.cur_mut().tail = false;
            self.compile_form(py, val_form)?;
            let slot = self.alloc_slot();
            self.emit(Op::StoreLocal(slot));
            let name_arc = Arc::clone(&name_sym.get().name);
            self.push_local(Arc::clone(&name_arc), slot);
            slot_list.push(slot);
            new_names_slots.push((name_arc, slot));
            i += 2;
        }
        self.cur_mut().tail = saved_tail;

        // Pre-scan body to populate `remaining_uses` for the new locals.
        // Skip the liveness pass if this is a `loop*` — loop slots must stay
        // live across the back-edge, so we handle them at loop exit only.
        let body: Vec<PyObject> = args[1..].iter().map(|o| o.clone_ref(py)).collect();
        let saved_uses = self.cur().remaining_uses.clone();
        if !is_loop {
            let name_to_slot: std::collections::HashMap<Arc<str>, u16> =
                new_names_slots.iter().map(|(n, s)| (Arc::clone(n), *s)).collect();
            let captured_by_fn = body_has_fn_capturing(py, &body, &name_to_slot)?;
            for (name, slot) in &new_names_slots {
                if captured_by_fn.contains(name.as_ref()) {
                    self.cur_mut().no_clear_slots.insert(*slot);
                    continue;
                }
                let count = count_outer_refs_in_forms(py, &body, name.as_ref())?;
                // `usize::MAX` is the macro-sentinel from
                // `count_outer_refs_in_form`: the body contains a macro
                // call whose expansion we can't predict statically. Don't
                // mid-body clear this slot — it stays alive to end of scope.
                if count == usize::MAX {
                    self.cur_mut().no_clear_slots.insert(*slot);
                } else if count > 0 {
                    self.cur_mut().remaining_uses.insert(*slot, count);
                }
            }
            // Inner-loop liveness: `emit_load_local` checks `loop_depth > 0`
            // at emit time and skips auto-clear inside any loop*. That's
            // simpler and more robust than pre-scanning the body for loops
            // (macros like `while` expand to loop* only after macroexpansion,
            // which a pre-scan would miss).
        } else {
            // Loop slots never mid-body clear.
            for (_, slot) in &new_names_slots {
                self.cur_mut().no_clear_slots.insert(*slot);
            }
        }

        let saved_loop = if is_loop {
            let top = self.here();
            self.cur_mut().loop_depth += 1;
            Some(std::mem::replace(
                &mut self.cur_mut().loop_target,
                Some(LoopInfo { top, slots: slot_list.clone() }),
            ))
        } else {
            None
        };

        if body.is_empty() {
            let nil = self.cur().pool.nil_ix();
            self.emit(Op::PushConst(nil));
        } else {
            let last = body.len() - 1;
            for (i, form) in body.iter().enumerate() {
                // A loop's body is its own tail context — `recur` targets
                // this loop regardless of the outer form's tail state. For
                // plain `let`, the body inherits the outer tail state.
                self.cur_mut().tail = if is_loop {
                    i == last
                } else {
                    saved_tail && i == last
                };
                self.compile_form(py, form.clone_ref(py))?;
                if i != last { self.emit(Op::Pop); }
            }
        }
        self.cur_mut().tail = saved_tail;

        if is_loop {
            self.cur_mut().loop_target = saved_loop.unwrap();
            self.cur_mut().loop_depth -= 1;
        }

        // Scope-end safety net: any new local that wasn't cleared yet
        // (either because its last-use was on a branch path not taken, or
        // because the local was never referenced) gets cleared now.
        for (_, slot) in &new_names_slots {
            let still_tracked = self.cur().remaining_uses.contains_key(slot);
            let no_clear = self.cur().no_clear_slots.contains(slot);
            if still_tracked || no_clear {
                self.emit(Op::ClearLocal(*slot));
            }
        }

        // Restore `remaining_uses` and `no_clear_slots` — removing entries
        // we added (inner scope shouldn't leak liveness state to outer).
        self.cur_mut().remaining_uses = saved_uses;
        for (_, slot) in &new_names_slots {
            self.cur_mut().no_clear_slots.remove(slot);
        }
        self.pop_locals_to(saved_locals);
        Ok(())
    }

    /// `letfn* [name1 fn-form1, name2 fn-form2, …] body…`
    ///
    /// All names are mutually visible inside every fn body and the
    /// trailing body. Implementation: each name maps to a slot holding
    /// a fresh `LetfnCell` (allocated via `LetfnCellInit`). All cells
    /// exist before any fn is compiled, so each closure can capture
    /// the cells of its peers. After a fn is constructed its closure
    /// is stored into its cell via `LetfnCellSet`. Name references
    /// (in any nesting depth) compile to `Load{Local,Capture} +
    /// LetfnCellGet`, dispatched on the `is_letfn_cell` flag carried
    /// by `LocalBinding` / `CaptureBinding`.
    fn compile_letfn(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() {
            return Err(errors::err("letfn* requires a binding vector"));
        }
        let bindings = args[0].bind(py);
        let bv = bindings.cast::<PersistentVector>().map_err(|_| {
            errors::err("letfn*: first argument must be a vector of bindings")
        })?;
        let bv_ref = bv.get();
        if bv_ref.cnt % 2 != 0 {
            return Err(errors::err("letfn* binding vector must have even length"));
        }
        let saved_locals = self.locals_len();
        let saved_tail = self.cur().tail;

        // Pass 1: parse names, allocate slots, init cells, push locals.
        let n = bv_ref.cnt as usize;
        let mut name_slot_pairs: Vec<(Arc<str>, u16)> = Vec::with_capacity(n / 2);
        let mut value_forms: Vec<PyObject> = Vec::with_capacity(n / 2);
        let mut i = 0;
        while i < n {
            let name_form = bv_ref.nth_internal_pub(py, i)?;
            let name_b = name_form.bind(py);
            let name_sym = name_b.cast::<Symbol>().map_err(|_| {
                errors::err("letfn* binding name must be a Symbol")
            })?;
            let val_form = bv_ref.nth_internal_pub(py, i + 1)?;
            let slot = self.alloc_slot();
            self.emit(Op::LetfnCellInit(slot));
            let name_arc = Arc::clone(&name_sym.get().name);
            self.push_letfn_local(Arc::clone(&name_arc), slot);
            // Cells live for the entire scope; never clear them mid-body.
            self.cur_mut().no_clear_slots.insert(slot);
            name_slot_pairs.push((name_arc, slot));
            value_forms.push(val_form);
            i += 2;
        }

        // Pass 2: compile each value form (expected to be a fn-form, but
        // we don't enforce that — vanilla doesn't either). After each is
        // on the stack, store it into its cell.
        self.cur_mut().tail = false;
        for ((_, slot), val_form) in name_slot_pairs.iter().zip(value_forms.into_iter()) {
            self.compile_form(py, val_form)?;
            self.emit(Op::LetfnCellSet(*slot));
        }
        self.cur_mut().tail = saved_tail;

        // Pass 3: compile body as implicit (do …).
        let body: Vec<PyObject> = args[1..].iter().map(|o| o.clone_ref(py)).collect();
        if body.is_empty() {
            let nil = self.cur().pool.nil_ix();
            self.emit(Op::PushConst(nil));
        } else {
            let last = body.len() - 1;
            for (i, form) in body.iter().enumerate() {
                self.cur_mut().tail = saved_tail && i == last;
                self.compile_form(py, form.clone_ref(py))?;
                if i != last { self.emit(Op::Pop); }
            }
        }
        self.cur_mut().tail = saved_tail;

        // Cleanup: clear each cell slot at scope exit, drop liveness state,
        // pop locals.
        for (_, slot) in &name_slot_pairs {
            self.emit(Op::ClearLocal(*slot));
            self.cur_mut().no_clear_slots.remove(slot);
        }
        self.pop_locals_to(saved_locals);
        Ok(())
    }

    /// `recur args...`. Must be in tail position of an enclosing loop/fn
    /// with matching arity. Lowers to StoreLocals + Jump.
    fn compile_recur(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if !self.cur().tail {
            return Err(errors::err("Can only recur from tail position"));
        }
        let loop_target = self.cur().loop_target.as_ref().ok_or_else(|| {
            errors::err("recur used outside of any loop or fn")
        })?;
        if args.len() != loop_target.slots.len() {
            return Err(errors::err(format!(
                "Mismatched argument count to recur, expected: {} args, got: {}",
                loop_target.slots.len(),
                args.len()
            )));
        }
        // Snapshot targets before mutating.
        let slots = loop_target.slots.clone();
        let top = loop_target.top;

        // Evaluate each arg in order; args are not in tail position.
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        for a in &args {
            self.compile_form(py, a.clone_ref(py))?;
        }
        self.cur_mut().tail = saved_tail;
        // Store to slots in reverse (last pushed → last stored → first slot).
        for slot in slots.iter().rev() {
            self.emit(Op::StoreLocal(*slot));
        }
        self.emit(Op::Jump(top));
        // After Jump, stack discipline expects the enclosing form to push a
        // value; emit a nil as an unreachable placeholder (won't actually
        // execute, but keeps the stack machine happy about "producing" a
        // value for this expression position).
        let nil = self.cur().pool.nil_ix();
        self.emit(Op::PushConst(nil));
        Ok(())
    }

    /// Three shapes:
    ///   `(def foo)`                   — declare; emits LoadVar.
    ///   `(def foo init)`              — bind.
    ///   `(def foo "docstring" init)`  — bind + attach `:doc` to Var meta.
    fn compile_def(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() || args.len() > 3 {
            return Err(errors::err(format!(
                "def requires 1, 2, or 3 arguments (got {})",
                args.len()
            )));
        }
        let name_form = args[0].bind(py);
        let sym_ref = name_form.cast::<Symbol>().map_err(|_| {
            errors::err("def's first argument must be a Symbol")
        })?;

        // Optional docstring in the 3-arg form.
        let (docstring, init_ix): (Option<PyObject>, Option<usize>) = match args.len() {
            3 => {
                let doc_form = args[1].bind(py);
                let doc = doc_form
                    .cast::<pyo3::types::PyString>()
                    .map_err(|_| errors::err("def: docstring must be a string literal"))?;
                (Some(doc.clone().unbind().into_any()), Some(2))
            }
            2 => (None, Some(1)),
            _ => (None, None),
        };

        // Propagate the Symbol's metadata (e.g. ^{:macro true}, :doc, :private)
        // to the Var. Vanilla Clojure takes this off the name symbol and
        // attaches it to the Var. Add `:doc` from the 3-arg form on top.
        let sym_meta: Option<PyObject> =
            sym_ref.get().meta.as_ref().map(|o| o.clone_ref(py));
        let (target_ix, target_var) = self.intern_def_target(py, sym_ref.get())?;
        let merged_meta = merge_doc_into_meta(py, sym_meta, docstring)?;
        // Also stamp `:ns` (the current namespace) and `:name` (the Var's
        // name as a Symbol) — vanilla Clojure does this so test runners
        // and introspection tools (e.g. clojure.test/test-vars) can locate
        // a Var's home namespace from its meta.
        let ns_kw: PyObject = crate::keyword::keyword(py, "ns", None)?.into_any();
        let name_kw: PyObject = crate::keyword::keyword(py, "name", None)?.into_any();
        let name_sym: PyObject = Py::new(
            py,
            crate::symbol::Symbol::new(None, sym_ref.get().name.clone()),
        )?.into_any();
        let ns_obj = self.current_ns.clone_ref(py);
        let stamped = match merged_meta {
            Some(m) => {
                let m1 = m.bind(py).call_method1("assoc", (ns_kw, ns_obj))?;
                let m2 = m1.call_method1("assoc", (name_kw, name_sym))?.unbind();
                Some(m2)
            }
            None => {
                let tup = pyo3::types::PyTuple::new(py, &[ns_kw, ns_obj, name_kw, name_sym])?;
                Some(crate::collections::parraymap::array_map(py, tup)?)
            }
        };
        if let Some(meta) = stamped {
            // If `:dynamic true` is on the meta, flip the Var's dynamic flag.
            let dyn_kw = crate::keyword::keyword(py, "dynamic", None)?.into_any();
            let dyn_val = crate::rt::get(py, meta.clone_ref(py), dyn_kw, py.None())?;
            let is_dyn = dyn_val.bind(py).is_truthy().unwrap_or(false);
            target_var.bind(py).get().set_dynamic(is_dyn);
            target_var.bind(py).get().set_meta(Some(meta));
        }

        match init_ix {
            Some(ix) => {
                let bind_root_ix = self.resolve_core_var(py, "bind-root")?;
                self.emit(Op::Deref(bind_root_ix));
                self.emit(Op::LoadVar(target_ix));
                let saved_tail = self.cur().tail;
                self.cur_mut().tail = false;
                self.compile_form(py, args[ix].clone_ref(py))?;
                self.cur_mut().tail = saved_tail;
                self.emit(Op::Invoke(2));
                // `bind-root` returns the Var; keep that as the def expr's value.
            }
            None => {
                // (def foo) — just ensure Var exists. Push it.
                self.emit(Op::LoadVar(target_ix));
            }
        }
        Ok(())
    }

    /// `(fn [params...] body...)`,
    /// `(fn name [params...] body...)`,
    /// `(fn ([p..] body..) ([p..] body..) ...)` — multi-arity,
    /// with `&` in any params vector marking the rest arg.
    fn compile_fn_form(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() {
            return Err(errors::err("fn requires at least a parameter vector"));
        }
        let (name, rest): (Option<String>, &[PyObject]) = {
            let b0 = args[0].bind(py);
            if let Ok(sym_ref) = b0.cast::<Symbol>() {
                if args.len() < 2 {
                    return Err(errors::err("fn with name requires a parameter vector"));
                }
                (Some(sym_ref.get().name.to_string()), &args[1..])
            } else {
                (None, &args[..])
            }
        };
        if rest.is_empty() {
            return Err(errors::err("fn requires at least one method body"));
        }

        // Decide single vs multi-arity based on the first form after name.
        let method_specs: Vec<(PyObject, Vec<PyObject>)> = {
            let first_b = rest[0].bind(py);
            if first_b.cast::<PersistentVector>().is_ok() {
                // single arity: rest = [params body...]
                vec![(rest[0].clone_ref(py), rest[1..].iter().map(|o| o.clone_ref(py)).collect())]
            } else if first_b.cast::<PersistentList>().is_ok()
                || is_non_list_seq(py, &rest[0])?
            {
                let mut specs = Vec::with_capacity(rest.len());
                for item in rest {
                    // Each arity spec may be a PersistentList or a non-list
                    // seq (e.g. Cons, arising from macro expansions like
                    // `(cons 'fn fdecl)` inside defn). Either way we iterate
                    // via collect_seq-style logic that handles both.
                    let items = if item.bind(py).cast::<PersistentList>().is_ok() {
                        list_items(py, item)?
                    } else {
                        collect_seq(py, item)?
                    };
                    if items.is_empty() {
                        return Err(errors::err(
                            "fn arity spec requires a parameter vector + body",
                        ));
                    }
                    let params = items[0].clone_ref(py);
                    let body: Vec<PyObject> = items[1..].iter().map(|o| o.clone_ref(py)).collect();
                    specs.push((params, body));
                }
                specs
            } else {
                return Err(errors::err(format!(
                    "fn: expected parameter vector or arity list, got {}",
                    first_b.repr().map(|s| s.to_string()).unwrap_or_default()
                )));
            }
        };

        // Enter a new FnCtx — pool and captures are shared across all arity
        // methods of this fn.
        self.fns.push(FnCtx::new(py, name.clone()));

        let mut methods: Vec<CompiledMethod> = Vec::new();
        let mut variadic: Option<CompiledMethod> = None;

        for (params_form, body) in method_specs {
            let (param_names, is_variadic) = parse_params(py, &params_form)?;
            let required = if is_variadic { param_names.len() - 1 } else { param_names.len() };
            if required > u16::MAX as usize {
                return Err(errors::err("fn arity too large"));
            }
            if is_variadic && variadic.is_some() {
                return Err(errors::err("Can't have more than 1 variadic overload"));
            }
            if !is_variadic && methods.iter().any(|m| m.arity as usize == required) {
                return Err(errors::err(format!(
                    "Can't have 2 overloads with same arity ({})",
                    required
                )));
            }

            // Reset per-method state on the ctx (pool + captures persist).
            {
                let cur = self.cur_mut();
                cur.locals.clear();
                cur.next_slot = 0;
                cur.code.clear();
                cur.loop_target = None;
                cur.tail = true;
            }

            // Allocate slots for all params (positional + rest if variadic).
            let mut all_slots: Vec<u16> = Vec::with_capacity(param_names.len());
            let mut new_names_slots: Vec<(Arc<str>, u16)> = Vec::new();
            for pname in &param_names {
                let slot = self.alloc_slot();
                self.push_local(Arc::clone(pname), slot);
                all_slots.push(slot);
                new_names_slots.push((Arc::clone(pname), slot));
            }
            // Implicit fn-level recur target: body entry is op 0; recur args
            // match all param slots (including the rest seq slot for variadic).
            // Params are recur-targets, so they must NOT be mid-body cleared.
            self.cur_mut().loop_target = Some(LoopInfo { top: 0, slots: all_slots.clone() });
            for s in &all_slots {
                self.cur_mut().no_clear_slots.insert(*s);
            }
            let _ = new_names_slots;  // params handled by no_clear_slots above

            // Compile body as implicit (do ...) in tail position.
            if body.is_empty() {
                let nil = self.cur().pool.nil_ix();
                self.emit(Op::PushConst(nil));
            } else {
                let last = body.len() - 1;
                for (i, form) in body.iter().enumerate() {
                    self.cur_mut().tail = i == last;
                    self.compile_form(py, form.clone_ref(py))?;
                    if i != last { self.emit(Op::Pop); }
                }
            }
            self.emit(Op::Return);

            // Peephole: collapse Deref+args+Invoke -> InvokeVar before
            // building the final CompiledMethod. Runs once per method.
            {
                let cur = self.cur_mut();
                let (code, pool) = (&mut cur.code, &mut cur.pool);
                fuse_deref_invoke_pass(code, pool);
            }

            let method = {
                let cur = self.cur();
                CompiledMethod {
                    arity: required as u16,
                    is_variadic,
                    local_slots: cur.next_slot,
                    code: cur.code.clone(),
                }
            };
            if is_variadic {
                variadic = Some(method);
            } else {
                methods.push(method);
            }
        }

        // Pop FnCtx — pool + captures assemble the FnTemplate.
        let inner = self.fns.pop().unwrap();
        let capture_sources: Vec<CaptureSource> =
            inner.captures.iter().map(|c| c.source.clone()).collect();
        let template = FnTemplate {
            name: name.clone(),
            current_ns: self.current_ns.clone_ref(py),
            capture_sources: capture_sources.clone(),
            methods,
            variadic,
            pool: inner.pool.finish(),
        };
        let n_captures = capture_sources.len();
        let template_py: Py<FnTemplate> = Py::new(py, template)?;

        // Outer ctx emits the closure construction.
        let make_closure_ix = self.resolve_core_var(py, "_make-closure")?;
        self.emit(Op::Deref(make_closure_ix));
        let template_ix = self.cur_mut().pool.intern_const(template_py.into_any());
        self.emit(Op::PushConst(template_ix));
        for src in &capture_sources {
            match src {
                CaptureSource::Local(slot) => { self.emit(Op::LoadLocal(*slot)); }
                CaptureSource::Capture(ix) => { self.emit(Op::LoadCapture(*ix)); }
                CaptureSource::SelfRef => { self.emit(Op::LoadSelf); }
            }
        }
        if 1 + n_captures > u8::MAX as usize {
            return Err(errors::err("fn captures too many locals"));
        }
        self.emit(Op::Invoke((1 + n_captures) as u8));
        Ok(())
    }

    // --- Python interop ---

    /// `(.-attr obj)` → GetAttr.
    fn compile_get_attr_sugar(
        &mut self,
        py: Python<'_>,
        list_py: PyObject,
        attr: &str,
    ) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() != 1 {
            return Err(errors::err(format!(
                "(.-{} obj) takes exactly 1 arg, got {}",
                attr,
                args.len()
            )));
        }
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        self.compile_form(py, args[0].clone_ref(py))?;
        self.cur_mut().tail = saved_tail;
        let name_py = pyo3::types::PyString::new(py, attr).unbind().into_any();
        let ix = self.cur_mut().pool.intern_const(name_py);
        self.emit(Op::GetAttr(ix));
        Ok(())
    }

    /// `(.method obj args...)` → CallMethod(name, nargs).
    fn compile_call_method_sugar(
        &mut self,
        py: Python<'_>,
        list_py: PyObject,
        method: &str,
    ) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() {
            return Err(errors::err(format!(
                "(.{} obj ...) requires an object",
                method
            )));
        }
        if args.len() - 1 > u8::MAX as usize {
            return Err(errors::err(format!(
                "method call with {} args exceeds u8::MAX",
                args.len() - 1
            )));
        }
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        // obj
        self.compile_form(py, args[0].clone_ref(py))?;
        // method args
        for a in &args[1..] {
            self.compile_form(py, a.clone_ref(py))?;
        }
        self.cur_mut().tail = saved_tail;
        let name_py = pyo3::types::PyString::new(py, method).unbind().into_any();
        let ix = self.cur_mut().pool.intern_const(name_py);
        self.emit(Op::CallMethod(ix, (args.len() - 1) as u8));
        Ok(())
    }

    /// Legacy `(. obj method args...)` / `(. obj (method args...))` /
    /// `(. obj -attr)`. Normalizes to the sugar form.
    fn compile_dot_legacy(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() < 2 {
            return Err(errors::err("(. obj <member>) requires at least 2 args"));
        }
        let obj_form = args[0].clone_ref(py);

        // `(. obj (method args...))` — parenthesized call form.
        if let Ok(pl) = args[1].bind(py).cast::<PersistentList>() {
            let head = pl.get().head.clone_ref(py);
            let hb = head.bind(py);
            let method_sym = hb.cast::<Symbol>().map_err(|_| {
                errors::err("(. obj (member args)): member must be a Symbol")
            })?;
            let method_name = method_sym.get().name.to_string();
            let inner_args = list_rest(py, &args[1])?;
            let saved_tail = self.cur().tail;
            self.cur_mut().tail = false;
            self.compile_form(py, obj_form)?;
            for a in &inner_args {
                self.compile_form(py, a.clone_ref(py))?;
            }
            self.cur_mut().tail = saved_tail;
            let name_py = pyo3::types::PyString::new(py, &method_name).unbind().into_any();
            let ix = self.cur_mut().pool.intern_const(name_py);
            if inner_args.len() > u8::MAX as usize {
                return Err(errors::err("method call too many args"));
            }
            self.emit(Op::CallMethod(ix, inner_args.len() as u8));
            return Ok(());
        }

        // Flat form: 2nd arg is a Symbol — either `-attr` or `method`.
        let member = args[1].bind(py);
        let sym_ref = member.cast::<Symbol>().map_err(|_| {
            errors::err("(. obj <member>): member must be a Symbol or (method args) list")
        })?;
        let name = sym_ref.get().name.to_string();
        if let Some(attr) = name.strip_prefix('-') {
            if args.len() != 2 {
                return Err(errors::err("(. obj -attr) takes no arguments"));
            }
            let saved_tail = self.cur().tail;
            self.cur_mut().tail = false;
            self.compile_form(py, obj_form)?;
            self.cur_mut().tail = saved_tail;
            let name_py = pyo3::types::PyString::new(py, attr).unbind().into_any();
            let ix = self.cur_mut().pool.intern_const(name_py);
            self.emit(Op::GetAttr(ix));
            return Ok(());
        }
        // method call: `(. obj method args...)`.
        let method_args = &args[2..];
        if method_args.len() > u8::MAX as usize {
            return Err(errors::err("method call too many args"));
        }
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        self.compile_form(py, obj_form)?;
        for a in method_args {
            self.compile_form(py, a.clone_ref(py))?;
        }
        self.cur_mut().tail = saved_tail;
        let name_py = pyo3::types::PyString::new(py, &name).unbind().into_any();
        let ix = self.cur_mut().pool.intern_const(name_py);
        self.emit(Op::CallMethod(ix, method_args.len() as u8));
        Ok(())
    }

    /// `(set! (.-attr obj) val)` → SetAttr. Other `set!` targets (Var
    /// mutation) are out of scope for this plan.
    fn compile_set_bang(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() != 2 {
            return Err(errors::err("set! takes a target and a value"));
        }
        let target = &args[0];
        let target_b = target.bind(py);
        // Target must be a list of the form (.-attr obj) or (. obj -attr).
        let items: Vec<PyObject> = if let Ok(_pl) = target_b.cast::<PersistentList>() {
            list_items(py, target)?
        } else {
            return Err(errors::err("set!: only (.-attr obj) targets are supported"));
        };
        if items.is_empty() {
            return Err(errors::err("set!: empty target"));
        }
        let head_b = items[0].bind(py);
        let sym_ref = head_b.cast::<Symbol>().map_err(|_| {
            errors::err("set!: target head must be a Symbol")
        })?;
        let head_name = sym_ref.get().name.clone();
        let (attr_owned, obj_form): (String, PyObject) = if let Some(attr) = head_name.strip_prefix(".-") {
            if items.len() != 2 {
                return Err(errors::err("set! (.-attr obj): needs exactly one object"));
            }
            (attr.to_string(), items[1].clone_ref(py))
        } else if head_name.as_ref() == "." {
            if items.len() != 3 {
                return Err(errors::err("set! (. obj -attr): wrong shape"));
            }
            let m_b = items[2].bind(py);
            let m_sym = m_b.cast::<Symbol>().map_err(|_| {
                errors::err("set! (. obj -attr): member must be a Symbol")
            })?;
            let m_name = m_sym.get().name.to_string();
            let attr = m_name.strip_prefix('-').ok_or_else(|| {
                errors::err("set! (. obj -attr): member must begin with -")
            })?.to_string();
            (attr, items[1].clone_ref(py))
        } else {
            return Err(errors::err(
                "set!: only (.-attr obj) and (. obj -attr) targets are supported",
            ));
        };
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        self.compile_form(py, obj_form)?;
        self.compile_form(py, args[1].clone_ref(py))?;
        self.cur_mut().tail = saved_tail;
        let name_py = pyo3::types::PyString::new(py, &attr_owned).unbind().into_any();
        let ix = self.cur_mut().pool.intern_const(name_py);
        self.emit(Op::SetAttr(ix));
        Ok(())
    }

    fn compile_throw(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.len() != 1 {
            return Err(errors::err(format!(
                "throw requires exactly 1 argument (got {})",
                args.len()
            )));
        }
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        self.compile_form(py, args[0].clone_ref(py))?;
        self.cur_mut().tail = saved_tail;
        self.emit(Op::Throw);
        Ok(())
    }

    /// `(try body* (catch Class binding catch-body*)* (finally finally-body*)?)`.
    ///
    /// Compilation uses two dedicated locals per try: `exc_slot` (holds the
    /// caught exception; set by the VM on unwind; nil on the normal path) and
    /// `result_slot` (holds the try/catch result, preserved across finally).
    /// The handler targets `catch_L`. After all catch clauses run, control
    /// falls through to the finally block; after finally, if `exc_slot` is
    /// still non-nil (no catch matched), the exception is re-thrown.
    fn compile_try(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;

        // Partition args into body / catch clauses / finally.
        let mut body_forms: Vec<PyObject> = Vec::new();
        let mut catch_clauses: Vec<(PyObject, Arc<str>, Vec<PyObject>)> = Vec::new();
        let mut finally_body: Option<Vec<PyObject>> = None;

        for arg in args.iter() {
            let clause_kind = clause_head_kind(py, arg);
            match clause_kind {
                ClauseKind::Catch => {
                    let items = list_items(py, arg)?;
                    if items.len() < 3 {
                        return Err(errors::err(
                            "catch requires (catch Class binding body...)",
                        ));
                    }
                    let class_form = items[1].clone_ref(py);
                    let binding_sym = items[2].bind(py).cast::<Symbol>().map_err(|_| {
                        errors::err("catch binding must be a symbol")
                    })?;
                    let binding_name = Arc::clone(&binding_sym.get().name);
                    let body: Vec<PyObject> =
                        items[3..].iter().map(|x| x.clone_ref(py)).collect();
                    catch_clauses.push((class_form, binding_name, body));
                }
                ClauseKind::Finally => {
                    if finally_body.is_some() {
                        return Err(errors::err("try supports only one finally clause"));
                    }
                    let items = list_items(py, arg)?;
                    finally_body =
                        Some(items[1..].iter().map(|x| x.clone_ref(py)).collect());
                }
                ClauseKind::Body => {
                    if !catch_clauses.is_empty() || finally_body.is_some() {
                        return Err(errors::err(
                            "try body forms must precede catch/finally clauses",
                        ));
                    }
                    body_forms.push(arg.clone_ref(py));
                }
            }
        }

        let has_catches = !catch_clauses.is_empty();
        let has_finally = finally_body.is_some();
        let nil_ix = self.cur().pool.nil_ix();

        // Degenerate case: no catches, no finally — `try` is just a `do`.
        if !has_catches && !has_finally {
            if body_forms.is_empty() {
                self.emit(Op::PushConst(nil_ix));
                return Ok(());
            }
            let last = body_forms.len() - 1;
            let saved_tail = self.cur().tail;
            for (i, f) in body_forms.iter().enumerate() {
                self.cur_mut().tail = saved_tail && i == last;
                self.compile_form(py, f.clone_ref(py))?;
                if i != last {
                    self.emit(Op::Pop);
                }
            }
            self.cur_mut().tail = saved_tail;
            return Ok(());
        }

        // try body/catch/finally is never in tail position — the finally
        // (and re-throw check) still need to run after the producing form.
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;

        // Intern builtins.isinstance if any catches exist.
        let isinstance_ix: u16 = if has_catches {
            let bi = py.import("builtins")?.getattr("isinstance")?.unbind();
            self.cur_mut().pool.intern_const(bi)
        } else {
            0
        };

        // Allocate dedicated slots and mark them exempt from the liveness
        // clearing pass — their writes happen via VM unwind, which the
        // emit-time liveness tracker can't see.
        let exc_slot = self.alloc_slot();
        let result_slot = self.alloc_slot();
        self.cur_mut().no_clear_slots.insert(exc_slot);
        self.cur_mut().no_clear_slots.insert(result_slot);

        // Initialize exc_slot to nil so the post-finally rethrow check can
        // distinguish "no exception" (nil) from "caught/unmatched" (value).
        self.emit(Op::PushConst(nil_ix));
        self.emit(Op::StoreLocal(exc_slot));

        // Install the handler. target_pc patched after catch_L is known.
        let handler_pc = self.emit(Op::PushHandler(0, exc_slot));

        // Compile body; its value is the try-success result.
        if body_forms.is_empty() {
            self.emit(Op::PushConst(nil_ix));
        } else {
            let last = body_forms.len() - 1;
            for (i, f) in body_forms.iter().enumerate() {
                self.compile_form(py, f.clone_ref(py))?;
                if i != last {
                    self.emit(Op::Pop);
                }
            }
        }

        // Body succeeded: pop handler, stash result, jump to finally.
        self.emit(Op::PopHandler);
        self.emit(Op::StoreLocal(result_slot));
        let jump_body_ok = self.emit(Op::Jump(0));

        // Catch landing. VM stashes exception into exc_slot and truncates
        // the value stack before jumping here.
        let catch_l = self.here();

        // Track catch-body success jumps so we can patch them to finally_pc.
        let mut catch_end_jumps: Vec<u32> = Vec::new();

        if has_catches {
            for (class_form, binding_name, body) in &catch_clauses {
                // isinstance(exc, Class)
                self.emit(Op::PushConst(isinstance_ix));
                self.emit(Op::LoadLocal(exc_slot));
                self.compile_form(py, class_form.clone_ref(py))?;
                self.emit(Op::Invoke(2));
                let skip_clause = self.emit(Op::JumpIfFalsy(0));

                // Matched: bind exception to the catch binding, run body.
                let v_slot = self.alloc_slot();
                self.cur_mut().no_clear_slots.insert(v_slot);
                self.emit(Op::LoadLocal(exc_slot));
                self.emit(Op::StoreLocal(v_slot));
                let saved_len = self.locals_len();
                self.push_local(Arc::clone(binding_name), v_slot);

                // Signal "caught" to the post-finally rethrow check.
                self.emit(Op::PushConst(nil_ix));
                self.emit(Op::StoreLocal(exc_slot));

                if body.is_empty() {
                    self.emit(Op::PushConst(nil_ix));
                } else {
                    let last = body.len() - 1;
                    for (i, f) in body.iter().enumerate() {
                        self.compile_form(py, f.clone_ref(py))?;
                        if i != last {
                            self.emit(Op::Pop);
                        }
                    }
                }
                self.pop_locals_to(saved_len);

                self.emit(Op::StoreLocal(result_slot));
                let j = self.emit(Op::Jump(0));
                catch_end_jumps.push(j);

                // Patch the per-clause skip to the next clause (or the
                // fall-through, which leaves exc_slot set).
                let next_pc = self.here();
                self.cur_mut().code[skip_clause as usize] = Op::JumpIfFalsy(next_pc);
            }
            // No catch matched: exc_slot still holds the value; fall through
            // to finally/rethrow. result_slot ends up unread.
        }

        // Finally block.
        let finally_pc = self.here();
        self.cur_mut().code[handler_pc as usize] = Op::PushHandler(catch_l, exc_slot);
        self.cur_mut().code[jump_body_ok as usize] = Op::Jump(finally_pc);
        for j in catch_end_jumps {
            self.cur_mut().code[j as usize] = Op::Jump(finally_pc);
        }

        if let Some(ref fb) = finally_body {
            for f in fb.iter() {
                self.compile_form(py, f.clone_ref(py))?;
                self.emit(Op::Pop);
            }
        }

        // Re-throw check. If exc_slot is nil (body ok, or catch matched),
        // produce result_slot. Else throw.
        self.emit(Op::LoadLocal(exc_slot));
        let j_ok = self.emit(Op::JumpIfFalsy(0));
        self.emit(Op::LoadLocal(exc_slot));
        self.emit(Op::Throw);
        let ok_pc = self.here();
        self.cur_mut().code[j_ok as usize] = Op::JumpIfFalsy(ok_pc);
        self.emit(Op::LoadLocal(result_slot));

        self.cur_mut().tail = saved_tail;
        Ok(())
    }

    fn compile_invoke(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let items = list_items(py, &list_py)?;
        if items.is_empty() {
            return Err(errors::err("empty list cannot be invoked"));
        }
        let fn_form = items[0].clone_ref(py);
        let args = &items[1..];
        if args.len() > u8::MAX as usize {
            return Err(errors::err(format!(
                "invocation with {} args exceeds u8::MAX",
                args.len()
            )));
        }
        // Head + args are not in tail position (they feed the Invoke, which
        // itself can be in tail — but TCO is only for `recur`, so Invoke is
        // always a normal call).
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        self.compile_form(py, fn_form)?;
        for a in args {
            self.compile_form(py, a.clone_ref(py))?;
        }
        self.cur_mut().tail = saved_tail;
        self.emit(Op::Invoke(args.len() as u8));
        Ok(())
    }

    // --- Collection literals ---

    fn compile_collection_literal(
        &mut self,
        py: Python<'_>,
        form: PyObject,
        _n: usize,
    ) -> PyResult<()> {
        if is_literal(py, &form)? {
            let ix = self.cur_mut().pool.intern_const(form);
            self.emit(Op::PushConst(ix));
            return Ok(());
        }
        let pv = form.bind(py).cast::<PersistentVector>().unwrap().clone().unbind();
        let pv_ref = pv.bind(py).get();
        let vector_sym = Symbol::new(None, Arc::from("vector"));
        match self.resolve_symbol(py, &vector_sym)? {
            Resolved::Var(ix) => { self.emit(Op::Deref(ix)); }
            _ => return Err(errors::err("`vector` is shadowed — can't compile dynamic vector literal")),
        }
        let count = pv_ref.cnt as usize;
        if count > u8::MAX as usize {
            return Err(errors::err("vector literal too large"));
        }
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        for i in 0..count {
            let el = pv_ref.nth_internal_pub(py, i)?;
            self.compile_form(py, el)?;
        }
        self.cur_mut().tail = saved_tail;
        self.emit(Op::Invoke(count as u8));
        Ok(())
    }

    fn compile_map_literal(&mut self, py: Python<'_>, form: PyObject) -> PyResult<()> {
        if is_literal(py, &form)? {
            let ix = self.cur_mut().pool.intern_const(form);
            self.emit(Op::PushConst(ix));
            return Ok(());
        }
        let hash_map_sym = Symbol::new(None, Arc::from("hash-map"));
        match self.resolve_symbol(py, &hash_map_sym)? {
            Resolved::Var(ix) => { self.emit(Op::Deref(ix)); }
            _ => return Err(errors::err("`hash-map` is shadowed — can't compile dynamic map literal")),
        }
        let b = form.bind(py);
        let mut count: usize = 0;
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            self.compile_form(py, k)?;
            self.compile_form(py, v)?;
            count += 2;
        }
        self.cur_mut().tail = saved_tail;
        if count > u8::MAX as usize {
            return Err(errors::err("map literal too large"));
        }
        self.emit(Op::Invoke(count as u8));
        Ok(())
    }

    fn compile_set_literal(&mut self, py: Python<'_>, form: PyObject) -> PyResult<()> {
        if is_literal(py, &form)? {
            let ix = self.cur_mut().pool.intern_const(form);
            self.emit(Op::PushConst(ix));
            return Ok(());
        }
        let hash_set_sym = Symbol::new(None, Arc::from("hash-set"));
        match self.resolve_symbol(py, &hash_set_sym)? {
            Resolved::Var(ix) => { self.emit(Op::Deref(ix)); }
            _ => return Err(errors::err("`hash-set` is shadowed — can't compile dynamic set literal")),
        }
        let b = form.bind(py);
        let mut count: usize = 0;
        let saved_tail = self.cur().tail;
        self.cur_mut().tail = false;
        for item in b.try_iter()? {
            self.compile_form(py, item?.unbind())?;
            count += 1;
        }
        self.cur_mut().tail = saved_tail;
        if count > u8::MAX as usize {
            return Err(errors::err("set literal too large"));
        }
        self.emit(Op::Invoke(count as u8));
        Ok(())
    }

    /// Finalize a top-level compile into a 0-arity CompiledMethod.
    pub fn finish_top_level(mut self) -> (CompiledMethod, Arc<FnPool>) {
        self.emit(Op::Return);
        {
            let cur = self.cur_mut();
            let (code, pool) = (&mut cur.code, &mut cur.pool);
            fuse_deref_invoke_pass(code, pool);
        }
        let ctx = self.fns.pop().unwrap();
        debug_assert!(self.fns.is_empty(), "top-level compile leaked a nested fn ctx");
        let method = CompiledMethod {
            arity: 0,
            is_variadic: false,
            local_slots: ctx.next_slot,
            code: ctx.code,
        };
        (method, ctx.pool.finish())
    }
}

// ---- Helpers ----

#[derive(Clone, Copy)]
enum ClauseKind {
    Body,
    Catch,
    Finally,
}

/// Classify a `try` subform: bare list whose head is the symbol `catch` or
/// `finally` is a special clause; anything else is part of the body.
fn clause_head_kind(py: Python<'_>, form: &PyObject) -> ClauseKind {
    let b = form.bind(py);
    let Ok(pl) = b.cast::<PersistentList>() else {
        return ClauseKind::Body;
    };
    let head = pl.get().head.clone_ref(py);
    let hb = head.bind(py);
    let Ok(sym_ref) = hb.cast::<Symbol>() else {
        return ClauseKind::Body;
    };
    let s = sym_ref.get();
    if s.ns.is_some() {
        return ClauseKind::Body;
    }
    match s.name.as_ref() {
        "catch" => ClauseKind::Catch,
        "finally" => ClauseKind::Finally,
        _ => ClauseKind::Body,
    }
}

pub fn list_rest(py: Python<'_>, list: &PyObject) -> PyResult<Vec<PyObject>> {
    let b = list.bind(py);
    let pl = b.cast::<PersistentList>().map_err(|_| {
        errors::err("list_rest: not a PersistentList")
    })?;
    walk_list_from(py, pl.get().tail.clone_ref(py))
}

pub fn list_items(py: Python<'_>, list: &PyObject) -> PyResult<Vec<PyObject>> {
    let b = list.bind(py);
    if let Ok(pl) = b.cast::<PersistentList>() {
        let mut out = vec![pl.get().head.clone_ref(py)];
        let mut tail = pl.get().tail.clone_ref(py);
        loop {
            let tb = tail.bind(py);
            if tb.cast::<EmptyList>().is_ok() { break; }
            if let Ok(p2) = tb.cast::<PersistentList>() {
                out.push(p2.get().head.clone_ref(py));
                tail = p2.get().tail.clone_ref(py);
                continue;
            }
            break;
        }
        Ok(out)
    } else if b.cast::<EmptyList>().is_ok() {
        Ok(Vec::new())
    } else {
        Err(errors::err("list_items: not a PersistentList"))
    }
}

fn walk_list_from(py: Python<'_>, start: PyObject) -> PyResult<Vec<PyObject>> {
    let mut out = Vec::new();
    let mut cur = start;
    loop {
        let b = cur.bind(py);
        if b.cast::<EmptyList>().is_ok() { break; }
        if let Ok(pl) = b.cast::<PersistentList>() {
            out.push(pl.get().head.clone_ref(py));
            cur = pl.get().tail.clone_ref(py);
            continue;
        }
        break;
    }
    Ok(out)
}

/// True if `form` is a sequential value other than a PersistentList —
/// e.g. Cons, LazySeq, VectorSeq produced by macros/core fns.
pub fn is_non_list_seq(py: Python<'_>, form: &PyObject) -> PyResult<bool> {
    let b = form.bind(py);
    if b.cast::<PersistentList>().is_ok() || b.cast::<EmptyList>().is_ok() {
        return Ok(false);
    }
    if b.cast::<crate::seqs::cons::Cons>().is_ok() {
        return Ok(true);
    }
    if b.cast::<crate::seqs::lazy_seq::LazySeq>().is_ok() {
        return Ok(true);
    }
    if b.cast::<crate::seqs::vector_seq::VectorSeq>().is_ok() {
        return Ok(true);
    }
    Ok(false)
}

/// Iterate a seq via Python iteration, collecting all items into a Vec.
pub fn collect_seq(py: Python<'_>, form: &PyObject) -> PyResult<Vec<PyObject>> {
    let b = form.bind(py);
    let mut out = Vec::new();
    for item in b.try_iter()? {
        out.push(item?.unbind());
    }
    let _ = py;
    Ok(out)
}

/// Build a PersistentList from a slice of items (head-first).
pub fn make_plist(py: Python<'_>, items: &[PyObject]) -> PyResult<PyObject> {
    let tup = pyo3::types::PyTuple::new(py, items)?;
    crate::collections::plist::list_(py, tup)
}

/// Find the Var a Symbol resolves to, without interning or derefing.
/// Qualified symbols look in the specified ns; unqualified check current-ns
/// then clojure.core.
pub fn find_var(
    py: Python<'_>,
    sym: &Symbol,
    current_ns: &PyObject,
) -> PyResult<Option<Py<crate::var::Var>>> {
    if let Some(ns_name) = sym.ns.as_deref() {
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let target_ns = match modules.get_item(ns_name) {
            Ok(ns) => ns,
            Err(_) => return Ok(None),
        };
        if let Ok(attr) = target_ns.getattr(sym.name.as_ref()) {
            if let Ok(var) = attr.cast::<crate::var::Var>() {
                return Ok(Some(var.clone().unbind()));
            }
        }
        return Ok(None);
    }
    let cur_bound = current_ns.bind(py);
    if let Ok(attr) = cur_bound.getattr(sym.name.as_ref()) {
        if let Ok(var) = attr.cast::<crate::var::Var>() {
            return Ok(Some(var.clone().unbind()));
        }
    }
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if let Ok(core_ns) = modules.get_item("clojure.core") {
        if let Ok(attr) = core_ns.getattr(sym.name.as_ref()) {
            if let Ok(var) = attr.cast::<crate::var::Var>() {
                return Ok(Some(var.clone().unbind()));
            }
        }
    }
    Ok(None)
}

/// Count how many OUTER-scope reads a single form will produce for a given
/// unqualified name. Recurses into subforms. `fn*` forms count as 1 per
/// captured name (that's one `LoadLocal` at `MakeFn` time, regardless of
/// how often the inner body uses the local). Shadowing inside let/loop/fn
/// stops the recursion for that subtree.
pub fn count_outer_refs_in_form(
    py: Python<'_>,
    form: &PyObject,
    name: &str,
) -> PyResult<usize> {
    let b = form.bind(py);

    // Symbol: direct match is 1, miss is 0.
    if let Ok(sym_ref) = b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() && s.name.as_ref() == name {
            return Ok(1);
        }
        return Ok(0);
    }

    // Atoms.
    if form.is_none(py)
        || b.cast::<PyBool>().is_ok()
        || b.cast::<PyInt>().is_ok()
        || b.cast::<PyFloat>().is_ok()
        || b.cast::<PyString>().is_ok()
        || b.cast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(0);
    }

    // Collections (vector / map / set) — count inner references.
    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        let mut total: usize = 0;
        for i in 0..(pv_ref.cnt as usize) {
            let el = pv_ref.nth_internal_pub(py, i)?;
            total = total.saturating_add(count_outer_refs_in_form(py, &el, name)?);
        }
        return Ok(total);
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        let mut total: usize = 0;
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            total = total.saturating_add(count_outer_refs_in_form(py, &k, name)?);
            total = total.saturating_add(count_outer_refs_in_form(py, &v, name)?);
        }
        return Ok(total);
    }
    if b.cast::<PersistentHashSet>().is_ok() {
        let mut total: usize = 0;
        for item in b.try_iter()? {
            total = total.saturating_add(count_outer_refs_in_form(py, &item?.unbind(), name)?);
        }
        return Ok(total);
    }

    // Lists — check for special scoping forms.
    let items = if let Ok(_pl) = b.cast::<PersistentList>() {
        list_items(py, form)?
    } else if b.cast::<EmptyList>().is_ok() {
        return Ok(0);
    } else if is_non_list_seq(py, form)? {
        collect_seq(py, form)?
    } else {
        return Ok(0);
    };
    if items.is_empty() { return Ok(0); }

    // Head-symbol special cases for shadowing.
    let head = items[0].clone_ref(py);
    let head_b = head.bind(py);
    if let Ok(sym_ref) = head_b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            match s.name.as_ref() {
                "quote" => return Ok(0),  // everything inside is data
                "fn*" => {
                    return count_fn_captures(py, &items[1..], name);
                }
                "let*" | "loop*" => {
                    return count_let_refs(py, &items[1..], name);
                }
                "letfn*" => {
                    return count_letfn_refs(py, &items[1..], name);
                }
                _ => {}
            }
        }
        // If the head resolves to a macro Var, macroexpansion will produce
        // forms this pre-scan can't see (e.g. `do-template` emits N copies
        // of the template expression). Undercounting here would cause the
        // compiler's liveness tracker to clear the local too early, so the
        // second macro-generated reference would see nil. Return
        // `usize::MAX` as a "don't use this count" sentinel.
        // Note: qualified heads (ns/name) also need this check — syntax-
        // quote emits fully-qualified forms.
        if head_looks_like_macro(py, s)? {
            return Ok(usize::MAX);
        }
    }

    // Default: count refs in every item (including the head, which may be
    // the symbol we're looking for in the fn-call position).
    let mut total: usize = 0;
    for item in &items {
        let c = count_outer_refs_in_form(py, item, name)?;
        // Saturate at usize::MAX so macro sentinels propagate upward.
        total = total.saturating_add(c);
    }
    Ok(total)
}

fn head_looks_like_macro(py: Python<'_>, s: &Symbol) -> PyResult<bool> {
    let name = s.name.as_ref();
    // Fast-reject ambient names that are known not to be macros. Keeps us
    // from walking a namespace for every `(+ a b)` call.
    if matches!(
        name,
        "+" | "-" | "*" | "/" | "=" | "<" | ">" | "<=" | ">="
            | "inc" | "dec" | "not" | "first" | "rest" | "next" | "cons"
            | "list" | "vector" | "hash-map" | "hash-set"
            | "nth" | "get" | "count" | "conj" | "assoc" | "dissoc"
            | "apply" | "map" | "filter" | "reduce" | "into"
            | "identity" | "seq" | "empty?"
    ) {
        return Ok(false);
    }
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    // Resolve to a Var. Search order:
    //   1. ns-qualified → that ns.
    //   2. unqualified → the CURRENT compile ns (to catch refers like
    //      `are`/`do-template` that live in clojure.test or elsewhere).
    //   3. unqualified → clojure.core as a fallback for eval-from-Python
    //      paths where the compile ns is clojure.user or fresh.
    let current_ns_opt: Option<PyObject> =
        crate::eval::load::CURRENT_LOAD_NS.with(|c| c.borrow().as_ref().map(|n| n.clone_ref(py)));
    let resolved: Option<Py<crate::var::Var>> = if let Some(ns_name) = s.ns.as_deref() {
        modules
            .get_item(ns_name)
            .ok()
            .and_then(|m| m.getattr(name).ok())
            .and_then(|v| v.cast::<crate::var::Var>().ok().map(|v| v.clone().unbind()))
    } else {
        let from_current = current_ns_opt
            .as_ref()
            .and_then(|ns| ns.bind(py).getattr(name).ok())
            .and_then(|v| v.cast::<crate::var::Var>().ok().map(|v| v.clone().unbind()));
        from_current.or_else(|| {
            modules
                .get_item("clojure.core")
                .ok()
                .and_then(|m| m.getattr(name).ok())
                .and_then(|v| v.cast::<crate::var::Var>().ok().map(|v| v.clone().unbind()))
        })
    };
    match resolved {
        Some(v) => Ok(v.bind(py).get().is_macro(py)),
        None => Ok(false),
    }
}

/// Sum `count_outer_refs_in_form` over a sequence of body forms.
pub fn count_outer_refs_in_forms(
    py: Python<'_>,
    forms: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    let mut total: usize = 0;
    for f in forms {
        total = total.saturating_add(count_outer_refs_in_form(py, f, name)?);
    }
    Ok(total)
}

/// For a `(fn ...)` — return 1 if `name` is captured by any of its arity
/// bodies (used and not shadowed by that arity's params), else 0.
fn count_fn_captures(
    py: Python<'_>,
    after_fn_head: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    if after_fn_head.is_empty() { return Ok(0); }
    // Skip an optional name symbol.
    let rest: &[PyObject] = if let Ok(_) = after_fn_head[0].bind(py).cast::<Symbol>() {
        &after_fn_head[1..]
    } else {
        after_fn_head
    };
    if rest.is_empty() { return Ok(0); }

    let specs: Vec<(PyObject, Vec<PyObject>)> = {
        let first_b = rest[0].bind(py);
        if first_b.cast::<PersistentVector>().is_ok() {
            vec![(rest[0].clone_ref(py), rest[1..].iter().map(|o| o.clone_ref(py)).collect())]
        } else if first_b.cast::<PersistentList>().is_ok() {
            let mut specs = Vec::new();
            for item in rest {
                let items = list_items(py, item)?;
                if items.is_empty() { continue; }
                let params = items[0].clone_ref(py);
                let body: Vec<PyObject> = items[1..].iter().map(|o| o.clone_ref(py)).collect();
                specs.push((params, body));
            }
            specs
        } else {
            return Ok(0);
        }
    };

    for (params, body) in &specs {
        let (param_names, _) = match parse_params(py, params) {
            Ok(x) => x,
            Err(_) => continue,
        };
        if param_names.iter().any(|n| n.as_ref() == name) {
            continue;  // shadowed in this arity — doesn't capture
        }
        // If the body references `name`, this arity captures it.
        for f in body {
            if count_outer_refs_in_form(py, f, name)? > 0 {
                return Ok(1);  // one LoadLocal at MakeFn time, regardless of inner-body count
            }
        }
    }
    Ok(0)
}

/// Handle `(let [n v ...] body...)` — v exprs are visible with the outer
/// scope; body sees shadowing by any binding name == `name`.
fn count_let_refs(
    py: Python<'_>,
    after_let_head: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    if after_let_head.is_empty() { return Ok(0); }
    let bindings = &after_let_head[0];
    let body = &after_let_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(0),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(0); }
    let mut total: usize = 0;
    let mut shadowed = false;
    let n = pv_ref.cnt as usize;
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let val = pv_ref.nth_internal_pub(py, i + 1)?;
        // Value expr uses the outer binding (unless already shadowed by a
        // prior binding in this same let).
        if !shadowed {
            total = total.saturating_add(count_outer_refs_in_form(py, &val, name)?);
        }
        // Check if this binding shadows `name`.
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                shadowed = true;
            }
        }
        i += 2;
    }
    if !shadowed {
        for f in body {
            total = total.saturating_add(count_outer_refs_in_form(py, f, name)?);
        }
    }
    Ok(total)
}

/// `letfn*` shadowing: all bound names are visible *throughout* the form
/// (value forms and body). If `name` is one of the bound names, the entire
/// form contributes 0 outer-scope refs. Otherwise sum refs across all
/// value forms and body forms.
fn count_letfn_refs(
    py: Python<'_>,
    after_head: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    if after_head.is_empty() { return Ok(0); }
    let bindings = &after_head[0];
    let body = &after_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(0),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(0); }
    let n = pv_ref.cnt as usize;
    // First pass: any binding name == `name` shadows the whole form.
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                return Ok(0);
            }
        }
        i += 2;
    }
    // Not shadowed: count refs across value forms and body.
    let mut total: usize = 0;
    let mut i = 1;
    while i < n {
        let val = pv_ref.nth_internal_pub(py, i)?;
        total = total.saturating_add(count_outer_refs_in_form(py, &val, name)?);
        i += 2;
    }
    for f in body {
        total = total.saturating_add(count_outer_refs_in_form(py, f, name)?);
    }
    Ok(total)
}

/// Return the subset of `names` captured by at least one inner fn* in the
/// body. Used to blacklist those locals from mid-body clearing (though
/// scope-end clearing is still safe for them).
pub fn body_has_fn_capturing(
    py: Python<'_>,
    body: &[PyObject],
    name_to_slot: &std::collections::HashMap<Arc<str>, u16>,
) -> PyResult<std::collections::HashSet<String>> {
    let mut out = std::collections::HashSet::new();
    for (name, _) in name_to_slot.iter() {
        let mut captured = false;
        for f in body {
            if form_has_fn_capturing(py, f, name.as_ref())? {
                captured = true;
                break;
            }
        }
        if captured {
            out.insert(name.to_string());
        }
    }
    Ok(out)
}

fn form_has_fn_capturing(
    py: Python<'_>,
    form: &PyObject,
    name: &str,
) -> PyResult<bool> {
    let b = form.bind(py);

    if b.cast::<Symbol>().is_ok()
        || form.is_none(py)
        || b.cast::<PyBool>().is_ok()
        || b.cast::<PyInt>().is_ok()
        || b.cast::<PyFloat>().is_ok()
        || b.cast::<PyString>().is_ok()
        || b.cast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(false);
    }

    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        for i in 0..(pv_ref.cnt as usize) {
            if form_has_fn_capturing(py, &pv_ref.nth_internal_pub(py, i)?, name)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            if form_has_fn_capturing(py, &k, name)? { return Ok(true); }
            if form_has_fn_capturing(py, &v, name)? { return Ok(true); }
        }
        return Ok(false);
    }
    if b.cast::<PersistentHashSet>().is_ok() {
        for item in b.try_iter()? {
            if form_has_fn_capturing(py, &item?.unbind(), name)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    let items = if let Ok(_pl) = b.cast::<PersistentList>() {
        list_items(py, form)?
    } else if b.cast::<EmptyList>().is_ok() {
        return Ok(false);
    } else if is_non_list_seq(py, form)? {
        collect_seq(py, form)?
    } else {
        return Ok(false);
    };
    if items.is_empty() { return Ok(false); }

    let head_b = items[0].bind(py);
    if let Ok(sym_ref) = head_b.cast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            if s.name.as_ref() == "quote" {
                return Ok(false);
            }
            if s.name.as_ref() == "fn*" {
                return fn_captures_name(py, &items[1..], name);
            }
            if matches!(s.name.as_ref(), "let*" | "loop*") {
                return let_captures_name(py, &items[1..], name);
            }
            if s.name.as_ref() == "letfn*" {
                return letfn_captures_name(py, &items[1..], name);
            }
        }
    }
    for item in &items {
        if form_has_fn_capturing(py, item, name)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn fn_captures_name(
    py: Python<'_>,
    after_fn_head: &[PyObject],
    name: &str,
) -> PyResult<bool> {
    Ok(count_fn_captures(py, after_fn_head, name)? > 0)
}

fn let_captures_name(
    py: Python<'_>,
    after_let_head: &[PyObject],
    name: &str,
) -> PyResult<bool> {
    if after_let_head.is_empty() { return Ok(false); }
    let bindings = &after_let_head[0];
    let body = &after_let_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(false); }
    let mut shadowed = false;
    let n = pv_ref.cnt as usize;
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let val = pv_ref.nth_internal_pub(py, i + 1)?;
        if !shadowed && form_has_fn_capturing(py, &val, name)? {
            return Ok(true);
        }
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                shadowed = true;
            }
        }
        i += 2;
    }
    if !shadowed {
        for f in body {
            if form_has_fn_capturing(py, f, name)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn letfn_captures_name(
    py: Python<'_>,
    after_head: &[PyObject],
    name: &str,
) -> PyResult<bool> {
    if after_head.is_empty() { return Ok(false); }
    let bindings = &after_head[0];
    let body = &after_head[1..];
    let b = bindings.bind(py);
    let pv = match b.cast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(false); }
    let n = pv_ref.cnt as usize;
    // Shadowing: any binding name == `name` blocks the entire form.
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.cast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                return Ok(false);
            }
        }
        i += 2;
    }
    let mut i = 1;
    while i < n {
        let val = pv_ref.nth_internal_pub(py, i)?;
        if form_has_fn_capturing(py, &val, name)? { return Ok(true); }
        i += 2;
    }
    for f in body {
        if form_has_fn_capturing(py, f, name)? { return Ok(true); }
    }
    Ok(false)
}

/// Parse a fn params vector like `[x y & rest]`.
/// Returns `(param_names, is_variadic)`. For variadic, the last element of
/// `param_names` is the rest arg — its slot holds a seq (or nil) at runtime.
pub fn parse_params(
    py: Python<'_>,
    params_form: &PyObject,
) -> PyResult<(Vec<Arc<str>>, bool)> {
    let b = params_form.bind(py);
    let pv = b.cast::<PersistentVector>().map_err(|_| {
        errors::err("fn parameters must be a vector")
    })?;
    let v = pv.get();
    let n = v.cnt as usize;
    let mut names: Vec<Arc<str>> = Vec::with_capacity(n);
    let mut seen_amp = false;
    let mut is_variadic = false;
    for i in 0..n {
        let f = v.nth_internal_pub(py, i)?;
        let fb = f.bind(py);
        let s = fb.cast::<Symbol>().map_err(|_| {
            errors::err("fn parameter must be a Symbol")
        })?;
        let name_ref = s.get().name.clone();
        if name_ref.as_ref() == "&" {
            if seen_amp {
                return Err(errors::err("fn params: only one `&` allowed"));
            }
            seen_amp = true;
            continue;
        }
        if seen_amp {
            // This is the rest-arg name. Expect exactly one after `&`.
            if is_variadic {
                return Err(errors::err(
                    "fn params: `&` must be followed by exactly one name",
                ));
            }
            is_variadic = true;
            names.push(name_ref);
        } else {
            names.push(name_ref);
        }
    }
    if seen_amp && !is_variadic {
        return Err(errors::err("fn params: `&` must be followed by a name"));
    }
    Ok((names, is_variadic))
}

pub fn is_literal(py: Python<'_>, form: &PyObject) -> PyResult<bool> {
    let b = form.bind(py);
    if form.is_none(py) { return Ok(true); }
    if b.cast::<PyBool>().is_ok()
        || b.cast::<PyInt>().is_ok()
        || b.cast::<PyFloat>().is_ok()
        || b.cast::<PyString>().is_ok()
        || b.cast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(true);
    }
    if b.cast::<Symbol>().is_ok() {
        return Ok(false);
    }
    if b.cast::<PersistentList>().is_ok() {
        // A `(quote X)` form inside a collection is NOT a const-literal:
        // if the outer vector/map/set is hoisted to a constant, the quote
        // form itself gets pushed (unevaluated), producing
        // `[(quote a) …]` instead of the expected `[a …]`. Fall back to
        // the dynamic build path so each element compiles via
        // `compile_form`, which handles `quote` as a special form.
        return Ok(false);
    }
    if b.cast::<EmptyList>().is_ok() { return Ok(true); }
    if let Ok(pv) = b.cast::<PersistentVector>() {
        let pv_ref = pv.get();
        for i in 0..(pv_ref.cnt as usize) {
            let child = pv_ref.nth_internal_pub(py, i)?;
            if !is_literal(py, &child)? { return Ok(false); }
        }
        return Ok(true);
    }
    if b.cast::<PersistentHashMap>().is_ok() || b.cast::<PersistentArrayMap>().is_ok() {
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            if !is_literal(py, &k)? || !is_literal(py, &v)? { return Ok(false); }
        }
        return Ok(true);
    }
    if b.cast::<PersistentHashSet>().is_ok() {
        for item in b.try_iter()? {
            let el = item?.unbind();
            if !is_literal(py, &el)? { return Ok(false); }
        }
        return Ok(true);
    }
    Ok(true)
}

/// Build the merged metadata map for a `def`, combining (optional) symbol
/// meta read by the reader (`^{...}`) with an (optional) docstring from the
/// 3-arg def form. The docstring assoc-s onto the symbol meta under `:doc`.
fn merge_doc_into_meta(
    py: Python<'_>,
    sym_meta: Option<PyObject>,
    docstring: Option<PyObject>,
) -> PyResult<Option<PyObject>> {
    match (sym_meta, docstring) {
        (None, None) => Ok(None),
        (Some(m), None) => Ok(Some(m)),
        (existing, Some(doc)) => {
            let kw_doc: PyObject = crate::keyword::keyword(py, "doc", None)?.into_any();
            match existing {
                None => {
                    let tup = pyo3::types::PyTuple::new(py, &[kw_doc, doc])?;
                    Ok(Some(crate::collections::parraymap::array_map(py, tup)?))
                }
                Some(m) => {
                    let new_m = m.bind(py).call_method1("assoc", (kw_doc, doc))?.unbind();
                    Ok(Some(new_m))
                }
            }
        }
    }
}

