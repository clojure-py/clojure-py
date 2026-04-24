//! Bytecode VM — stack-based dispatch loop.
//!
//! Per plan §5: a single `run` entry point executes one `CompiledMethod`
//! against its pool + captures + args. Invokes recurse through `rt::invoke_n`
//! (preserving the protocol-dispatch invariant). No implicit TCO; `recur`
//! compiles down to `StoreLocal`s + `Jump` so the VM has no Recur op at all.

use crate::compiler::method::CompiledMethod;
use crate::compiler::op::Op;
use crate::compiler::pool::FnPool;
use crate::eval::errors;
use pyo3::prelude::*;
use pyo3::types::PyAny;

type PyObject = Py<PyAny>;

/// A pending try-handler. On exception unwind, the VM pops the topmost handler,
/// truncates the value stack to `stack_depth`, stashes the caught exception in
/// `locals[exc_slot]`, and resumes execution at `target_pc`.
#[derive(Debug)]
struct Handler {
    target_pc: u32,
    exc_slot: u16,
    stack_depth: usize,
}

/// Outcome of executing a single VM op.
enum Step {
    Next,                 // continue to the next op
    Return(PyObject),     // Op::Return fired; unwind back to the caller
}

/// Execute a compiled method. Caller supplies:
/// * `method` — the arity-specific compiled code + frame layout.
/// * `pool`   — shared const/var pool (owned by the fn).
/// * `captures` — closure-captured values (`LoadCapture(ix)` indexes these).
/// * `args`   — evaluated arguments (`frame.locals[0..arity]` — the
///              compiler's slot allocator places params in slots `0..arity`).
pub fn run(
    py: Python<'_>,
    method: &CompiledMethod,
    pool: &FnPool,
    captures: &[PyObject],
    args: &[PyObject],
    self_fn: Option<&PyObject>,
) -> PyResult<PyObject> {
    let n_slots = method.local_slots as usize;
    if args.len() > n_slots {
        return Err(errors::err(format!(
            "compiled method frame too small: args={} slots={}",
            args.len(),
            n_slots
        )));
    }
    let mut locals: Vec<PyObject> = Vec::with_capacity(n_slots);
    for a in args.iter() { locals.push(a.clone_ref(py)); }
    while locals.len() < n_slots { locals.push(py.None()); }

    let mut stack: Vec<PyObject> = Vec::with_capacity(16);
    let mut pc: usize = 0;
    let mut handlers: Vec<Handler> = Vec::new();

    'vm: loop {
        // Bounds-check the op before anything else — an out-of-bounds pc is
        // an internal compiler error, not a user-catchable exception.
        let op = method.code.get(pc).ok_or_else(|| {
            errors::err(format!("pc {} out of bounds (code len {})", pc, method.code.len()))
        })?;

        // Execute one op. The closure captures `stack`, `locals`, `pc` by
        // mutable reference (all distinct locals, so the borrow checker is
        // happy). Errors raised inside the closure propagate out to the
        // outer loop, which then consults the handler stack.
        let step_result: PyResult<Step> = (|| -> PyResult<Step> {
            match op {
                Op::PushConst(ix) => {
                    let c = pool.constants.get(*ix as usize).ok_or_else(|| {
                        errors::err(format!("PushConst: invalid const index {}", ix))
                    })?;
                    stack.push(c.clone_ref(py));
                    pc += 1;
                }
                Op::Pop => {
                    stack.pop().ok_or_else(|| errors::err("Pop on empty stack"))?;
                    pc += 1;
                }
                Op::Dup => {
                    let top = stack.last().ok_or_else(|| errors::err("Dup on empty stack"))?;
                    stack.push(top.clone_ref(py));
                    pc += 1;
                }
                Op::LoadLocal(ix) => {
                    let v = locals.get(*ix as usize).ok_or_else(|| {
                        errors::err(format!("LoadLocal: invalid slot {}", ix))
                    })?;
                    stack.push(v.clone_ref(py));
                    pc += 1;
                }
                Op::StoreLocal(ix) => {
                    let v = stack.pop().ok_or_else(|| errors::err("StoreLocal on empty stack"))?;
                    let slot = locals.get_mut(*ix as usize).ok_or_else(|| {
                        errors::err(format!("StoreLocal: invalid slot {}", ix))
                    })?;
                    *slot = v;
                    pc += 1;
                }
                Op::ClearLocal(ix) => {
                    let slot = locals.get_mut(*ix as usize).ok_or_else(|| {
                        errors::err(format!("ClearLocal: invalid slot {}", ix))
                    })?;
                    *slot = py.None();
                    pc += 1;
                }
                Op::LoadSelf => {
                    let f = self_fn.ok_or_else(|| {
                        errors::err("LoadSelf: no enclosing fn (internal compiler error)")
                    })?;
                    stack.push(f.clone_ref(py));
                    pc += 1;
                }
                Op::LoadCapture(ix) => {
                    let v = captures.get(*ix as usize).ok_or_else(|| {
                        errors::err(format!("LoadCapture: invalid index {}", ix))
                    })?;
                    stack.push(v.clone_ref(py));
                    pc += 1;
                }
                Op::Deref(ix) => {
                    let var = pool.vars.get(*ix as usize).ok_or_else(|| {
                        errors::err(format!("Deref: invalid var index {}", ix))
                    })?;
                    let v = var.bind(py).call_method0("deref")?.unbind();
                    stack.push(v);
                    pc += 1;
                }
                Op::LoadVar(ix) => {
                    let var = pool.vars.get(*ix as usize).ok_or_else(|| {
                        errors::err(format!("LoadVar: invalid var index {}", ix))
                    })?;
                    stack.push(var.clone_ref(py).into_any());
                    pc += 1;
                }
                Op::Jump(target) => {
                    pc = *target as usize;
                }
                Op::JumpIfFalsy(target) => {
                    let v = stack.pop().ok_or_else(|| errors::err("JumpIfFalsy on empty stack"))?;
                    if is_falsy(py, &v) {
                        pc = *target as usize;
                    } else {
                        pc += 1;
                    }
                }
                Op::Invoke(nargs) => {
                    let n = *nargs as usize;
                    if stack.len() < n + 1 {
                        return Err(errors::err(format!(
                            "Invoke({}): stack has only {} values",
                            n, stack.len()
                        )));
                    }
                    let args_start = stack.len() - n;
                    let args: Vec<PyObject> = stack.drain(args_start..).collect();
                    let target = stack.pop().unwrap();
                    let result = crate::rt::invoke_n(py, target, &args)?;
                    stack.push(result);
                    pc += 1;
                }
                Op::Return => {
                    let v = stack.pop().ok_or_else(|| errors::err("Return on empty stack"))?;
                    return Ok(Step::Return(v));
                }
                Op::GetAttr(ix) => {
                    let name = const_as_str(py, pool, *ix as usize, "GetAttr")?;
                    let obj = stack.pop().ok_or_else(|| errors::err("GetAttr on empty stack"))?;
                    let v = obj.bind(py).getattr(name.as_str())?.unbind();
                    stack.push(v);
                    pc += 1;
                }
                Op::SetAttr(ix) => {
                    let name = const_as_str(py, pool, *ix as usize, "SetAttr")?;
                    let value = stack.pop().ok_or_else(|| errors::err("SetAttr: missing value"))?;
                    let obj = stack.pop().ok_or_else(|| errors::err("SetAttr: missing obj"))?;
                    obj.bind(py).setattr(name.as_str(), value)?;
                    stack.push(py.None());
                    pc += 1;
                }
                Op::CallMethod(ix, nargs) => {
                    let name = const_as_str(py, pool, *ix as usize, "CallMethod")?;
                    let n = *nargs as usize;
                    if stack.len() < n + 1 {
                        return Err(errors::err(format!(
                            "CallMethod({}, {}): stack has only {} values",
                            name, n, stack.len()
                        )));
                    }
                    let args_start = stack.len() - n;
                    let args: Vec<PyObject> = stack.drain(args_start..).collect();
                    let obj = stack.pop().unwrap();
                    let args_tup = pyo3::types::PyTuple::new(py, &args)?;
                    let result = obj.bind(py).call_method1(name.as_str(), args_tup)?.unbind();
                    stack.push(result);
                    pc += 1;
                }
                Op::Throw => {
                    let v = stack.pop().ok_or_else(|| errors::err("Throw on empty stack"))?;
                    return Err(raise_value(py, v));
                }
                Op::PushHandler(target, exc_slot) => {
                    handlers.push(Handler {
                        target_pc: *target,
                        exc_slot: *exc_slot,
                        stack_depth: stack.len(),
                    });
                    pc += 1;
                }
                Op::PopHandler => {
                    handlers.pop().ok_or_else(|| {
                        errors::err("PopHandler with empty handler stack")
                    })?;
                    pc += 1;
                }
                Op::LetfnCellInit(ix) => {
                    let cell = crate::compiler::letfn_cell::LetfnCell::new();
                    let cell_py: PyObject = Py::new(py, cell)?.into_any();
                    let slot = locals.get_mut(*ix as usize).ok_or_else(|| {
                        errors::err(format!("LetfnCellInit: invalid slot {}", ix))
                    })?;
                    *slot = cell_py;
                    pc += 1;
                }
                Op::LetfnCellSet(ix) => {
                    let v = stack.pop().ok_or_else(|| errors::err("LetfnCellSet on empty stack"))?;
                    let slot = locals.get(*ix as usize).ok_or_else(|| {
                        errors::err(format!("LetfnCellSet: invalid slot {}", ix))
                    })?;
                    let cell = slot.bind(py)
                        .downcast::<crate::compiler::letfn_cell::LetfnCell>()
                        .map_err(|_| errors::err("LetfnCellSet: slot does not hold a LetfnCell"))?;
                    cell.get().set(v);
                    pc += 1;
                }
                Op::LetfnCellGet => {
                    let cell_obj = stack.pop().ok_or_else(|| errors::err("LetfnCellGet on empty stack"))?;
                    let cell = cell_obj.bind(py)
                        .downcast::<crate::compiler::letfn_cell::LetfnCell>()
                        .map_err(|_| errors::err("LetfnCellGet: top of stack is not a LetfnCell"))?;
                    let v = cell.get().get(py)?;
                    stack.push(v);
                    pc += 1;
                }
            }
            Ok(Step::Next)
        })();

        match step_result {
            Ok(Step::Next) => continue 'vm,
            Ok(Step::Return(v)) => return Ok(v),
            Err(e) => {
                // Unwind to the nearest registered handler (if any). Each
                // handler consumes itself — re-entering the same try body
                // after a catch would re-trigger it otherwise.
                match handlers.pop() {
                    Some(h) => {
                        stack.truncate(h.stack_depth);
                        let exc_val: PyObject = e.into_value(py).into_any();
                        if let Some(slot) = locals.get_mut(h.exc_slot as usize) {
                            *slot = exc_val;
                        }
                        pc = h.target_pc as usize;
                        continue 'vm;
                    }
                    None => return Err(e),
                }
            }
        }
    }
}

/// Raise a Clojure-level `throw` value. Accepts either an exception instance
/// (preferred) or — for compatibility with forms that throw non-exception
/// values — wraps anything else into an `IllegalStateException` so the PyErr
/// path stays clean.
fn raise_value(py: Python<'_>, v: PyObject) -> PyErr {
    let b = v.bind(py);
    // If it's a class, instantiate it with no args first.
    if let Ok(exc_type) = b.cast::<pyo3::types::PyType>() {
        match exc_type.call0() {
            Ok(inst) => return PyErr::from_value(inst),
            Err(e) => return e,
        }
    }
    // If it's already an exception instance, raise as-is.
    if b.is_instance_of::<pyo3::exceptions::PyBaseException>() {
        return PyErr::from_value(b.clone());
    }
    // Fallback: wrap in IllegalStateException with a short message.
    let msg = match b.str() {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => format!("throw value of unprintable type: {:?}", b.get_type()),
    };
    crate::exceptions::IllegalStateException::new_err(msg)
}

#[inline]
fn is_falsy(py: Python<'_>, v: &PyObject) -> bool {
    if v.is_none(py) { return true; }
    if let Ok(b) = v.bind(py).cast::<pyo3::types::PyBool>() {
        return !b.is_true();
    }
    false
}

fn const_as_str(
    py: Python<'_>,
    pool: &FnPool,
    ix: usize,
    op_name: &'static str,
) -> PyResult<String> {
    let c = pool.constants.get(ix).ok_or_else(|| {
        errors::err(format!("{}: invalid const index {}", op_name, ix))
    })?;
    let s = c.bind(py).cast::<pyo3::types::PyString>().map_err(|_| {
        errors::err(format!("{}: constant at {} is not a string", op_name, ix))
    })?;
    Ok(s.to_str()?.to_string())
}
