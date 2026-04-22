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

#[derive(Clone)]
pub struct LocalBinding {
    pub name: Arc<str>,
    pub slot: u16,
}

/// A single closure capture: where, in the immediately enclosing fn's
/// frame, the captured value comes from.
#[derive(Clone)]
pub struct CaptureBinding {
    pub name: Arc<str>,
    pub source: CaptureSource,
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
        }
    }
}

/// Resolution outcome for a Symbol reference, relative to the current fn ctx.
pub enum Resolved {
    Local(u16),
    Capture(u16),
    Var(u16),
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
        self.cur_mut().locals.push(LocalBinding { name, slot });
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
        if let Some(slot) = self.cur().locals.iter().rev()
            .find(|lb| lb.name.as_ref() == name).map(|lb| lb.slot)
        {
            return Some(Resolved::Local(slot));
        }
        // Walk upward looking for a local or capture with this name.
        let mut found: Option<(usize, CaptureSource)> = None;
        for (i, ctx) in self.fns.iter().enumerate().rev().skip(1) {
            if let Some(slot) = ctx.locals.iter().rev()
                .find(|lb| lb.name.as_ref() == name).map(|lb| lb.slot)
            {
                found = Some((i, CaptureSource::Local(slot)));
                break;
            }
            if let Some(ix) = ctx.captures.iter().position(|cb| cb.name.as_ref() == name) {
                found = Some((i, CaptureSource::Capture(ix as u16)));
                break;
            }
        }
        let (found_at, initial_source) = found?;
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
                });
                ix
            };
            source = CaptureSource::Capture(cap_ix);
        }
        // The current ctx's own capture index:
        match source {
            CaptureSource::Capture(ix) => Some(Resolved::Capture(ix)),
            CaptureSource::Local(_) => unreachable!(), // we always chained through at least one
        }
    }

    pub fn resolve_symbol(&mut self, py: Python<'_>, sym: &Symbol) -> PyResult<Resolved> {
        if let Some(ns_name) = sym.ns.as_deref() {
            let sys = py.import("sys")?;
            let modules = sys.getattr("modules")?;
            let target_ns = modules.get_item(ns_name).map_err(|_| {
                errors::err(format!("No namespace: {}", ns_name))
            })?;
            let attr = target_ns.getattr(sym.name.as_ref()).map_err(|_| {
                errors::err(format!("Unable to resolve: {}/{}", ns_name, sym.name))
            })?;
            let var = attr.downcast::<crate::var::Var>().map_err(|_| {
                errors::err(format!("{}/{} does not resolve to a Var", ns_name, sym.name))
            })?;
            let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
            return Ok(Resolved::Var(ix));
        }

        if let Some(r) = self.resolve_local_or_capture(sym.name.as_ref()) {
            return Ok(r);
        }

        let current = self.current_ns.bind(py);
        if let Ok(attr) = current.getattr(sym.name.as_ref()) {
            if let Ok(var) = attr.downcast::<crate::var::Var>() {
                let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
                return Ok(Resolved::Var(ix));
            }
        }

        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        if let Ok(core_ns) = modules.get_item("clojure.core") {
            if let Ok(attr) = core_ns.getattr(sym.name.as_ref()) {
                if let Ok(var) = attr.downcast::<crate::var::Var>() {
                    let ix = self.cur_mut().pool.intern_var(py, var.clone().unbind());
                    return Ok(Resolved::Var(ix));
                }
            }
        }

        Err(errors::err(format!(
            "Unable to resolve symbol: {} in this context",
            sym.name
        )))
    }

    /// For `def` — interns a fresh Var in current-ns and adds it to the
    /// pool. Does NOT deref; returns the pool index.
    pub fn intern_def_target(&mut self, py: Python<'_>, sym: &Symbol) -> PyResult<u16> {
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
        Ok(self.cur_mut().pool.intern_var(py, var))
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
        let sym_ref = match hb.downcast::<Symbol>() {
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
        let var = attr.downcast::<crate::var::Var>().map_err(|_| {
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
        if b.downcast::<PyBool>().is_ok()
            || b.downcast::<PyInt>().is_ok()
            || b.downcast::<PyFloat>().is_ok()
            || b.downcast::<PyString>().is_ok()
            || b.downcast::<crate::keyword::Keyword>().is_ok()
        {
            let ix = self.cur_mut().pool.intern_const(form);
            self.emit(Op::PushConst(ix));
            return Ok(());
        }

        if let Ok(sym_ref) = b.downcast::<Symbol>() {
            match self.resolve_symbol(py, sym_ref.get())? {
                Resolved::Local(slot) => { self.emit_load_local(slot); }
                Resolved::Capture(ix) => { self.emit(Op::LoadCapture(ix)); }
                Resolved::Var(ix) => { self.emit(Op::Deref(ix)); }
            }
            return Ok(());
        }

        if let Ok(pl) = b.downcast::<PersistentList>() {
            let head = pl.get().head.clone_ref(py);
            return self.compile_list_form(py, form.clone_ref(py), head);
        }
        if b.downcast::<EmptyList>().is_ok() {
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

        if let Ok(pv) = b.downcast::<PersistentVector>() {
            return self.compile_collection_literal(py, form.clone_ref(py), pv.get().cnt as usize);
        }
        if b.downcast::<PersistentHashMap>().is_ok() || b.downcast::<PersistentArrayMap>().is_ok() {
            return self.compile_map_literal(py, form);
        }
        if b.downcast::<PersistentHashSet>().is_ok() {
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
        // Built-in macroexpansion (defn, defmacro, when, cond, or, and, …).
        if let Some(macro_name) = crate::eval::macros::lookup_builtin_macro(&head, py) {
            let expanded = crate::eval::macros::expand(py, macro_name, list_py)?;
            return self.compile_form(py, expanded);
        }

        // User-defined macroexpansion: head resolves to a Var with :macro meta.
        if let Some(expanded) = self.try_macroexpand_user(py, &list_py, &head)? {
            return self.compile_form(py, expanded);
        }

        let hb = head.bind(py);
        if let Ok(sym_ref) = hb.downcast::<Symbol>() {
            let s = sym_ref.get();
            if s.ns.is_none() {
                let n = s.name.as_ref();
                match n {
                    "quote" => return self.compile_quote(py, list_py),
                    "if" => return self.compile_if(py, list_py),
                    "do" => return self.compile_do(py, list_py),
                    "let" | "let*" => return self.compile_let(py, list_py, false),
                    "loop" | "loop*" => return self.compile_let(py, list_py, true),
                    "recur" => return self.compile_recur(py, list_py),
                    "var" => return self.compile_var_form(py, list_py),
                    "fn" | "fn*" => return self.compile_fn_form(py, list_py),
                    "def" => return self.compile_def(py, list_py),
                    "set!" => return self.compile_set_bang(py, list_py),
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
        let sym_ref = b.downcast::<Symbol>().map_err(|_| {
            errors::err("var's argument must be a Symbol")
        })?;
        let s = sym_ref.get();
        let target_ns_py: Bound<'_, PyAny> = match s.ns.as_deref() {
            Some(n) => {
                let sys = py.import("sys")?;
                let modules = sys.getattr("modules")?;
                modules.get_item(n).map_err(|_| {
                    errors::err(format!("No namespace: {} found in (var ...)", n))
                })?
            }
            None => self.current_ns.bind(py).clone(),
        };
        let attr = target_ns_py.getattr(s.name.as_ref()).map_err(|_| {
            errors::err(format!("Unable to resolve var: {}", s.name))
        })?;
        let var = attr.downcast::<crate::var::Var>().map_err(|_| {
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
        let bv = bindings.downcast::<PersistentVector>().map_err(|_| {
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
            let name_sym = name_b.downcast::<Symbol>().map_err(|_| {
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
                    // Don't mid-body clear — an inner fn* captures this local.
                    self.cur_mut().no_clear_slots.insert(*slot);
                    continue;
                }
                let count = count_outer_refs_in_forms(py, &body, name.as_ref())?;
                if count > 0 {
                    self.cur_mut().remaining_uses.insert(*slot, count);
                }
            }
        } else {
            // Loop slots never mid-body clear.
            for (_, slot) in &new_names_slots {
                self.cur_mut().no_clear_slots.insert(*slot);
            }
        }

        let saved_loop = if is_loop {
            let top = self.here();
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
                self.cur_mut().tail = saved_tail && i == last;
                self.compile_form(py, form.clone_ref(py))?;
                if i != last { self.emit(Op::Pop); }
            }
        }
        self.cur_mut().tail = saved_tail;

        if is_loop {
            self.cur_mut().loop_target = saved_loop.unwrap();
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

    /// `(def foo init)` → Deref(bind_root_ix); LoadVar(target); <init>; Invoke(2).
    fn compile_def(&mut self, py: Python<'_>, list_py: PyObject) -> PyResult<()> {
        let args = list_rest(py, &list_py)?;
        if args.is_empty() || args.len() > 2 {
            return Err(errors::err(format!(
                "def requires 1 or 2 arguments (got {})",
                args.len()
            )));
        }
        let name_form = args[0].bind(py);
        let sym_ref = name_form.downcast::<Symbol>().map_err(|_| {
            errors::err("def's first argument must be a Symbol")
        })?;
        let target_ix = self.intern_def_target(py, sym_ref.get())?;

        if args.len() == 2 {
            let bind_root_ix = self.resolve_core_var(py, "bind-root")?;
            self.emit(Op::Deref(bind_root_ix));
            self.emit(Op::LoadVar(target_ix));
            let saved_tail = self.cur().tail;
            self.cur_mut().tail = false;
            self.compile_form(py, args[1].clone_ref(py))?;
            self.cur_mut().tail = saved_tail;
            self.emit(Op::Invoke(2));
            // `bind-root` returns the Var; we keep that as the value of the def expr.
        } else {
            // (def foo) — just ensure Var exists. Push it.
            self.emit(Op::LoadVar(target_ix));
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
            if let Ok(sym_ref) = b0.downcast::<Symbol>() {
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
            if first_b.downcast::<PersistentVector>().is_ok() {
                // single arity: rest = [params body...]
                vec![(rest[0].clone_ref(py), rest[1..].iter().map(|o| o.clone_ref(py)).collect())]
            } else if first_b.downcast::<PersistentList>().is_ok() {
                let mut specs = Vec::with_capacity(rest.len());
                for item in rest {
                    let items = list_items(py, item)?;
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
                return Err(errors::err("fn: expected parameter vector or arity list"));
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
        if let Ok(pl) = args[1].bind(py).downcast::<PersistentList>() {
            let head = pl.get().head.clone_ref(py);
            let hb = head.bind(py);
            let method_sym = hb.downcast::<Symbol>().map_err(|_| {
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
        let sym_ref = member.downcast::<Symbol>().map_err(|_| {
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
        let items: Vec<PyObject> = if let Ok(_pl) = target_b.downcast::<PersistentList>() {
            list_items(py, target)?
        } else {
            return Err(errors::err("set!: only (.-attr obj) targets are supported"));
        };
        if items.is_empty() {
            return Err(errors::err("set!: empty target"));
        }
        let head_b = items[0].bind(py);
        let sym_ref = head_b.downcast::<Symbol>().map_err(|_| {
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
            let m_sym = m_b.downcast::<Symbol>().map_err(|_| {
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
        let pv = form.bind(py).downcast::<PersistentVector>().unwrap().clone().unbind();
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

pub fn list_rest(py: Python<'_>, list: &PyObject) -> PyResult<Vec<PyObject>> {
    let b = list.bind(py);
    let pl = b.downcast::<PersistentList>().map_err(|_| {
        errors::err("list_rest: not a PersistentList")
    })?;
    walk_list_from(py, pl.get().tail.clone_ref(py))
}

pub fn list_items(py: Python<'_>, list: &PyObject) -> PyResult<Vec<PyObject>> {
    let b = list.bind(py);
    if let Ok(pl) = b.downcast::<PersistentList>() {
        let mut out = vec![pl.get().head.clone_ref(py)];
        let mut tail = pl.get().tail.clone_ref(py);
        loop {
            let tb = tail.bind(py);
            if tb.downcast::<EmptyList>().is_ok() { break; }
            if let Ok(p2) = tb.downcast::<PersistentList>() {
                out.push(p2.get().head.clone_ref(py));
                tail = p2.get().tail.clone_ref(py);
                continue;
            }
            break;
        }
        Ok(out)
    } else if b.downcast::<EmptyList>().is_ok() {
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
        if b.downcast::<EmptyList>().is_ok() { break; }
        if let Ok(pl) = b.downcast::<PersistentList>() {
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
    if b.downcast::<PersistentList>().is_ok() || b.downcast::<EmptyList>().is_ok() {
        return Ok(false);
    }
    if b.downcast::<crate::seqs::cons::Cons>().is_ok() {
        return Ok(true);
    }
    if b.downcast::<crate::seqs::lazy_seq::LazySeq>().is_ok() {
        return Ok(true);
    }
    if b.downcast::<crate::seqs::vector_seq::VectorSeq>().is_ok() {
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
            if let Ok(var) = attr.downcast::<crate::var::Var>() {
                return Ok(Some(var.clone().unbind()));
            }
        }
        return Ok(None);
    }
    let cur_bound = current_ns.bind(py);
    if let Ok(attr) = cur_bound.getattr(sym.name.as_ref()) {
        if let Ok(var) = attr.downcast::<crate::var::Var>() {
            return Ok(Some(var.clone().unbind()));
        }
    }
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if let Ok(core_ns) = modules.get_item("clojure.core") {
        if let Ok(attr) = core_ns.getattr(sym.name.as_ref()) {
            if let Ok(var) = attr.downcast::<crate::var::Var>() {
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
    if let Ok(sym_ref) = b.downcast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() && s.name.as_ref() == name {
            return Ok(1);
        }
        return Ok(0);
    }

    // Atoms.
    if form.is_none(py)
        || b.downcast::<PyBool>().is_ok()
        || b.downcast::<PyInt>().is_ok()
        || b.downcast::<PyFloat>().is_ok()
        || b.downcast::<PyString>().is_ok()
        || b.downcast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(0);
    }

    // Collections (vector / map / set) — count inner references.
    if let Ok(pv) = b.downcast::<PersistentVector>() {
        let pv_ref = pv.get();
        let mut total = 0;
        for i in 0..(pv_ref.cnt as usize) {
            let el = pv_ref.nth_internal_pub(py, i)?;
            total += count_outer_refs_in_form(py, &el, name)?;
        }
        return Ok(total);
    }
    if b.downcast::<PersistentHashMap>().is_ok() || b.downcast::<PersistentArrayMap>().is_ok() {
        let mut total = 0;
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            total += count_outer_refs_in_form(py, &k, name)?;
            total += count_outer_refs_in_form(py, &v, name)?;
        }
        return Ok(total);
    }
    if b.downcast::<PersistentHashSet>().is_ok() {
        let mut total = 0;
        for item in b.try_iter()? {
            total += count_outer_refs_in_form(py, &item?.unbind(), name)?;
        }
        return Ok(total);
    }

    // Lists — check for special scoping forms.
    let items = if let Ok(_pl) = b.downcast::<PersistentList>() {
        list_items(py, form)?
    } else if b.downcast::<EmptyList>().is_ok() {
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
    if let Ok(sym_ref) = head_b.downcast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            match s.name.as_ref() {
                "quote" => return Ok(0),  // everything inside is data
                "fn" | "fn*" => {
                    return count_fn_captures(py, &items[1..], name);
                }
                "let" | "let*" | "loop" | "loop*" => {
                    return count_let_refs(py, &items[1..], name);
                }
                _ => {}
            }
        }
    }

    // Default: count refs in every item (including the head, which may be
    // the symbol we're looking for in the fn-call position).
    let mut total = 0;
    for item in &items {
        total += count_outer_refs_in_form(py, item, name)?;
    }
    Ok(total)
}

/// Sum `count_outer_refs_in_form` over a sequence of body forms.
pub fn count_outer_refs_in_forms(
    py: Python<'_>,
    forms: &[PyObject],
    name: &str,
) -> PyResult<usize> {
    let mut total = 0;
    for f in forms {
        total += count_outer_refs_in_form(py, f, name)?;
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
    let rest: &[PyObject] = if let Ok(_) = after_fn_head[0].bind(py).downcast::<Symbol>() {
        &after_fn_head[1..]
    } else {
        after_fn_head
    };
    if rest.is_empty() { return Ok(0); }

    let specs: Vec<(PyObject, Vec<PyObject>)> = {
        let first_b = rest[0].bind(py);
        if first_b.downcast::<PersistentVector>().is_ok() {
            vec![(rest[0].clone_ref(py), rest[1..].iter().map(|o| o.clone_ref(py)).collect())]
        } else if first_b.downcast::<PersistentList>().is_ok() {
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
    let pv = match b.downcast::<PersistentVector>() {
        Ok(p) => p,
        Err(_) => return Ok(0),
    };
    let pv_ref = pv.get();
    if pv_ref.cnt % 2 != 0 { return Ok(0); }
    let mut total = 0;
    let mut shadowed = false;
    let n = pv_ref.cnt as usize;
    let mut i = 0;
    while i < n {
        let bind_name = pv_ref.nth_internal_pub(py, i)?;
        let val = pv_ref.nth_internal_pub(py, i + 1)?;
        // Value expr uses the outer binding (unless already shadowed by a
        // prior binding in this same let).
        if !shadowed {
            total += count_outer_refs_in_form(py, &val, name)?;
        }
        // Check if this binding shadows `name`.
        let bn_b = bind_name.bind(py);
        if let Ok(sym_ref) = bn_b.downcast::<Symbol>() {
            if sym_ref.get().ns.is_none() && sym_ref.get().name.as_ref() == name {
                shadowed = true;
            }
        }
        i += 2;
    }
    if !shadowed {
        for f in body {
            total += count_outer_refs_in_form(py, f, name)?;
        }
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

    if b.downcast::<Symbol>().is_ok()
        || form.is_none(py)
        || b.downcast::<PyBool>().is_ok()
        || b.downcast::<PyInt>().is_ok()
        || b.downcast::<PyFloat>().is_ok()
        || b.downcast::<PyString>().is_ok()
        || b.downcast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(false);
    }

    if let Ok(pv) = b.downcast::<PersistentVector>() {
        let pv_ref = pv.get();
        for i in 0..(pv_ref.cnt as usize) {
            if form_has_fn_capturing(py, &pv_ref.nth_internal_pub(py, i)?, name)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if b.downcast::<PersistentHashMap>().is_ok() || b.downcast::<PersistentArrayMap>().is_ok() {
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            if form_has_fn_capturing(py, &k, name)? { return Ok(true); }
            if form_has_fn_capturing(py, &v, name)? { return Ok(true); }
        }
        return Ok(false);
    }
    if b.downcast::<PersistentHashSet>().is_ok() {
        for item in b.try_iter()? {
            if form_has_fn_capturing(py, &item?.unbind(), name)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    let items = if let Ok(_pl) = b.downcast::<PersistentList>() {
        list_items(py, form)?
    } else if b.downcast::<EmptyList>().is_ok() {
        return Ok(false);
    } else if is_non_list_seq(py, form)? {
        collect_seq(py, form)?
    } else {
        return Ok(false);
    };
    if items.is_empty() { return Ok(false); }

    let head_b = items[0].bind(py);
    if let Ok(sym_ref) = head_b.downcast::<Symbol>() {
        let s = sym_ref.get();
        if s.ns.is_none() {
            if s.name.as_ref() == "quote" {
                return Ok(false);
            }
            if s.name.as_ref() == "fn" || s.name.as_ref() == "fn*" {
                return fn_captures_name(py, &items[1..], name);
            }
            if matches!(s.name.as_ref(), "let" | "let*" | "loop" | "loop*") {
                return let_captures_name(py, &items[1..], name);
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
    let pv = match b.downcast::<PersistentVector>() {
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
        if let Ok(sym_ref) = bn_b.downcast::<Symbol>() {
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

/// Parse a fn params vector like `[x y & rest]`.
/// Returns `(param_names, is_variadic)`. For variadic, the last element of
/// `param_names` is the rest arg — its slot holds a seq (or nil) at runtime.
pub fn parse_params(
    py: Python<'_>,
    params_form: &PyObject,
) -> PyResult<(Vec<Arc<str>>, bool)> {
    let b = params_form.bind(py);
    let pv = b.downcast::<PersistentVector>().map_err(|_| {
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
        let s = fb.downcast::<Symbol>().map_err(|_| {
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
    if b.downcast::<PyBool>().is_ok()
        || b.downcast::<PyInt>().is_ok()
        || b.downcast::<PyFloat>().is_ok()
        || b.downcast::<PyString>().is_ok()
        || b.downcast::<crate::keyword::Keyword>().is_ok()
    {
        return Ok(true);
    }
    if b.downcast::<Symbol>().is_ok() {
        return Ok(false);
    }
    if let Ok(pl) = b.downcast::<PersistentList>() {
        let h = pl.get().head.clone_ref(py);
        let hb = h.bind(py);
        if let Ok(sym_ref) = hb.downcast::<Symbol>() {
            let s = sym_ref.get();
            if s.ns.is_none() && s.name.as_ref() == "quote" {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if b.downcast::<EmptyList>().is_ok() { return Ok(true); }
    if let Ok(pv) = b.downcast::<PersistentVector>() {
        let pv_ref = pv.get();
        for i in 0..(pv_ref.cnt as usize) {
            let child = pv_ref.nth_internal_pub(py, i)?;
            if !is_literal(py, &child)? { return Ok(false); }
        }
        return Ok(true);
    }
    if b.downcast::<PersistentHashMap>().is_ok() || b.downcast::<PersistentArrayMap>().is_ok() {
        for item in b.try_iter()? {
            let k = item?.unbind();
            let v = b.call_method1("val_at", (k.clone_ref(py),))?.unbind();
            if !is_literal(py, &k)? || !is_literal(py, &v)? { return Ok(false); }
        }
        return Ok(true);
    }
    if b.downcast::<PersistentHashSet>().is_ok() {
        for item in b.try_iter()? {
            let el = item?.unbind();
            if !is_literal(py, &el)? { return Ok(false); }
        }
        return Ok(true);
    }
    Ok(true)
}
