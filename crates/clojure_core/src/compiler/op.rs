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

    // Vars (pre-resolved at compile time into pool.vars)
    Deref(u16),         // pool.vars[ix].deref() → push
    LoadVar(u16),       // push pool.vars[ix] as a Var object

    // Control flow (absolute op indices)
    Jump(u32),
    JumpIfFalsy(u32),   // Clojure truthiness: only nil / False branch

    // Invocation
    Invoke(u8),         // pop N args, pop fn, push call result
    Return,             // pop, return from current frame

    // Python interop
    GetAttr(u16),       // pop obj, push obj.<constants[ix]>
    SetAttr(u16),       // pop value, pop obj; obj.<constants[ix]> = value; push nil
    CallMethod(u16, u8), // pop N args, pop obj; push obj.<constants[ix]>(*args)
}
