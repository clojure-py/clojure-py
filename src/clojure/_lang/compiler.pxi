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

def _make_cell(value):
    """Return a fresh Python closure cell holding `value`. Used when an
    inner fn captures a `loop` binding so that each iteration's closure
    sees that iteration's value rather than sharing one mutating cell."""
    return (lambda: value).__closure__[0]


# --- per-function compiler state ----------------------------------------

class _FnContext:
    """Mutable state for compiling a single Python code object: argnames,
    the locals map, the cellvar/freevar lists, the instruction stream,
    label allocator, and the recur target stack."""

    def __init__(self, name="__anon__", argnames=None, parent=None):
        self.name = name
        self.argnames = list(argnames) if argnames else []
        self.parent = parent
        # clj-name -> ('FAST'|'CELL'|'FREE', python_slot_name)
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
    """Emit code that pushes the *value* of `sym` (a local, or a Var
    resolved through the current namespace and dereffed)."""
    # Lexical local first — only unqualified symbols can shadow.
    if sym.ns is None and sym.name in ctx.locals:
        kind, slot = ctx.locals[sym.name]
        if kind == "FAST":
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
