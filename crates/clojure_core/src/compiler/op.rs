//! Bytecode op codes. 15 variants — enumerated in §1 of the plan.

/// A single bytecode instruction.
///
/// Control-flow operands (`Jump`, `JumpIfFalsy`) are absolute indices into
/// the method's `code: Vec<Op>`. Local and var indices reference the frame's
/// local slots and the fn's `pool.vars` respectively.
#[derive(Debug, Clone)]
pub enum Op {
    // Stack
    PushConst(u16),     // push constants[ix] — nil is constants[0] by convention
    Pop,
    Dup,

    // Locals (slot index within current frame)
    LoadLocal(u16),
    StoreLocal(u16),
    ClearLocal(u16),    // locals[ix] = None — releases retained references
    LoadCapture(u16),   // read the fn's captures[ix]
    LoadSelf,           // push the currently-executing Fn (used for named-fn self-recursion)

    // Vars (pre-resolved at compile time into pool.vars)
    Deref(u16),         // pool.vars[ix].deref() → push
    LoadVar(u16),       // push pool.vars[ix] as a Var object

    // Control flow (absolute op indices)
    Jump(u32),
    JumpIfFalsy(u32),   // Clojure truthiness: only nil / False branch

    // Invocation
    Invoke(u8),         // pop N args, pop fn, push call result
    /// Fused Deref(var_ix) + Invoke(nargs): deref pool.vars[var_ix] and invoke
    /// the result with `nargs` args drained from the top of the value stack.
    /// Common case for calls to top-level (def'd) fns — saves one op + one
    /// stack round-trip vs the split form.
    ///
    /// The third field is an index into `FnPool.ic_slots`. The VM consults
    /// the slot to skip `ProtocolFn::resolve` on repeat calls against the
    /// same target type. Semantics are preserved — a miss falls through to
    /// the full `invoke_n_owned` path and (re-)installs the slot.
    InvokeVar(u16, u8, u16),
    /// Arity-specialized fused Deref+Invoke for zero args. Skips the arg
    /// `Vec` allocation entirely on IC hit. `(var_ix, ic_slot_ix)`.
    InvokeVar0(u16, u16),
    /// Arity-specialized fused Deref+Invoke for one arg. `(var_ix,
    /// ic_slot_ix)` — the single arg is the top of the value stack.
    InvokeVar1(u16, u16),
    /// Arity-specialized fused Deref+Invoke for two args. `(var_ix,
    /// ic_slot_ix)` — args are top two of the value stack (second then
    /// first).
    InvokeVar2(u16, u16),
    Return,             // pop, return from current frame

    // Python interop
    GetAttr(u16),       // pop obj, push obj.<constants[ix]>
    SetAttr(u16),       // pop value, pop obj; obj.<constants[ix]> = value; push nil
    CallMethod(u16, u8), // pop N args, pop obj; push obj.<constants[ix]>(*args)

    // Exceptions
    Throw,              // pop exception instance, raise as PyErr
    PushHandler(u32, u16), // install a try-handler: (target_pc, exc_slot). On unwind,
                           // the VM stashes the exception in locals[exc_slot],
                           // truncates the value stack to its pre-try depth, and
                           // resumes at target_pc.
    PopHandler,         // remove the topmost handler (normal-exit from try body).

    // letfn* mutable forward-reference cells. See compiler/letfn_cell.rs.
    LetfnCellInit(u16), // allocate a fresh LetfnCell; store at locals[ix].
    LetfnCellSet(u16),  // pop value; locals[ix].set(value). Pushes nothing.
    LetfnCellGet,       // pop cell from stack; push cell.get().
}
