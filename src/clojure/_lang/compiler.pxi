# Compiler — Lisp form → Python bytecode.
#
# We walk reader output and emit Python bytecode via the `bytecode` library
# (Victor Stinner's), then wrap the resulting code object as a real Python
# function. Clojure functions ARE Python functions — same __closure__,
# __code__, dis output, traceback frames, etc.
#
# The strategy is direct bytecode emission rather than Python-AST emission:
#  - At the bytecode level the expression/statement split disappears (every
#    op pushes a value), so multiline `if`/`let`/`fn` come out naturally
#    without ANF-style temporary locals polluting the frame.
#  - `recur` is a JUMP_ABSOLUTE to a label; no `while True` + sentinel dance.
#  - Locals clearing is exact — emit LOAD_CONST None / STORE_FAST at the
#    precise PC we want.
#  - Try/catch is just exception-table entries pointing at labels.
#
# Closures use real Python cells (`__closure__` tuple). Because Clojure
# `let`/`loop` bindings are immutable at the language level (no `set!` on
# locals), we never need to emit STORE_DEREF after the initial bind, so
# nothing the language disallows can mutate a captured value. The one
# corner — an inner fn that captures a `loop` binding which is later
# rebound by recur — is handled by allocating a fresh cell at the inner-fn
# creation site (see `_make_cell`).


# --- helpers ------------------------------------------------------------

_CellType = _pytypes.CellType  # types.CellType — 3.8+

def _make_arity_dispatcher(*arity_fns):
    """Wrap N single-arity functions in a dispatcher that selects based
    on argument count. Each fn's __code__.co_argcount + CO_VARARGS flag
    determines its arity range. At most one variadic arity is permitted
    (and any variadic arity matches >= its required-arg count, after
    fixed arities have a chance)."""
    fixed = []
    var = None
    for fn in arity_fns:
        argc = fn.__code__.co_argcount
        is_var = bool(fn.__code__.co_flags & 0x04)
        if is_var:
            if var is not None:
                raise SyntaxError(
                    "Can't have more than 1 variadic overload")
            var = (argc, fn)
        else:
            fixed.append((argc, fn))
    fixed.sort(key=lambda p: p[0])
    fn_name = arity_fns[0].__name__ if arity_fns else "__fn__"

    def dispatcher(*args):
        n = len(args)
        for argc, fn in fixed:
            if n == argc:
                return fn(*args)
        if var is not None and n >= var[0]:
            return var[1](*args)
        raise TypeError(
            "Wrong number of args (" + str(n) + ") passed to: " + fn_name)
    dispatcher.__name__ = fn_name
    return dispatcher


def _make_cell(value):
    """Return a fresh Python closure cell holding `value`. Used when an
    inner fn captures a `loop` binding so that each iteration's closure
    sees that iteration's value rather than sharing one mutating cell.

    The `(lambda: value).__closure__[0]` idiom is correct in pure Python
    but Cython optimizes the lambda's free-var reference into a constant
    inline, leaving __closure__ as None. types.CellType(value) is the
    explicit constructor and side-steps that."""
    return _CellType(value)


# --- per-function compiler state ----------------------------------------

class _FnContext:
    """Mutable state for compiling a single Python code object: argnames,
    the locals map, the cellvar/freevar lists, the instruction stream,
    label allocator, and the recur target stack."""

    def __init__(self, name="__anon__", argnames=None, parent=None,
                 varargs=False):
        self.name = name
        self.argnames = list(argnames) if argnames else []
        self.parent = parent
        self.varargs = varargs   # True iff the last argname is *rest
        # clj-name -> ('FAST'|'FAST_LOOP'|'CELL'|'FREE', python_slot_name)
        self.locals = {}
        self.cellvars = []   # cells declared in THIS frame
        self.freevars = []   # captured FROM parent
        self.instrs = []
        self._gensym_counter = 0
        self.recur_targets = []   # stack of (label, [local_names])
        for an in self.argnames:
            self.locals[an] = ('FAST', an)

    def gensym(self, base="t"):
        self._gensym_counter += 1
        return "__" + base + "_" + str(self._gensym_counter) + "__"

    def emit(self, *xs):
        self.instrs.extend(xs)

    def new_label(self):
        return _bc_Label()

    def to_code(self):
        bc = _bc_Bytecode()
        bc.name = self.name
        # CO_OPTIMIZED | CO_NEWLOCALS — standard for any user function;
        # `bytecode` doesn't infer them automatically.
        bc.flags = bc.flags | _bc_CompilerFlags.OPTIMIZED | _bc_CompilerFlags.NEWLOCALS
        # With *rest, the last argname is the rest param and is NOT
        # counted in argcount.
        if self.varargs:
            bc.argcount = len(self.argnames) - 1
            bc.flags = bc.flags | _bc_CompilerFlags.VARARGS
        else:
            bc.argcount = len(self.argnames)
        bc.argnames = list(self.argnames)
        bc.cellvars = list(self.cellvars)
        bc.freevars = list(self.freevars)
        if self.freevars:
            bc.append(_bc_Instr("COPY_FREE_VARS", len(self.freevars)))
        for cv in self.cellvars:
            bc.append(_bc_Instr("MAKE_CELL", _bc_CellVar(cv)))
        bc.append(_bc_Instr("RESUME", 0))
        bc.extend(self.instrs)
        return bc.to_code()


# --- form classification ------------------------------------------------

def _is_self_eval_literal(form):
    """True if `form` evaluates to itself with no further work — direct
    LOAD_CONST suffices."""
    if form is None or form is True or form is False:
        return True
    if isinstance(form, (int, float, str, Keyword, BigInt, BigDecimal, Ratio)):
        return True
    return False


# --- FAST → CELL promotion / capture chain ------------------------------

def _promote_to_cell(ctx, slot_name):
    """Convert a FAST local at python `slot_name` into a CELL. Idempotent
    for already-cells. Patches every already-emitted LOAD_FAST/STORE_FAST
    for this slot to LOAD_DEREF/STORE_DEREF, and adds the slot to
    `ctx.cellvars` so MAKE_CELL appears in the prologue at to_code time.

    Used when an inner fn discovers it needs to capture an outer let*
    binding or arg that we'd previously assumed was a plain local."""
    if slot_name in ctx.cellvars:
        return
    ctx.cellvars.append(slot_name)
    cv = _bc_CellVar(slot_name)
    patched = []
    for ins in ctx.instrs:
        nm = getattr(ins, "name", None)
        arg = getattr(ins, "arg", None)
        if nm == "LOAD_FAST" and arg == slot_name:
            patched.append(_bc_Instr("LOAD_DEREF", cv))
        elif nm == "STORE_FAST" and arg == slot_name:
            patched.append(_bc_Instr("STORE_DEREF", cv))
        else:
            patched.append(ins)
    ctx.instrs = patched
    for clj_name, (kind, sn) in list(ctx.locals.items()):
        if sn == slot_name and kind == "FAST":
            ctx.locals[clj_name] = ("CELL", slot_name)


def _resolve_local_through_chain(start_ctx, clj_name):
    """If `clj_name` is bound in `start_ctx` or any ancestor, ensure the
    cell/freevar plumbing exists between binding site and `start_ctx`,
    and return the python slot name in `start_ctx`. Returns None if not
    bound anywhere up the chain.

    FAST locals get promoted to CELL on capture (mutation-free, since
    Clojure forbids set! on locals). FAST_LOOP locals (loop* bindings)
    do NOT get promoted — instead, the closest enclosing fn boundary
    freshens a new cell at MAKE_FUNCTION time so each iteration's
    closure sees that iteration's value (the JS late-binding fix).

    Slot names propagate unchanged through a capture chain — every layer
    uses the same string. They live in disjoint frames so there's no
    clash."""
    if clj_name in start_ctx.locals:
        return start_ctx.locals[clj_name][1]
    intermediates = []
    bind_ctx = start_ctx.parent
    while bind_ctx is not None:
        if clj_name in bind_ctx.locals:
            kind, slot = bind_ctx.locals[clj_name]
            if kind == "FAST":
                _promote_to_cell(bind_ctx, slot)
            # FAST_LOOP stays FAST_LOOP — no promotion. The freshen
            # happens at the inner-fn closure-tuple build site (see
            # _compile_fn_star's freevar emit loop).
            for c in intermediates + [start_ctx]:
                if slot not in c.freevars:
                    c.freevars.append(slot)
                if clj_name not in c.locals:
                    c.locals[clj_name] = ("FREE", slot)
            return slot
        intermediates.append(bind_ctx)
        bind_ctx = bind_ctx.parent
    return None


def _slot_kind_in(ctx, slot_name):
    """Reverse-lookup: what kind is `slot_name` in `ctx`'s locals? Used
    by fn* when deciding how to push a captured cell into the closure
    tuple. Returns ('FAST'|'FAST_LOOP'|'CELL'|'FREE') or None."""
    for _, (kind, sn) in ctx.locals.items():
        if sn == slot_name:
            return kind
    return None


# --- symbol resolution --------------------------------------------------

def _resolve_in_current_ns(sym):
    """Look `sym` up in the current namespace's mappings. Returns a Var,
    a class, some other mapped value, or None."""
    return Compiler.maybe_resolve_in(Compiler.current_ns(), sym)


def _resolve_var_or_die(sym):
    """For special forms that need a Var specifically (`var`, later `def`
    in some shapes)."""
    v = _resolve_in_current_ns(sym)
    if isinstance(v, Var):
        return v
    raise NameError("Unable to resolve var: " + str(sym))


def _emit_symbol_value(sym, ctx):
    """Emit code that pushes the *value* of `sym` (a local — possibly
    captured from an outer fn — or a Var resolved through the current
    namespace and dereffed)."""
    # Lexical local first — walk the lexical chain. This may promote a
    # parent FAST local to a CELL and add freevar entries through any
    # intermediate fn contexts.
    if sym.ns is None:
        slot = _resolve_local_through_chain(ctx, sym.name)
        if slot is not None:
            kind = ctx.locals[sym.name][0]
            if kind == "FAST" or kind == "FAST_LOOP":
                ctx.emit(_bc_Instr("LOAD_FAST", slot))
                return
            if kind == "CELL":
                ctx.emit(_bc_Instr("LOAD_DEREF", _bc_CellVar(slot)))
                return
            if kind == "FREE":
                ctx.emit(_bc_Instr("LOAD_DEREF", _bc_FreeVar(slot)))
                return
            raise AssertionError("unknown local kind: " + repr(kind))

    v = _resolve_in_current_ns(sym)
    if v is None:
        raise NameError("Unable to resolve symbol: " + str(sym))
    if isinstance(v, Var):
        # LOAD_CONST <var>; LOAD_ATTR (method-form) deref; CALL 0
        ctx.emit(
            _bc_Instr("LOAD_CONST", v),
            _bc_Instr("LOAD_ATTR", (True, "deref")),
            _bc_Instr("CALL", 0),
        )
        return
    # Class or other mapped object — just load it.
    ctx.emit(_bc_Instr("LOAD_CONST", v))


# --- emitter ------------------------------------------------------------

def _compile_form(form, ctx):
    """Emit instructions that leave the value of `form` on the operand
    stack."""
    if _is_self_eval_literal(form):
        ctx.emit(_bc_Instr("LOAD_CONST", form))
        return

    if isinstance(form, Symbol):
        _emit_symbol_value(form, ctx)
        return

    # Lists / seqs are calls or special forms. An empty list evaluates to
    # itself.
    if isinstance(form, ISeq):
        s = form.seq()
        if s is None:
            ctx.emit(_bc_Instr("LOAD_CONST", form))
            return
        first = s.first()
        if isinstance(first, Symbol) and first.ns is None:
            sname = first.name
            if sname == "quote":
                rest = s.next()
                if rest is None:
                    raise SyntaxError("quote requires one argument")
                ctx.emit(_bc_Instr("LOAD_CONST", rest.first()))
                return
            if sname == "var":
                rest = s.next()
                if rest is None:
                    raise SyntaxError("var requires a symbol argument")
                target = rest.first()
                if not isinstance(target, Symbol):
                    raise SyntaxError("var requires a symbol argument")
                ctx.emit(_bc_Instr("LOAD_CONST", _resolve_var_or_die(target)))
                return
            if sname == "if":
                _compile_if(s.next(), ctx)
                return
            if sname == "do":
                _compile_do(s.next(), ctx)
                return
            if sname == "let*":
                _compile_let_star(s.next(), ctx)
                return
            if sname == "fn*":
                _compile_fn_star(s.next(), ctx)
                return
            if sname == "def":
                _compile_def(s.next(), ctx)
                return
            if sname == "loop*":
                _compile_loop_star(s.next(), ctx)
                return
            if sname == "recur":
                _compile_recur(s.next(), ctx)
                return
        # Function-call form: (f arg1 arg2 ...)
        _compile_call(s, ctx)
        return

    raise NotImplementedError(
        "compile: form not yet supported: " + repr(form))


# --- if / do / function-call -------------------------------------------

def _compile_if(args, ctx):
    """args is the seq AFTER the `if` head: (test then else?) — `else` is
    optional and defaults to nil."""
    if args is None:
        raise SyntaxError("if requires at least a test and a then branch")
    test = args.first()
    rest = args.next()
    if rest is None:
        raise SyntaxError("if requires a then branch")
    then_form = rest.first()
    else_rest = rest.next()
    has_else = else_rest is not None
    else_form = else_rest.first() if has_else else None
    if has_else and else_rest.next() is not None:
        raise SyntaxError("if takes at most 3 arguments (test then else)")

    else_label = ctx.new_label()
    end_label = ctx.new_label()

    # Wrap the test in RT.boolean_cast so Clojure's nil/false-only falsy
    # semantics drive the jump rather than Python's broader falsiness.
    ctx.emit(
        _bc_Instr("LOAD_CONST", RT.boolean_cast),
        _bc_Instr("PUSH_NULL"),
    )
    _compile_form(test, ctx)
    ctx.emit(
        _bc_Instr("CALL", 1),
        _bc_Instr("POP_JUMP_IF_FALSE", else_label),
    )
    _compile_form(then_form, ctx)
    ctx.emit(_bc_Instr("JUMP_FORWARD", end_label))
    ctx.emit(else_label)
    _compile_form(else_form, ctx)
    ctx.emit(end_label)


def _compile_do(args, ctx):
    """(do form1 form2 ... formN) — evaluate each in order, value of the
    last is the value of the do. Empty `(do)` is nil."""
    if args is None:
        ctx.emit(_bc_Instr("LOAD_CONST", None))
        return
    s = args
    while True:
        nxt = s.next()
        if nxt is None:
            # Last form — leave its value on the stack.
            _compile_form(s.first(), ctx)
            return
        # Intermediate form — evaluate for effect, discard value.
        _compile_form(s.first(), ctx)
        ctx.emit(_bc_Instr("POP_TOP"))
        s = nxt


def _compile_let_star(args, ctx):
    """(let* [name1 val1, name2 val2, ...] body...)

    Each binding is compiled in order and stored to a freshly gensym'd
    Python local; later bindings can reference earlier ones. The body is
    evaluated as an implicit `do`. On exit, the lexical scope is popped
    so the names are no longer visible to enclosing forms (the Python
    locals themselves remain in the frame — that's fine, they're just
    inaccessible from Clojure code)."""
    if args is None:
        raise SyntaxError("let* requires a bindings vector")
    bindings = args.first()
    body = args.next()
    if not isinstance(bindings, IPersistentVector):
        raise SyntaxError("let* requires a vector for its bindings")
    bcount = bindings.count()
    if bcount % 2 != 0:
        raise SyntaxError(
            "let* requires an even number of forms in the binding vector")

    saved_locals = dict(ctx.locals)
    try:
        for i in range(0, bcount, 2):
            name_sym = bindings.nth(i)
            value_form = bindings.nth(i + 1)
            if not isinstance(name_sym, Symbol) or name_sym.ns is not None:
                raise SyntaxError(
                    "let* binding names must be unqualified symbols")
            slot = ctx.gensym(name_sym.name)
            _compile_form(value_form, ctx)
            ctx.emit(_bc_Instr("STORE_FAST", slot))
            ctx.locals[name_sym.name] = ("FAST", slot)
        _compile_do(body, ctx)
    finally:
        ctx.locals = saved_locals


def _compile_loop_star(args, ctx):
    """(loop* [name1 init1 name2 init2 ...] body...)

    Like let* but also establishes a `recur` target. Bindings get the
    FAST_LOOP slot kind so any inner fn that captures one of them gets
    a freshly allocated cell at MAKE_FUNCTION time rather than sharing
    a mutating cell with subsequent iterations."""
    if args is None:
        raise SyntaxError("loop* requires a bindings vector")
    bindings = args.first()
    body = args.next()
    if not isinstance(bindings, IPersistentVector):
        raise SyntaxError("loop* requires a vector for its bindings")
    bcount = bindings.count()
    if bcount % 2 != 0:
        raise SyntaxError(
            "loop* requires an even number of forms in the binding vector")

    saved_locals = dict(ctx.locals)
    slot_names = []
    clj_names = []
    try:
        for i in range(0, bcount, 2):
            name_sym = bindings.nth(i)
            value_form = bindings.nth(i + 1)
            if not isinstance(name_sym, Symbol) or name_sym.ns is not None:
                raise SyntaxError(
                    "loop* binding names must be unqualified symbols")
            slot = ctx.gensym(name_sym.name)
            _compile_form(value_form, ctx)
            ctx.emit(_bc_Instr("STORE_FAST", slot))
            ctx.locals[name_sym.name] = ("FAST_LOOP", slot)
            slot_names.append(slot)
            clj_names.append(name_sym.name)

        loop_label = ctx.new_label()
        ctx.emit(loop_label)
        ctx.recur_targets.append((loop_label, slot_names, clj_names))
        try:
            _compile_do(body, ctx)
        finally:
            ctx.recur_targets.pop()
    finally:
        ctx.locals = saved_locals


def _compile_recur(args, ctx):
    """(recur expr...) — evaluate each expr, rebind the recur targets,
    and jump back. Tail-position only (we don't enforce that yet, but
    Python's stack tracker would catch many misuses)."""
    n = len(ctx.recur_targets)
    if n == 0:
        raise SyntaxError("recur outside of loop")
    # Note: explicit positive index — Cython compiler-directive
    # wraparound=False makes [-1] index literally -1 (out of range).
    target = ctx.recur_targets[n - 1]
    label = target[0]
    slot_names = target[1]

    # Count and validate
    nargs = 0
    cur = args
    while cur is not None:
        nargs += 1
        cur = cur.next()
    if nargs != len(slot_names):
        raise SyntaxError(
            "Mismatched argument count to recur, expected: "
            + str(len(slot_names)) + " args, got: " + str(nargs))

    # Evaluate all args first, leaving them on the stack — this preserves
    # the old binding values during evaluation (so e.g. (recur (inc i))
    # uses the OLD i rather than a partially-overwritten one).
    cur = args
    while cur is not None:
        _compile_form(cur.first(), ctx)
        cur = cur.next()
    # Pop them into the slots in reverse order (last value popped first
    # corresponds to the last slot).
    for slot in reversed(slot_names):
        ctx.emit(_bc_Instr("STORE_FAST", slot))
    ctx.emit(_bc_Instr("JUMP_BACKWARD", label))


def _parse_fn_args(args_vec):
    """Parse an arglist vector into (arg_names, has_rest)."""
    if not isinstance(args_vec, IPersistentVector):
        raise SyntaxError("fn* arglist must be a vector")
    arg_names = []
    has_rest = False
    av_count = args_vec.count()
    i = 0
    while i < av_count:
        an = args_vec.nth(i)
        if isinstance(an, Symbol) and an.ns is None and an.name == "&":
            if i + 1 >= av_count:
                raise SyntaxError("Missing rest arg after `&`")
            rest_sym = args_vec.nth(i + 1)
            if not isinstance(rest_sym, Symbol) or rest_sym.ns is not None:
                raise SyntaxError("rest arg must be an unqualified symbol")
            if i + 2 != av_count:
                raise SyntaxError("Only one symbol allowed after `&`")
            arg_names.append(rest_sym.name)
            has_rest = True
            break
        if not isinstance(an, Symbol) or an.ns is not None:
            raise SyntaxError("fn* arg names must be unqualified symbols")
        arg_names.append(an.name)
        i += 1
    return arg_names, has_rest


def _compile_one_arity(args_vec, body, ctx, fn_name, self_cell_slot):
    """Compile one arity into an inner _FnContext + code object. Caller
    is responsible for emitting MAKE_FUNCTION in `ctx` afterward (via
    _emit_make_function).

    `self_cell_slot` (if non-None) is the OUTER cellvar slot holding the
    fn-self reference for recursion; this arity's body will see `fn_name`
    as a freevar pointing at it.

    Fn args are marked FAST_LOOP and a recur target is pushed at the
    start of the body — that's how `(recur ...)` in fn-tail position
    rebinds the args and jumps back. The recur label sits AFTER the
    prologue (vararg seq-conversion etc.), so subsequent iterations
    skip that work."""
    arg_names, has_rest = _parse_fn_args(args_vec)
    inner = _FnContext(
        name=fn_name if fn_name else "__fn__",
        argnames=arg_names,
        parent=ctx,
        varargs=has_rest,
    )
    if has_rest:
        rest_slot = arg_names[len(arg_names) - 1]
        inner.emit(
            _bc_Instr("LOAD_CONST", RT.seq),
            _bc_Instr("PUSH_NULL"),
            _bc_Instr("LOAD_FAST", rest_slot),
            _bc_Instr("CALL", 1),
            _bc_Instr("STORE_FAST", rest_slot),
        )
    if self_cell_slot is not None and fn_name is not None:
        inner.freevars.append(self_cell_slot)
        inner.locals[fn_name] = ("FREE", self_cell_slot)
    # Mark args as FAST_LOOP so any recur in the body can rebind them,
    # and any inner fn capturing one freshens a per-creation cell rather
    # than promoting to a shared mutating cell.
    for an in arg_names:
        inner.locals[an] = ("FAST_LOOP", an)
    recur_label = inner.new_label()
    inner.emit(recur_label)
    inner.recur_targets.append(
        (recur_label, list(arg_names), list(arg_names)))
    try:
        _compile_do(body, inner)
    finally:
        inner.recur_targets.pop()
    inner.emit(_bc_Instr("RETURN_VALUE"))
    return inner, inner.to_code()


def _emit_make_function(ctx, inner, inner_code):
    """Emit the closure-tuple + MAKE_FUNCTION + SET_FUNCTION_ATTRIBUTE
    sequence in `ctx` for the inner code object. Leaves the function on
    the operand stack."""
    if inner.freevars:
        for fv in inner.freevars:
            if fv in ctx.cellvars:
                ctx.emit(_bc_Instr("LOAD_FAST", _bc_CellVar(fv)))
            elif fv in ctx.freevars:
                ctx.emit(_bc_Instr("LOAD_FAST", _bc_FreeVar(fv)))
            elif _slot_kind_in(ctx, fv) == "FAST_LOOP":
                ctx.emit(
                    _bc_Instr("LOAD_CONST", _make_cell),
                    _bc_Instr("PUSH_NULL"),
                    _bc_Instr("LOAD_FAST", fv),
                    _bc_Instr("CALL", 1),
                )
            else:
                raise AssertionError(
                    "fn* freevar not classifiable in outer: " + repr(fv))
        ctx.emit(_bc_Instr("BUILD_TUPLE", len(inner.freevars)))
        ctx.emit(_bc_Instr("LOAD_CONST", inner_code))
        ctx.emit(_bc_Instr("MAKE_FUNCTION"))
        ctx.emit(_bc_Instr("SET_FUNCTION_ATTRIBUTE", 8))
    else:
        ctx.emit(_bc_Instr("LOAD_CONST", inner_code))
        ctx.emit(_bc_Instr("MAKE_FUNCTION"))


def _compile_fn_star(args, ctx):
    """(fn* [args...] body...)
       (fn* name [args...] body...)
       (fn* ([args1...] body1) ([args2...] body2) ...)
       (fn* name ([args1...] body1) ([args2...] body2) ...)

    Single-arity yields one Python function. Multi-arity compiles each
    overload as its own inner function and wraps them in a runtime
    dispatcher (_make_arity_dispatcher) that selects on len(args).

    For named fn, a self-cell in OUTER lets the body recurse through the
    name; in multi-arity that cell ends up holding the dispatcher, so
    recursive calls go through dispatch."""
    if args is None:
        raise SyntaxError("fn* requires at least an arglist or arity list")
    first = args.first()
    rest = args.next()
    fn_name = None
    if isinstance(first, Symbol):
        fn_name = first.name
        if rest is None:
            raise SyntaxError("fn* with name requires at least one arity")
        first = rest.first()
        rest = rest.next()

    # Detect multi-arity: each remaining form is a list whose first
    # element is a vector. Single-arity: `first` itself is a vector.
    if isinstance(first, IPersistentVector):
        arities = [(first, rest)]
    elif isinstance(first, ISeq):
        arities = []
        cur = args
        if fn_name is not None:
            cur = args.next()
        while cur is not None:
            arity_form = cur.first()
            if not isinstance(arity_form, ISeq):
                raise SyntaxError(
                    "fn* multi-arity expects each overload as (args body...)")
            a_seq = arity_form.seq()
            if a_seq is None:
                raise SyntaxError("fn* arity overload requires an arglist")
            a_args = a_seq.first()
            a_body = a_seq.next()
            arities.append((a_args, a_body))
            cur = cur.next()
    else:
        raise SyntaxError("fn* expects a vector or arity list")

    self_cell_slot = None
    if fn_name is not None:
        self_cell_slot = ctx.gensym(fn_name)
        if self_cell_slot not in ctx.cellvars:
            ctx.cellvars.append(self_cell_slot)

    if len(arities) == 1:
        a_args, a_body = arities[0]
        inner, inner_code = _compile_one_arity(
            a_args, a_body, ctx, fn_name, self_cell_slot)
        _emit_make_function(ctx, inner, inner_code)
    else:
        # Validate at most one variadic arity, and only as the highest
        # arity-count overload. Build each then call the dispatcher.
        ctx.emit(
            _bc_Instr("LOAD_CONST", _make_arity_dispatcher),
            _bc_Instr("PUSH_NULL"),
        )
        for a_args, a_body in arities:
            inner, inner_code = _compile_one_arity(
                a_args, a_body, ctx, fn_name, self_cell_slot)
            _emit_make_function(ctx, inner, inner_code)
        ctx.emit(_bc_Instr("CALL", len(arities)))

    if self_cell_slot is not None:
        ctx.emit(
            _bc_Instr("COPY", 1),
            _bc_Instr("STORE_DEREF", _bc_CellVar(self_cell_slot)),
        )


def _compile_def(args, ctx):
    """(def name)              — intern an unbound Var
       (def name init)         — intern and set its root to init
       (def name docstring init) — same, with :doc metadata

    The Var is interned at compile time so subsequent forms (within
    the same compilation unit, or in other forms compiled afterwards)
    can resolve `name` even if `init` later refers back to the var.
    The init value is evaluated at run time and stored as the root."""
    if args is None:
        raise SyntaxError("def requires a name")
    name_sym = args.first()
    if not isinstance(name_sym, Symbol):
        raise SyntaxError("def's first argument must be a symbol")
    if name_sym.ns is not None:
        raise SyntaxError(
            "def's name must not be namespace-qualified: " + str(name_sym))

    rest = args.next()
    docstring = None
    init_form = None
    has_init = False
    if rest is not None:
        first_rest = rest.first()
        rest2 = rest.next()
        if rest2 is not None and isinstance(first_rest, str):
            docstring = first_rest
            init_form = rest2.first()
            has_init = True
            if rest2.next() is not None:
                raise SyntaxError("Too many arguments to def")
        else:
            init_form = first_rest
            has_init = True
            if rest2 is not None:
                raise SyntaxError("Too many arguments to def")

    ns = Compiler.current_ns()
    v = ns.intern(name_sym)
    if docstring is not None:
        v.set_meta(RT.assoc(v.meta() or PersistentArrayMap.EMPTY,
                            Keyword.intern(None, "doc"), docstring))

    if has_init:
        # Stack: leave Var on TOS at the end.
        # Emit: var.bind_root(<init>); LOAD_CONST var
        ctx.emit(
            _bc_Instr("LOAD_CONST", v),
            _bc_Instr("LOAD_ATTR", (True, "bind_root")),
        )
        _compile_form(init_form, ctx)
        ctx.emit(
            _bc_Instr("CALL", 1),
            _bc_Instr("POP_TOP"),
            _bc_Instr("LOAD_CONST", v),
        )
    else:
        ctx.emit(_bc_Instr("LOAD_CONST", v))


def _compile_call(s, ctx):
    """Plain function-call form: (callable arg1 arg2 ...)."""
    callable_form = s.first()
    args = s.next()
    # Stack layout for CALL n: [callable, NULL_or_self, arg1, ..., argn].
    _compile_form(callable_form, ctx)
    ctx.emit(_bc_Instr("PUSH_NULL"))
    nargs = 0
    cur = args
    while cur is not None:
        _compile_form(cur.first(), ctx)
        nargs += 1
        cur = cur.next()
    ctx.emit(_bc_Instr("CALL", nargs))


# --- entry points -------------------------------------------------------

def _compile_to_thunk(form):
    """Compile `form` to a 0-arg Python function returning its value."""
    ctx = _FnContext(name="__clj_eval__")
    _compile_form(form, ctx)
    ctx.emit(_bc_Instr("RETURN_VALUE"))
    code = ctx.to_code()
    return _pytypes.FunctionType(code, globals())


def _compiler_eval(form):
    return _compile_to_thunk(form)()


# Wire onto Compiler (defined in runtime_support.pxi).
Compiler.eval = staticmethod(_compiler_eval)
