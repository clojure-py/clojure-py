//! `clojure.lang.RT` namespace — low-level primitive shims.
//!
//! Mirrors the JVM Clojure arrangement: `clojure.core` is the user-facing
//! Clojure-defined API, built atop tiny primitives interned in
//! `clojure.lang.RT`. The source port of `core.clj` calls `clojure.lang.RT/x`
//! exactly where vanilla Clojure calls `(. clojure.lang.RT (x ...))`.
//!
//! Everything here is a thin wrapper over an `rt::*` helper, a protocol
//! check, or a Python built-in. No user-facing policy lives in this file.

use crate::exceptions::{IllegalArgumentException, IllegalStateException};
use crate::keyword::Keyword;
use crate::symbol::Symbol;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyCFunction, PyDict, PyModule, PyTuple, PyType};
use std::sync::Arc;

type PyObject = Py<PyAny>;

static FRACTION_CLS: once_cell::sync::OnceCell<Py<PyType>> = once_cell::sync::OnceCell::new();

fn fraction_cls<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyType>> {
    if let Some(cls) = FRACTION_CLS.get() {
        return Ok(cls.bind(py).clone());
    }
    let fractions = py.import("fractions")?;
    let cls = fractions.getattr("Fraction")?.downcast_into::<PyType>()?;
    let _ = FRACTION_CLS.set(cls.clone().unbind());
    Ok(cls)
}

static DECIMAL_CLS: once_cell::sync::OnceCell<Py<PyType>> = once_cell::sync::OnceCell::new();

fn decimal_cls<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyType>> {
    if let Some(cls) = DECIMAL_CLS.get() {
        return Ok(cls.bind(py).clone());
    }
    let decimal = py.import("decimal")?;
    let cls = decimal.getattr("Decimal")?.downcast_into::<PyType>()?;
    let _ = DECIMAL_CLS.set(cls.clone().unbind());
    Ok(cls)
}

fn make_closure(
    py: Python<'_>,
    f: impl Fn(&Bound<'_, PyTuple>, Python<'_>) -> PyResult<PyObject> + Send + Sync + 'static,
) -> PyResult<PyObject> {
    let wrapper = PyCFunction::new_closure(
        py,
        None,
        None,
        move |args: &Bound<'_, PyTuple>, _kw: Option<&Bound<'_, PyDict>>| -> PyResult<PyObject> {
            let py = args.py();
            f(args, py)
        },
    )?;
    Ok(wrapper.unbind().into_any())
}

fn intern_fn(
    py: Python<'_>,
    ns: &PyObject,
    name: &str,
    f: impl Fn(&Bound<'_, PyTuple>, Python<'_>) -> PyResult<PyObject> + Send + Sync + 'static,
) -> PyResult<()> {
    let callable = make_closure(py, f)?;
    let sym = Py::new(py, Symbol::new(None, Arc::from(name)))?;
    let var = crate::ns_ops::intern(py, ns.clone_ref(py), sym)?;
    var.bind(py).call_method1("bind_root", (callable,))?;
    Ok(())
}

fn true_py(py: Python<'_>) -> PyObject {
    PyBool::new(py, true).to_owned().unbind().into_any()
}

fn false_py(py: Python<'_>) -> PyObject {
    PyBool::new(py, false).to_owned().unbind().into_any()
}

fn need_args(args: &Bound<'_, PyTuple>, n: usize, name: &str) -> PyResult<()> {
    if args.len() != n {
        return Err(IllegalArgumentException::new_err(format!(
            "{} requires {} arg(s), got {}",
            name,
            n,
            args.len()
        )));
    }
    Ok(())
}

/// Walk an ISeq-compatible value into a Rust Vec. `nil` → empty Vec.
fn seq_to_vec(py: Python<'_>, coll: PyObject) -> PyResult<Vec<PyObject>> {
    let mut out = Vec::new();
    let mut cur = crate::rt::seq(py, coll)?;
    while !cur.is_none(py) {
        out.push(crate::rt::first(py, cur.clone_ref(py))?);
        cur = crate::rt::next_(py, cur)?;
    }
    Ok(out)
}

/// Build an `isinstance(x, cls)` check closure.
fn mk_instance_pred(
    py: Python<'_>,
    ns: &PyObject,
    name: &str,
    cls: PyObject,
) -> PyResult<()> {
    let err_name: Arc<str> = Arc::from(name);
    intern_fn(py, ns, name, move |args, py| {
        if args.len() != 1 {
            return Err(IllegalArgumentException::new_err(format!(
                "{} requires 1 arg, got {}", err_name, args.len()
            )));
        }
        let x = args.get_item(0)?;
        let cls_b = cls.bind(py);
        let cls_ty = cls_b.cast::<PyType>().map_err(|_| {
            IllegalStateException::new_err("instance-*? class is not a type")
        })?;
        Ok(PyBool::new(py, x.is_instance(cls_ty)?).to_owned().unbind().into_any())
    })
}

/// Build a `protocol_is_implemented(type(x))` check closure.
/// Uses the protocol's method cache: if any method is registered for `type(x)`
/// or an ancestor, the protocol is considered implemented.
fn mk_protocol_pred(
    py: Python<'_>,
    ns: &PyObject,
    name: &str,
    proto: Py<crate::Protocol>,
) -> PyResult<()> {
    let err_name: Arc<str> = Arc::from(name);
    intern_fn(py, ns, name, move |args, py| {
        if args.len() != 1 {
            return Err(IllegalArgumentException::new_err(format!(
                "{} requires 1 arg, got {}", err_name, args.len()
            )));
        }
        let x = args.get_item(0)?;
        let ty = x.get_type();
        let proto_ref = proto.bind(py).get();
        let exact_key = crate::protocol::CacheKey::for_py_type(&ty);
        if proto_ref.cache.lookup(exact_key).is_some() {
            return Ok(true_py(py));
        }
        let mro = ty.getattr("__mro__")?;
        let mro_tuple: Bound<'_, PyTuple> = mro.cast_into()?;
        for parent in mro_tuple.iter().skip(1) {
            let parent_ty: Bound<'_, PyType> = parent.cast_into()?;
            let pk = crate::protocol::CacheKey::for_py_type(&parent_ty);
            if proto_ref.cache.lookup(pk).is_some() {
                return Ok(true_py(py));
            }
        }
        Ok(false_py(py))
    })
}

fn get_proto(m: &Bound<'_, PyModule>, name: &str) -> PyResult<Py<crate::Protocol>> {
    Ok(m.getattr(name)?.cast::<crate::Protocol>()?.clone().unbind())
}

pub(crate) fn init(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let rt_sym = Py::new(py, Symbol::new(None, Arc::from("clojure.lang.RT")))?;
    let rt_ns = crate::namespace::create_ns(py, rt_sym)?;

    // Populate the `clojure.lang` placeholder module with exception classes
    // so `(catch clojure.lang.IllegalArgumentException e ...)` resolves via
    // the normal qualified-symbol path.
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if let Ok(clj_lang) = modules.get_item("clojure.lang") {
        clj_lang.setattr(
            "IllegalArgumentException",
            py.get_type::<crate::exceptions::IllegalArgumentException>(),
        )?;
        clj_lang.setattr(
            "IllegalStateException",
            py.get_type::<crate::exceptions::IllegalStateException>(),
        )?;
        clj_lang.setattr(
            "ArityException",
            py.get_type::<crate::exceptions::ArityException>(),
        )?;
        clj_lang.setattr(
            "EvalError",
            py.get_type::<crate::eval::errors::EvalError>(),
        )?;
    }

    // --- Seq primitives ---

    intern_fn(py, &rt_ns, "cons", |args, py| {
        need_args(args, 2, "cons")?;
        crate::rt::cons(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "first", |args, py| {
        need_args(args, 1, "first")?;
        crate::rt::first(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "next", |args, py| {
        need_args(args, 1, "next")?;
        crate::rt::next_(py, args.get_item(0)?.unbind())
    })?;

    // `more` is Clojure's RT/more — what `rest` calls through to on JVM.
    intern_fn(py, &rt_ns, "more", |args, py| {
        need_args(args, 1, "more")?;
        crate::rt::rest(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "seq", |args, py| {
        need_args(args, 1, "seq")?;
        crate::rt::seq(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "count", |args, py| {
        need_args(args, 1, "count")?;
        let n = crate::rt::count(py, args.get_item(0)?.unbind())?;
        Ok((n as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "conj", |args, py| {
        need_args(args, 2, "conj")?;
        crate::rt::conj(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "assoc", |args, py| {
        need_args(args, 3, "assoc")?;
        crate::rt::assoc(
            py,
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
            args.get_item(2)?.unbind(),
        )
    })?;

    // --- Metadata ---

    intern_fn(py, &rt_ns, "meta", |args, py| {
        need_args(args, 1, "meta")?;
        crate::rt::meta(py, args.get_item(0)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "with-meta", |args, py| {
        need_args(args, 2, "with-meta")?;
        crate::rt::with_meta(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind())
    })?;

    // --- Equality / identity ---

    intern_fn(py, &rt_ns, "equiv", |args, py| {
        need_args(args, 2, "equiv")?;
        let r = crate::rt::equiv(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind())?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // Vanilla `==` (RT.numEquiv → Numbers.equiv): numeric equality that
    // crosses int/float categories — `(== 1 1.0)` is true, unlike `=`.
    // For Ratio (fractions.Fraction) vs float, we convert the ratio to float
    // first, mirroring JVM Numbers.equiv(Ratio, Double).
    intern_fn(py, &rt_ns, "num-equiv", |args, py| {
        use pyo3::types::PyFloat;
        need_args(args, 2, "num-equiv")?;
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        let a_is_float = a.is_instance_of::<PyFloat>();
        let b_is_float = b.is_instance_of::<PyFloat>();
        // Vanilla `Numbers.equiv` widens cross-category numeric pairs through
        // BigDecimal. We approximate by promoting Ratio/Decimal operands to
        // f64 when the other side is a float, matching JVM `doubleValue`
        // semantics for in-range values. (== 0.1 (decimal "0.1")) is true.
        let r = if a_is_float || b_is_float {
            let frac_cls = fraction_cls(py)?;
            let dec_cls = decimal_cls(py)?;
            let a_promotes = !a_is_float && (a.is_instance(&frac_cls)? || a.is_instance(&dec_cls)?);
            let b_promotes = !b_is_float && (b.is_instance(&frac_cls)? || b.is_instance(&dec_cls)?);
            if a_promotes || b_promotes {
                let a_f: f64 = a.extract()?;
                let b_f: f64 = b.extract()?;
                a_f == b_f
            } else {
                a.eq(&b)?
            }
        } else {
            a.eq(&b)?
        };
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // Java-style .equals: treat as equiv for our purposes.
    intern_fn(py, &rt_ns, "equals", |args, py| {
        need_args(args, 2, "equals")?;
        let r = crate::rt::equiv(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind())?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // 32-bit two's-complement wrapping arithmetic, mirroring vanilla
    // `unchecked-*-int`. Required for hash-mixing code that builds an
    // accumulator over many elements; otherwise Python's arbitrary-precision
    // ints overflow C-long bounds inside protocol thunks.
    fn extract_i32(b: Bound<'_, PyAny>) -> PyResult<i32> {
        if let Ok(v) = b.extract::<i32>() { return Ok(v); }
        if let Ok(v) = b.extract::<i64>() { return Ok(v as i32); }
        let v: u32 = b.extract()?;
        Ok(v as i32)
    }
    intern_fn(py, &rt_ns, "unchecked-add-int", |args, py| {
        need_args(args, 2, "unchecked-add-int")?;
        let a = extract_i32(args.get_item(0)?)?;
        let b = extract_i32(args.get_item(1)?)?;
        let r = a.wrapping_add(b) as i64;
        Ok(r.into_pyobject(py)?.unbind().into_any())
    })?;
    intern_fn(py, &rt_ns, "unchecked-multiply-int", |args, py| {
        need_args(args, 2, "unchecked-multiply-int")?;
        let a = extract_i32(args.get_item(0)?)?;
        let b = extract_i32(args.get_item(1)?)?;
        let r = a.wrapping_mul(b) as i64;
        Ok(r.into_pyobject(py)?.unbind().into_any())
    })?;
    intern_fn(py, &rt_ns, "unchecked-subtract-int", |args, py| {
        need_args(args, 2, "unchecked-subtract-int")?;
        let a = extract_i32(args.get_item(0)?)?;
        let b = extract_i32(args.get_item(1)?)?;
        let r = a.wrapping_sub(b) as i64;
        Ok(r.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "identical?", |args, py| {
        need_args(args, 2, "identical?")?;
        let r = crate::rt::identical(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind());
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // --- Instance predicates (one per protocol / class we care about in Phase A) ---

    mk_protocol_pred(py, &rt_ns, "instance-seq?", get_proto(m, "ISeq")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-map?", get_proto(m, "IPersistentMap")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-vector?", get_proto(m, "IPersistentVector")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-set?", get_proto(m, "IPersistentSet")?)?;

    // (instance-number? x) — int/float/complex. Excludes bool (Python's
    // bool is an int subclass, but Clojure's number? returns false for it).
    intern_fn(py, &rt_ns, "instance-number?", |args, py| {
        need_args(args, 1, "instance-number?")?;
        let x = args.get_item(0)?;
        use pyo3::types::{PyInt, PyFloat, PyComplex};
        if x.cast::<PyBool>().is_ok() {
            return Ok(PyBool::new(py, false).to_owned().unbind().into_any());
        }
        let ok = x.cast::<PyInt>().is_ok() || x.cast::<PyFloat>().is_ok() || x.cast::<PyComplex>().is_ok();
        Ok(PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    // (instance-var? x)
    {
        let var_cls = py.get_type::<crate::var::Var>().unbind().into_any();
        mk_instance_pred(py, &rt_ns, "instance-var?", var_cls)?;
    }

    // (time-ns) — monotonic nanosecond clock, for the `time` macro.
    intern_fn(py, &rt_ns, "time-ns", |args, py| {
        need_args(args, 0, "time-ns")?;
        let _ = args;
        let t = py.import("time")?;
        Ok(t.call_method0("perf_counter_ns")?.unbind())
    })?;

    // (getattr obj name default) — mirrors Python's builtin for
    // namespace/object attribute lookup from Clojure.
    intern_fn(py, &rt_ns, "getattr", |args, py| {
        let n = args.len();
        if n != 2 && n != 3 {
            return Err(IllegalArgumentException::new_err("getattr: 2 or 3 args"));
        }
        let obj = args.get_item(0)?;
        let name = args.get_item(1)?.extract::<String>()?;
        match obj.getattr(name.as_str()) {
            Ok(v) => Ok(v.unbind()),
            Err(e) => {
                if n == 3 {
                    Ok(args.get_item(2)?.unbind())
                } else {
                    Err(e)
                }
            }
        }
    })?;
    mk_protocol_pred(py, &rt_ns, "instance-sequential?", get_proto(m, "Sequential")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-ifn?", get_proto(m, "IFn")?)?;
    // `fn?` differs from `ifn?`: only our Fn pyclass (actual Clojure functions
    // created by fn/defn) counts — keywords, maps, sets, and Vars all
    // implement IFn too but aren't "functions" in the trampoline/apply sense.
    let fn_cls: PyObject = m.getattr("Fn")?.unbind();
    mk_instance_pred(py, &rt_ns, "instance-fn?", fn_cls)?;
    mk_protocol_pred(py, &rt_ns, "instance-imeta?", get_proto(m, "IMeta")?)?;
    // IObj on the JVM is "IMeta that can .withMeta"; we model it as IMeta.
    mk_protocol_pred(py, &rt_ns, "instance-iobj?", get_proto(m, "IMeta")?)?;

    let sym_cls: PyObject = m.getattr("Symbol")?.unbind();
    mk_instance_pred(py, &rt_ns, "instance-symbol?", sym_cls)?;
    let kw_cls: PyObject = m.getattr("Keyword")?.unbind();
    mk_instance_pred(py, &rt_ns, "instance-keyword?", kw_cls)?;

    intern_fn(py, &rt_ns, "instance-string?", |args, py| {
        need_args(args, 1, "instance-string?")?;
        let x = args.get_item(0)?;
        Ok(PyBool::new(py, x.cast::<pyo3::types::PyString>().is_ok())
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "instance-bool?", |args, py| {
        if args.len() != 1 {
            return Err(IllegalArgumentException::new_err("instance-bool? takes 1 arg"));
        }
        let x = args.get_item(0)?;
        Ok(PyBool::new(py, x.cast::<PyBool>().is_ok())
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "instance-char?", |args, py| {
        need_args(args, 1, "instance-char?")?;
        let x = args.get_item(0)?;
        let ok = x.cast::<crate::char::Char>().is_ok();
        Ok(PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    // (char x) coerces x to a Char. Accepts Char, int (codepoint), 1-char str,
    // or float (truncated to int, matching JVM `(char 3.7) → \space-equivalent
    // codepoint 3` behavior).
    intern_fn(py, &rt_ns, "to-char", |args, py| {
        need_args(args, 1, "char")?;
        let x = args.get_item(0)?;
        if let Ok(c) = x.cast::<crate::char::Char>() {
            return Ok(Py::new(py, crate::char::Char::new(c.get().value))?.into_any());
        }
        if let Ok(s) = x.cast::<pyo3::types::PyString>() {
            let st = s.to_str()?;
            let mut iter = st.chars();
            return match (iter.next(), iter.next()) {
                (Some(c), None) => Ok(Py::new(py, crate::char::Char::new(c))?.into_any()),
                _ => Err(crate::exceptions::IllegalArgumentException::new_err(
                    "char: requires a Char, int codepoint, or 1-char str",
                )),
            };
        }
        // Numeric coercion: int directly, float truncated. Validate codepoint range.
        let n: i64 = if let Ok(v) = x.extract::<i64>() {
            v
        } else if let Ok(f) = x.extract::<f64>() {
            f as i64
        } else {
            return Err(crate::exceptions::IllegalArgumentException::new_err(
                "char: requires a Char, number, or 1-char str",
            ));
        };
        if !(0..=0x10FFFF).contains(&n) {
            return Err(crate::exceptions::IllegalArgumentException::new_err(
                format!("char: invalid Unicode codepoint: {}", n),
            ));
        }
        char::from_u32(n as u32)
            .ok_or_else(|| crate::exceptions::IllegalArgumentException::new_err(
                format!("char: invalid Unicode codepoint: {}", n),
            ))
            .and_then(|c| Ok(Py::new(py, crate::char::Char::new(c))?.into_any()))
    })?;

    // --- Exception raisers (core.clj uses these where JVM would `(Foo. "msg")`) ---

    intern_fn(py, &rt_ns, "throw-iae", |args, _py| {
        let msg = if args.len() == 0 {
            String::from("")
        } else {
            args.get_item(0)?.str()?.to_string_lossy().into_owned()
        };
        Err(IllegalArgumentException::new_err(msg))
    })?;

    intern_fn(py, &rt_ns, "throw-ise", |args, _py| {
        let msg = if args.len() == 0 {
            String::from("")
        } else {
            args.get_item(0)?.str()?.to_string_lossy().into_owned()
        };
        Err(IllegalStateException::new_err(msg))
    })?;

    // --- Var / macro support ---

    intern_fn(py, &rt_ns, "set-macro", |args, py| {
        need_args(args, 1, "set-macro")?;
        let var_any = args.get_item(0)?;
        let var = var_any.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("set-macro: arg must be a Var")
        })?;
        var.get().set_macro_flag(py)?;
        Ok(var.clone().unbind().into_any())
    })?;

    // --- String / formatting ---

    intern_fn(py, &rt_ns, "str-concat", |args, py| {
        let mut out = String::new();
        for i in 0..args.len() {
            let a = args.get_item(i)?;
            if a.is_none() { continue; }
            let s = a.str()?;
            out.push_str(s.to_str()?);
        }
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    // Vanilla `str` variadic uses StringBuilder over the argument seq —
    // O(N) total char copies and one bulk Python string allocation. We
    // mirror that here: walk the input seq once, accumulate into a Rust
    // `String` (which doubles geometrically), and emit one PyString. This
    // replaces an O(N²) loop/recur of `str-concat` calls in core.clj's
    // `(defn str ...)` variadic branch — the loop pressure is what
    // overflows Windows's smaller stack on `(apply str (repeat 10000 ...))`.
    intern_fn(py, &rt_ns, "strs-concat-impl", |args, py| {
        need_args(args, 1, "strs-concat")?;
        let coll = args.get_item(0)?.unbind();
        let mut out = String::new();
        let mut cur = crate::rt::seq(py, coll)?;
        while !cur.is_none(py) {
            let head = crate::rt::first(py, cur.clone_ref(py))?;
            if !head.is_none(py) {
                let s = head.bind(py).str()?;
                out.push_str(s.to_str()?);
            }
            cur = crate::rt::next_(py, cur)?;
        }
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "to-string", |args, py| {
        need_args(args, 1, "to-string")?;
        let a = args.get_item(0)?;
        if a.is_none() {
            return Ok(pyo3::types::PyString::new(py, "").unbind().into_any());
        }
        Ok(a.str()?.unbind().into_any())
    })?;

    // --- Symbol / keyword constructors ---

    intern_fn(py, &rt_ns, "symbol", |args, py| {
        let (ns_opt, name) = match args.len() {
            1 => (None, args.get_item(0)?.extract::<String>()?),
            2 => {
                let n0 = args.get_item(0)?;
                let ns_s = if n0.is_none() { None } else { Some(n0.extract::<String>()?) };
                (ns_s, args.get_item(1)?.extract::<String>()?)
            }
            _ => return Err(IllegalArgumentException::new_err("symbol: 1 or 2 args")),
        };
        let ns_arc = ns_opt.map(|s| Arc::<str>::from(s.as_str()));
        let sym = Symbol::new(ns_arc, Arc::<str>::from(name.as_str()));
        Ok(Py::new(py, sym)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "keyword", |args, py| {
        let (ns_opt, name) = match args.len() {
            1 => {
                let a0 = args.get_item(0)?;
                if let Ok(kw) = a0.cast::<Keyword>() {
                    return Ok(kw.clone().unbind().into_any());
                }
                (None, a0.extract::<String>()?)
            }
            2 => {
                let n0 = args.get_item(0)?;
                let ns_s = if n0.is_none() { None } else { Some(n0.extract::<String>()?) };
                (ns_s, args.get_item(1)?.extract::<String>()?)
            }
            _ => return Err(IllegalArgumentException::new_err("keyword: 1 or 2 args")),
        };
        // `keyword::keyword` takes `(ns_or_name, Option<name>)`. When both ns
        // and name are present, ns is the first arg; when only name is
        // present, that's the bare name.
        let kw = match ns_opt {
            Some(ns) => crate::keyword::keyword(py, ns.as_str(), Some(name.as_str()))?,
            None => crate::keyword::keyword(py, name.as_str(), None)?,
        };
        Ok(kw.into_any())
    })?;

    // --- Collection constructors (used by the `list` def in core.clj) ---

    // (list-from-seq xs) — return a PersistentList containing the items of xs
    // in order. Accepts nil, seqs, vectors, or any ISeqable.
    intern_fn(py, &rt_ns, "list-from-seq", |args, py| {
        need_args(args, 1, "list-from-seq")?;
        let xs = args.get_item(0)?.unbind();
        let s = crate::rt::seq(py, xs)?;
        let mut items: Vec<PyObject> = Vec::new();
        let mut cur = s;
        while !cur.is_none(py) {
            let f = crate::rt::first(py, cur.clone_ref(py))?;
            items.push(f);
            cur = crate::rt::next_(py, cur)?;
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // (apply f args) / (apply f x1 x2 ... args) — invoke f with x1..xn and the
    // seq-expanded tail as positional args. Mirrors clojure.core/apply.
    //
    // When `f` is a compiled Fn, we route through `Fn::apply_with_self_seq`
    // which peels only enough elements off the tail to satisfy the matching
    // arity and hands the *unrealized remainder* to the variadic rest slot —
    // matching vanilla's `applyTo(ISeq)`. Skipping full realization on a
    // 10000-element lazy seq matters on smaller-stack platforms (Windows).
    intern_fn(py, &rt_ns, "apply", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "apply requires at least a fn and a seq",
            ));
        }
        let f = args.get_item(0)?.unbind();
        let last_ix = args.len() - 1;

        // Fast path: target is a compiled Fn. Build a (list* leading tail)
        // seq and hand the whole thing to apply_with_self_seq. Leading args
        // are typically 0-3 elements; this is O(leading.len()) cons cells.
        if let Ok(fn_target) = f.bind(py).cast::<crate::eval::fn_value::Fn>() {
            let tail = args.get_item(last_ix)?.unbind();
            let mut combined: PyObject = tail;
            for i in (1..last_ix).rev() {
                let head = args.get_item(i)?.unbind();
                let cons = crate::seqs::cons::Cons::new(head, combined);
                combined = Py::new(py, cons)?.into_any();
            }
            return crate::eval::fn_value::Fn::apply_with_self_seq(
                fn_target.clone().unbind(),
                py,
                combined,
            );
        }

        // Generic path: realize the tail seq and call via invoke_n. Used for
        // Python callables, multimethods, vars wrapping non-Fn targets, etc.
        let mut call_args: Vec<PyObject> = Vec::with_capacity(last_ix);
        for i in 1..last_ix {
            call_args.push(args.get_item(i)?.unbind());
        }
        let tail = args.get_item(last_ix)?.unbind();
        let mut cur = crate::rt::seq(py, tail)?;
        while !cur.is_none(py) {
            call_args.push(crate::rt::first(py, cur.clone_ref(py))?);
            cur = crate::rt::next_(py, cur)?;
        }
        crate::rt::invoke_n(py, f, &call_args)
    })?;

    // (concat & seqs) — eager concatenation producing a PersistentList.
    // Used heavily by syntax-quote expansions.
    intern_fn(py, &rt_ns, "concat", |args, py| {
        let mut items: Vec<PyObject> = Vec::new();
        for i in 0..args.len() {
            let s = args.get_item(i)?.unbind();
            let mut cur = crate::rt::seq(py, s)?;
            while !cur.is_none(py) {
                items.push(crate::rt::first(py, cur.clone_ref(py))?);
                cur = crate::rt::next_(py, cur)?;
            }
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // (vector & items)
    intern_fn(py, &rt_ns, "vector", |args, py| {
        let v = crate::collections::pvector::vector(py, args.clone())?;
        Ok(v.into_any())
    })?;

    // (hash-map & kvs) — IAE on odd arity
    intern_fn(py, &rt_ns, "hash-map", |args, py| {
        let m = crate::collections::phashmap::hash_map(py, args.clone())?;
        Ok(m.into_any())
    })?;

    // (hash-set & items)
    intern_fn(py, &rt_ns, "hash-set", |args, py| {
        let s = crate::collections::phashset::hash_set(py, args.clone())?;
        Ok(s.into_any())
    })?;

    // --- Identity / counters ---

    intern_fn(py, &rt_ns, "next-id", |args, py| {
        if args.len() != 0 {
            return Err(IllegalArgumentException::new_err(
                "next-id takes no arguments",
            ));
        }
        let id = crate::rt::next_id();
        Ok((id as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    // (find-keyword name) / (find-keyword ns name) — lookup without interning.
    intern_fn(py, &rt_ns, "find-keyword", |args, py| {
        let (ns_opt, name) = match args.len() {
            1 => {
                let a0 = args.get_item(0)?;
                // If called on an existing Keyword, return it as-is.
                if let Ok(kw) = a0.cast::<Keyword>() {
                    return Ok(kw.clone().unbind().into_any());
                }
                let s = a0.extract::<String>()?;
                if let Some((n, nm)) = s.split_once('/') {
                    if !n.is_empty() && !nm.is_empty() {
                        (Some(n.to_owned()), nm.to_owned())
                    } else {
                        (None, s)
                    }
                } else {
                    (None, s)
                }
            }
            2 => {
                let n0 = args.get_item(0)?;
                let ns_s = if n0.is_none() { None } else { Some(n0.extract::<String>()?) };
                (ns_s, args.get_item(1)?.extract::<String>()?)
            }
            _ => {
                return Err(IllegalArgumentException::new_err(
                    "find-keyword: 1 or 2 args",
                ))
            }
        };
        match crate::keyword::find_keyword(py, ns_opt.as_deref(), &name) {
            Some(kw) => Ok(kw.into_any()),
            None => Ok(py.None()),
        }
    })?;

    // (compare a b) — three-way comparison via Comparable protocol.
    intern_fn(py, &rt_ns, "compare", |args, py| {
        need_args(args, 2, "compare")?;
        let r = crate::rt::compare(py, args.get_item(0)?.unbind(), args.get_item(1)?.unbind())?;
        Ok(r.into_pyobject(py)?.unbind().into_any())
    })?;

    // (lazy-seq thunk) — wraps a 0-arg callable in a LazySeq. The macro
    // expansion in core.clj produces `(clojure.lang.RT/lazy-seq (fn* [] body))`.
    intern_fn(py, &rt_ns, "lazy-seq", |args, py| {
        need_args(args, 1, "lazy-seq")?;
        let thunk = args.get_item(0)?.unbind();
        let ls = crate::seqs::lazy_seq::py_lazy_seq(thunk);
        Ok(Py::new(py, ls)?.into_any())
    })?;

    // (delay thunk) — wraps a 0-arg callable in a Delay (memoized).
    intern_fn(py, &rt_ns, "delay", |args, py| {
        need_args(args, 1, "delay")?;
        let thunk = args.get_item(0)?.unbind();
        let d = crate::seqs::delay::Delay::new(thunk);
        Ok(Py::new(py, d)?.into_any())
    })?;

    // (force x) — if x is Delay, force; else return x.
    intern_fn(py, &rt_ns, "force", |args, py| {
        need_args(args, 1, "force")?;
        let x = args.get_item(0)?.unbind();
        crate::seqs::delay::py_force(py, x)
    })?;

    // (instance-delay? x)
    intern_fn(py, &rt_ns, "instance-delay?", |args, py| {
        need_args(args, 1, "instance-delay?")?;
        let x = args.get_item(0)?;
        Ok(PyBool::new(py, x.cast::<crate::seqs::delay::Delay>().is_ok())
            .to_owned().unbind().into_any())
    })?;

    // --- Chunked seqs ---

    // (chunk-buffer capacity)
    intern_fn(py, &rt_ns, "chunk-buffer", |args, py| {
        need_args(args, 1, "chunk-buffer")?;
        let cap: usize = args.get_item(0)?.extract()?;
        Ok(Py::new(py, crate::seqs::chunk_buffer::ChunkBuffer::new(cap))?.into_any())
    })?;

    // (chunk-append b x) — mutate-in-place on ChunkBuffer.
    intern_fn(py, &rt_ns, "chunk-append", |args, py| {
        need_args(args, 2, "chunk-append")?;
        let b = args.get_item(0)?;
        let buf = b.cast::<crate::seqs::chunk_buffer::ChunkBuffer>().map_err(|_| {
            IllegalArgumentException::new_err("chunk-append: first arg must be a ChunkBuffer")
        })?;
        buf.get().push(args.get_item(1)?.unbind())?;
        Ok(py.None())
    })?;

    // (chunk b) — seal ChunkBuffer into an ArrayChunk.
    intern_fn(py, &rt_ns, "chunk", |args, py| {
        need_args(args, 1, "chunk")?;
        let b = args.get_item(0)?;
        let buf = b.cast::<crate::seqs::chunk_buffer::ChunkBuffer>().map_err(|_| {
            IllegalArgumentException::new_err("chunk: first arg must be a ChunkBuffer")
        })?;
        Ok(buf.get().seal(py)?.into_any())
    })?;

    // (chunk-first s) / (chunk-rest s) / (chunk-next s) — IChunkedSeq helpers.
    intern_fn(py, &rt_ns, "chunk-first", |args, py| {
        need_args(args, 1, "chunk-first")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "IChunkedSeq", "chunked_first",
            args.get_item(0)?.unbind(),
        )
    })?;

    intern_fn(py, &rt_ns, "chunk-rest", |args, py| {
        need_args(args, 1, "chunk-rest")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "IChunkedSeq", "chunked_more",
            args.get_item(0)?.unbind(),
        )
    })?;

    intern_fn(py, &rt_ns, "chunk-next", |args, py| {
        need_args(args, 1, "chunk-next")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "IChunkedSeq", "chunked_next",
            args.get_item(0)?.unbind(),
        )
    })?;

    // (chunk-cons chunk rest) — if chunk empty return rest, else build ChunkedCons.
    intern_fn(py, &rt_ns, "chunk-cons", |args, py| {
        need_args(args, 2, "chunk-cons")?;
        let chunk_any = args.get_item(0)?;
        let rest = args.get_item(1)?.unbind();
        let ac = chunk_any.cast::<crate::seqs::array_chunk::ArrayChunk>().map_err(|_| {
            IllegalArgumentException::new_err("chunk-cons: first arg must be an ArrayChunk")
        })?;
        let cnt = {
            let g = ac.get();
            g.items.len().saturating_sub(g.offset)
        };
        if cnt == 0 {
            return Ok(rest);
        }
        let cc = crate::seqs::chunked_cons::ChunkedCons::new(ac.clone().unbind(), rest);
        Ok(Py::new(py, cc)?.into_any())
    })?;

    // (instance-chunked-seq? x) — implements IChunkedSeq?
    mk_protocol_pred(py, &rt_ns, "instance-chunked-seq?", get_proto(m, "IChunkedSeq")?)?;

    // --- Reduce protocols ---

    // (coll-reduce coll f) / (coll-reduce coll f init)
    intern_fn(py, &rt_ns, "coll-reduce", |args, py| {
        let coll = args.get_item(0)?.unbind();
        let f = args.get_item(1)?.unbind();
        match args.len() {
            2 => {
                static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
                crate::protocol_fn::dispatch_cached_2(
                    py, &PFN, "CollReduce", "coll_reduce1", coll, f,
                )
            }
            3 => {
                let init = args.get_item(2)?.unbind();
                static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
                crate::protocol_fn::dispatch_cached_3(
                    py, &PFN, "CollReduce", "coll_reduce2", coll, f, init,
                )
            }
            _ => Err(IllegalArgumentException::new_err("coll-reduce: 2 or 3 args")),
        }
    })?;

    // (kv-reduce coll f init)
    intern_fn(py, &rt_ns, "kv-reduce", |args, py| {
        need_args(args, 3, "kv-reduce")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_3(
            py, &PFN, "IKVReduce", "kv_reduce",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
            args.get_item(2)?.unbind(),
        )
    })?;

    // --- Arithmetic 2-arg primitives. Python's operator dispatch handles
    //     mixed int/float; type errors bubble up as TypeError. ---

    intern_fn(py, &rt_ns, "add", |args, py| {
        need_args(args, 2, "add")?;
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        ensure_numeric(&a, "+")?;
        ensure_numeric(&b, "+")?;
        let r = normalize_ratio(py, a.add(b)?)?;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "subtract", |args, py| {
        need_args(args, 2, "subtract")?;
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        ensure_numeric(&a, "-")?;
        ensure_numeric(&b, "-")?;
        let r = normalize_ratio(py, a.sub(b)?)?;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "multiply", |args, py| {
        need_args(args, 2, "multiply")?;
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        ensure_numeric(&a, "*")?;
        ensure_numeric(&b, "*")?;
        let r = normalize_ratio(py, a.mul(b)?)?;
        Ok(r.unbind())
    })?;
    // Division matches vanilla Clojure semantics: int/int yields an exact
    // result. If the quotient is a whole number it's returned as an int;
    // otherwise a `fractions.Fraction` is returned. Non-int operands use
    // Python's normal `/` (float for int/float, Fraction for Fraction ops,
    // Decimal for Decimal ops).
    intern_fn(py, &rt_ns, "divide", |args, py| {
        use pyo3::types::PyFloat;
        need_args(args, 2, "divide")?;
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        ensure_numeric(&a, "/")?;
        ensure_numeric(&b, "/")?;
        if is_exact_int(&a) && is_exact_int(&b) {
            let zero = 0i64.into_pyobject(py)?;
            if b.eq(&zero)? {
                return Err(pyo3::exceptions::PyZeroDivisionError::new_err(
                    "Divide by zero",
                ));
            }
            let fractions = py.import("fractions")?;
            let frac = fractions.getattr("Fraction")?.call1((&a, &b))?;
            return normalize_ratio(py, frac).map(|v| v.unbind());
        }
        // IEEE-754: when either operand is a float, division by zero yields
        // ±Inf / NaN (not an exception). Vanilla Clojure follows the JVM here.
        if a.cast::<PyFloat>().is_ok() || b.cast::<PyFloat>().is_ok() {
            let af: f64 = a.extract().unwrap_or(f64::NAN);
            let bf: f64 = b.extract().unwrap_or(f64::NAN);
            let r = af / bf;
            return Ok(r.into_pyobject(py)?.into_any().unbind());
        }
        let r = a.div(b)?;
        normalize_ratio(py, r).map(|v| v.unbind())
    })?;

    // Clojure's quot/rem truncate toward zero (JVM semantics). Python `//`
    // floors, so we route through `divmod` and correct the sign of the
    // quotient when dividend and divisor have opposite signs with nonzero
    // remainder.
    intern_fn(py, &rt_ns, "quot", |args, py| {
        need_args(args, 2, "quot")?;
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        let builtins = py.import("builtins")?;
        let pair = builtins.getattr("divmod")?.call1((&a, &b))?;
        let q = pair.get_item(0)?;
        let r = pair.get_item(1)?;
        let zero = 0i64.into_pyobject(py)?;
        let r_is_zero: bool = r.eq(&zero)?;
        if !r_is_zero {
            let a_neg: bool = a.lt(&zero)?;
            let b_neg: bool = b.lt(&zero)?;
            if a_neg != b_neg {
                let one = 1i64.into_pyobject(py)?;
                let q_adj = q.add(&one)?;
                return Ok(normalize_ratio(py, q_adj)?.unbind());
            }
        }
        Ok(normalize_ratio(py, q)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "rem", |args, py| {
        need_args(args, 2, "rem")?;
        // rem = a - quot(a, b) * b — compute via Python.
        let a = args.get_item(0)?;
        let b = args.get_item(1)?;
        let builtins = py.import("builtins")?;
        let pair = builtins.getattr("divmod")?.call1((&a, &b))?;
        let q = pair.get_item(0)?;
        let r = pair.get_item(1)?;
        let zero = 0i64.into_pyobject(py)?;
        let r_is_zero: bool = r.eq(&zero)?;
        if !r_is_zero {
            let a_neg: bool = a.lt(&zero)?;
            let b_neg: bool = b.lt(&zero)?;
            if a_neg != b_neg {
                // r_quot = r - b (Clojure rem's sign follows dividend)
                let r_adj = r.sub(&b)?;
                return normalize_ratio(py, r_adj).map(|v| v.unbind());
            }
        }
        let _ = q;
        normalize_ratio(py, r).map(|v| v.unbind())
    })?;

    // --- Ordering 2-arg primitives (return Python bool). ---

    intern_fn(py, &rt_ns, "lt", |args, py| {
        need_args(args, 2, "lt")?;
        let r: bool = args.get_item(0)?.lt(args.get_item(1)?)?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;
    intern_fn(py, &rt_ns, "gt", |args, py| {
        need_args(args, 2, "gt")?;
        let r: bool = args.get_item(0)?.gt(args.get_item(1)?)?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;
    intern_fn(py, &rt_ns, "lte", |args, py| {
        need_args(args, 2, "lte")?;
        let r: bool = args.get_item(0)?.le(args.get_item(1)?)?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;
    intern_fn(py, &rt_ns, "gte", |args, py| {
        need_args(args, 2, "gte")?;
        let r: bool = args.get_item(0)?.ge(args.get_item(1)?)?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // --- Unary arithmetic. ---

    intern_fn(py, &rt_ns, "inc", |args, py| {
        need_args(args, 1, "inc")?;
        let one = 1i64.into_pyobject(py)?;
        let r = args.get_item(0)?.add(&one)?;
        normalize_ratio(py, r).map(|v| v.unbind())
    })?;
    intern_fn(py, &rt_ns, "dec", |args, py| {
        need_args(args, 1, "dec")?;
        let one = 1i64.into_pyobject(py)?;
        let r = args.get_item(0)?.sub(&one)?;
        normalize_ratio(py, r).map(|v| v.unbind())
    })?;
    intern_fn(py, &rt_ns, "negate", |args, py| {
        need_args(args, 1, "negate")?;
        let r = args.get_item(0)?.neg()?;
        normalize_ratio(py, r).map(|v| v.unbind())
    })?;
    intern_fn(py, &rt_ns, "abs", |args, py| {
        need_args(args, 1, "abs")?;
        let b = py.import("builtins")?;
        let r = b.getattr("abs")?.call1((args.get_item(0)?,))?;
        Ok(r.unbind())
    })?;

    // (coerce-int x) — Python int(x). Truncates floats toward zero.
    intern_fn(py, &rt_ns, "coerce-int", |args, py| {
        need_args(args, 1, "coerce-int")?;
        let b = py.import("builtins")?;
        let r = b.getattr("int")?.call1((args.get_item(0)?,))?;
        Ok(r.unbind())
    })?;

    // (chunk-reduce chunk f init) — dispatches IChunk::chunk_reduce. Used by
    // the Clojure-level `reduce1` and any other chunk-at-a-time consumer.
    intern_fn(py, &rt_ns, "chunk-reduce", |args, py| {
        need_args(args, 3, "chunk-reduce")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_3(
            py, &PFN, "IChunk", "chunk_reduce",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
            args.get_item(2)?.unbind(),
        )
    })?;

    // --- Type predicates backed by Python isinstance. ---

    // Clojure's integer? / int? both mean "integral". In Python we accept
    // `int` but exclude `bool` (which is a subclass of int).
    intern_fn(py, &rt_ns, "integer?", |args, py| {
        need_args(args, 1, "integer?")?;
        let x = args.get_item(0)?;
        let is_bool = x.cast::<PyBool>().is_ok();
        let is_int = x.cast::<pyo3::types::PyInt>().is_ok();
        Ok(PyBool::new(py, is_int && !is_bool).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "double?", |args, py| {
        need_args(args, 1, "double?")?;
        let x = args.get_item(0)?;
        Ok(PyBool::new(py, x.cast::<pyo3::types::PyFloat>().is_ok())
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "number?", |args, py| {
        need_args(args, 1, "number?")?;
        let x = args.get_item(0)?;
        let is_bool = x.cast::<PyBool>().is_ok();
        let is_int = x.cast::<pyo3::types::PyInt>().is_ok();
        let is_float = x.cast::<pyo3::types::PyFloat>().is_ok();
        if (is_int && !is_bool) || is_float {
            return Ok(PyBool::new(py, true).to_owned().unbind().into_any());
        }
        // Fraction or Decimal also count as numbers for the numeric tower.
        if x.is_instance(fraction_cls(py)?.as_any())? {
            return Ok(PyBool::new(py, true).to_owned().unbind().into_any());
        }
        if x.is_instance(decimal_cls(py)?.as_any())? {
            return Ok(PyBool::new(py, true).to_owned().unbind().into_any());
        }
        Ok(PyBool::new(py, false).to_owned().unbind().into_any())
    })?;

    // --- Collection access backed by existing protocols. ---

    intern_fn(py, &rt_ns, "get", |args, py| {
        match args.len() {
            2 => {
                crate::rt::get(
                    py,
                    args.get_item(0)?.unbind(),
                    args.get_item(1)?.unbind(),
                    py.None(),
                )
            }
            3 => {
                crate::rt::get(
                    py,
                    args.get_item(0)?.unbind(),
                    args.get_item(1)?.unbind(),
                    args.get_item(2)?.unbind(),
                )
            }
            _ => Err(IllegalArgumentException::new_err("get: 2 or 3 args")),
        }
    })?;

    intern_fn(py, &rt_ns, "contains?", |args, py| {
        need_args(args, 2, "contains?")?;
        let coll = args.get_item(0)?.unbind();
        if coll.is_none(py) {
            return Ok(false_py(py));
        }
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        match crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "Associative", "contains_key",
            coll, args.get_item(1)?.unbind(),
        ) {
            Ok(v) => Ok(v),
            // Sets should respond to contains? too; fall through if not Associative.
            Err(e) if e.is_instance_of::<IllegalArgumentException>(py) => {
                // Try IPersistentSet — a set "contains" the key if get returns it.
                // For simplicity we treat `(contains? set x)` as set membership.
                let s = args.get_item(0)?;
                let contains = s.contains(args.get_item(1)?)?;
                Ok(PyBool::new(py, contains).to_owned().unbind().into_any())
            }
            Err(e) => Err(e),
        }
    })?;

    intern_fn(py, &rt_ns, "find", |args, py| {
        need_args(args, 2, "find")?;
        let coll = args.get_item(0)?.unbind();
        if coll.is_none(py) {
            return Ok(py.None());
        }
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "Associative", "entry_at",
            coll, args.get_item(1)?.unbind(),
        )
    })?;

    intern_fn(py, &rt_ns, "dissoc", |args, py| {
        need_args(args, 2, "dissoc")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "IPersistentMap", "without",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    intern_fn(py, &rt_ns, "disj", |args, py| {
        need_args(args, 2, "disj")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "IPersistentSet", "disjoin",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    intern_fn(py, &rt_ns, "peek", |args, py| {
        need_args(args, 1, "peek")?;
        let coll = args.get_item(0)?.unbind();
        if coll.is_none(py) { return Ok(py.None()); }
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(py, &PFN, "IPersistentStack", "peek", coll)
    })?;

    intern_fn(py, &rt_ns, "pop", |args, py| {
        need_args(args, 1, "pop")?;
        let coll = args.get_item(0)?.unbind();
        if coll.is_none(py) { return Ok(py.None()); }
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(py, &PFN, "IPersistentStack", "pop", coll)
    })?;

    // (keys map) / (vals map) — walk the map yielding a seq of keys or values.
    // Build a PersistentList in-order by using the map's native seq (which
    // produces MapEntry-bearing seq) and mapping to key/val.
    intern_fn(py, &rt_ns, "keys", |args, py| {
        need_args(args, 1, "keys")?;
        let coll = args.get_item(0)?.unbind();
        if coll.is_none(py) { return Ok(py.None()); }
        let s = crate::rt::seq(py, coll)?;
        if s.is_none(py) { return Ok(py.None()); }
        let mut items: Vec<PyObject> = Vec::new();
        let mut cur = s;
        while !cur.is_none(py) {
            let me = crate::rt::first(py, cur.clone_ref(py))?;
            let k = me.bind(py).getattr("key")?;
            items.push(k.unbind());
            cur = crate::rt::next_(py, cur)?;
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    intern_fn(py, &rt_ns, "vals", |args, py| {
        need_args(args, 1, "vals")?;
        let coll = args.get_item(0)?.unbind();
        if coll.is_none(py) { return Ok(py.None()); }
        let s = crate::rt::seq(py, coll)?;
        if s.is_none(py) { return Ok(py.None()); }
        let mut items: Vec<PyObject> = Vec::new();
        let mut cur = s;
        while !cur.is_none(py) {
            let me = crate::rt::first(py, cur.clone_ref(py))?;
            let v = me.bind(py).getattr("val")?;
            items.push(v.unbind());
            cur = crate::rt::next_(py, cur)?;
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // (name x) / (namespace x) — route through Symbol/Keyword attributes.
    intern_fn(py, &rt_ns, "name", |args, py| {
        need_args(args, 1, "name")?;
        let x = args.get_item(0)?;
        if x.cast::<pyo3::types::PyString>().is_ok() {
            return Ok(x.unbind());
        }
        Ok(x.getattr("name")?.unbind())
    })?;

    intern_fn(py, &rt_ns, "namespace", |args, py| {
        need_args(args, 1, "namespace")?;
        let x = args.get_item(0)?;
        let ns = x.getattr("ns")?;
        let _ = py;
        Ok(ns.unbind())
    })?;

    // (instance-map-entry? x) — used by core.clj's `map-entry?`.
    let me_cls: PyObject = m.getattr("MapEntry")?.unbind();
    mk_instance_pred(py, &rt_ns, "instance-map-entry?", me_cls)?;

    // --- Reduced (short-circuit wrapper) ---

    // (reduced x) — construct
    intern_fn(py, &rt_ns, "reduced", |args, py| {
        need_args(args, 1, "reduced")?;
        let r = crate::reduced::Reduced::new(args.get_item(0)?.unbind());
        Ok(Py::new(py, r)?.into_any())
    })?;

    // (reduced? x)
    intern_fn(py, &rt_ns, "reduced?", |args, py| {
        need_args(args, 1, "reduced?")?;
        let x = args.get_item(0)?;
        Ok(PyBool::new(py, x.cast::<crate::reduced::Reduced>().is_ok())
            .to_owned().unbind().into_any())
    })?;

    // (unreduced x) — unwrap if Reduced, else return as-is
    intern_fn(py, &rt_ns, "unreduced", |args, py| {
        need_args(args, 1, "unreduced")?;
        Ok(crate::reduced::unreduced(py, args.get_item(0)?.unbind()))
    })?;

    // (ensure-reduced x) — wrap unconditionally
    intern_fn(py, &rt_ns, "ensure-reduced", |args, py| {
        need_args(args, 1, "ensure-reduced")?;
        let x = args.get_item(0)?;
        if x.cast::<crate::reduced::Reduced>().is_ok() {
            return Ok(x.unbind());
        }
        let r = crate::reduced::Reduced::new(x.unbind());
        Ok(Py::new(py, r)?.into_any())
    })?;

    // --- Sort ---

    // (sort-with cmp coll) — materialize coll, sort via cmp (a 2-arg fn
    // returning a signed int), return a seq.
    intern_fn(py, &rt_ns, "sort-with", |args, py| {
        need_args(args, 2, "sort-with")?;
        let cmp = args.get_item(0)?.unbind();
        let coll = args.get_item(1)?.unbind();

        let mut items: Vec<PyObject> = Vec::new();
        let mut cur = crate::rt::seq(py, coll)?;
        while !cur.is_none(py) {
            items.push(crate::rt::first(py, cur.clone_ref(py))?);
            cur = crate::rt::next_(py, cur)?;
        }

        // Rust Vec::sort_by can't propagate errors; we stash the first error
        // in a slot and treat all subsequent comparisons as Equal.
        let mut first_err: Option<PyErr> = None;
        items.sort_by(|a, b| {
            if first_err.is_some() {
                return std::cmp::Ordering::Equal;
            }
            match crate::rt::invoke_n(
                py,
                cmp.clone_ref(py),
                &[a.clone_ref(py), b.clone_ref(py)],
            ) {
                Ok(r) => {
                    // Predicate mode: `(pred a b) → true` means a comes first.
                    // Check PyBool BEFORE int (bool is a subclass of int in
                    // Python; extract::<i64> would return 1/0 and we'd
                    // misinterpret direction).
                    let r_b = r.bind(py);
                    if let Ok(b_v) = r_b.cast::<PyBool>() {
                        if b_v.is_true() {
                            std::cmp::Ordering::Less
                        } else {
                            // `(pred a b) = false` — check reverse for Greater.
                            match crate::rt::invoke_n(
                                py,
                                cmp.clone_ref(py),
                                &[b.clone_ref(py), a.clone_ref(py)],
                            ) {
                                Ok(r2) => {
                                    if let Ok(b2) = r2.bind(py).cast::<PyBool>() {
                                        if b2.is_true() {
                                            std::cmp::Ordering::Greater
                                        } else {
                                            std::cmp::Ordering::Equal
                                        }
                                    } else {
                                        std::cmp::Ordering::Equal
                                    }
                                }
                                Err(e) => {
                                    first_err = Some(e);
                                    std::cmp::Ordering::Equal
                                }
                            }
                        }
                    } else {
                        match r_b.extract::<i64>() {
                            Ok(n) if n < 0 => std::cmp::Ordering::Less,
                            Ok(n) if n > 0 => std::cmp::Ordering::Greater,
                            Ok(_) => std::cmp::Ordering::Equal,
                            Err(e) => {
                                first_err = Some(e);
                                std::cmp::Ordering::Equal
                            }
                        }
                    }
                }
                Err(e) => {
                    first_err = Some(e);
                    std::cmp::Ordering::Equal
                }
            }
        });
        if let Some(e) = first_err {
            return Err(e);
        }
        // Return as a seq over a persistent vector (vanilla returns a seq).
        let tup = PyTuple::new(py, &items)?;
        let v = crate::collections::pvector::vector(py, tup)?;
        crate::rt::seq(py, v.into_any())
    })?;

    // --- Atom / deref ---

    // (atom initial) — construct
    intern_fn(py, &rt_ns, "atom", |args, py| {
        need_args(args, 1, "atom")?;
        let a = crate::atom::Atom::new(py, args.get_item(0)?.unbind());
        Ok(Py::new(py, a)?.into_any())
    })?;

    // (deref x) — dispatch through IDeref protocol.
    intern_fn(py, &rt_ns, "deref", |args, py| {
        need_args(args, 1, "deref")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "IDeref", "deref",
            args.get_item(0)?.unbind(),
        )
    })?;

    // (swap-bang atom f & args) — forwarded to Atom::swap_bang.
    intern_fn(py, &rt_ns, "swap-bang", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err("swap! requires at least an atom and a fn"));
        }
        let a = args.get_item(0)?;
        let atom = a.cast::<crate::atom::Atom>().map_err(|_| {
            IllegalArgumentException::new_err("swap!: first arg must be an Atom")
        })?;
        let f = args.get_item(1)?.unbind();
        let rest: Vec<PyObject> = (2..args.len())
            .map(|i| -> PyResult<_> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        let rest_tup = PyTuple::new(py, &rest)?;
        crate::atom::Atom::swap(atom.clone().unbind(), py, f, rest_tup)
    })?;

    intern_fn(py, &rt_ns, "swap-vals-bang", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err("swap-vals! requires at least an atom and a fn"));
        }
        let a = args.get_item(0)?;
        let atom = a.cast::<crate::atom::Atom>().map_err(|_| {
            IllegalArgumentException::new_err("swap-vals!: first arg must be an Atom")
        })?;
        let f = args.get_item(1)?.unbind();
        let rest: Vec<PyObject> = (2..args.len())
            .map(|i| -> PyResult<_> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        let rest_tup = PyTuple::new(py, &rest)?;
        crate::atom::Atom::swap_vals(atom.clone().unbind(), py, f, rest_tup)
    })?;

    intern_fn(py, &rt_ns, "reset-bang", |args, py| {
        need_args(args, 2, "reset!")?;
        let a = args.get_item(0)?;
        let atom = a.cast::<crate::atom::Atom>().map_err(|_| {
            IllegalArgumentException::new_err("reset!: first arg must be an Atom")
        })?;
        crate::atom::Atom::reset(atom.clone().unbind(), py, args.get_item(1)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "reset-vals-bang", |args, py| {
        need_args(args, 2, "reset-vals!")?;
        let a = args.get_item(0)?;
        let atom = a.cast::<crate::atom::Atom>().map_err(|_| {
            IllegalArgumentException::new_err("reset-vals!: first arg must be an Atom")
        })?;
        crate::atom::Atom::reset_vals(atom.clone().unbind(), py, args.get_item(1)?.unbind())
    })?;

    intern_fn(py, &rt_ns, "compare-and-set-bang", |args, py| {
        need_args(args, 3, "compare-and-set!")?;
        let a = args.get_item(0)?;
        let atom = a.cast::<crate::atom::Atom>().map_err(|_| {
            IllegalArgumentException::new_err("compare-and-set!: first arg must be an Atom")
        })?;
        let r = crate::atom::Atom::compare_and_set(
            atom.clone().unbind(),
            py,
            args.get_item(1)?.unbind(),
            args.get_item(2)?.unbind(),
        )?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // --- Volatile ---

    intern_fn(py, &rt_ns, "volatile", |args, py| {
        need_args(args, 1, "volatile!")?;
        let v = crate::volatile::Volatile::new(args.get_item(0)?.unbind());
        Ok(Py::new(py, v)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "volatile?", |args, py| {
        need_args(args, 1, "volatile?")?;
        let x = args.get_item(0)?;
        Ok(PyBool::new(py, x.cast::<crate::volatile::Volatile>().is_ok())
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "vreset", |args, py| {
        need_args(args, 2, "vreset!")?;
        let a = args.get_item(0)?;
        let v = a.cast::<crate::volatile::Volatile>().map_err(|_| {
            IllegalArgumentException::new_err("vreset!: first arg must be a Volatile")
        })?;
        Ok(v.get().reset(py, args.get_item(1)?.unbind()))
    })?;

    intern_fn(py, &rt_ns, "vswap", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err("vswap! requires at least a volatile and a fn"));
        }
        let a = args.get_item(0)?;
        let v = a.cast::<crate::volatile::Volatile>().map_err(|_| {
            IllegalArgumentException::new_err("vswap!: first arg must be a Volatile")
        })?;
        let f = args.get_item(1)?.unbind();
        let rest: Vec<PyObject> = (2..args.len())
            .map(|i| -> PyResult<_> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        v.get().vswap(py, f, &rest)
    })?;

    // (boolean x) — Clojure truthy coercion: nil/false → false, else true.
    intern_fn(py, &rt_ns, "boolean", |args, py| {
        need_args(args, 1, "boolean")?;
        let x = args.get_item(0)?;
        let truthy = !x.is_none() && !matches!(x.cast::<PyBool>(), Ok(b) if !b.is_true());
        Ok(PyBool::new(py, truthy).to_owned().unbind().into_any())
    })?;

    // --- nth — Indexed fast path, seq-walk fallback. ---

    intern_fn(py, &rt_ns, "nth", |args, py| {
        match args.len() {
            2 => {
                let coll = args.get_item(0)?.unbind();
                let i = args.get_item(1)?.unbind();
                rt_nth(py, coll, i, None)
            }
            3 => {
                let coll = args.get_item(0)?.unbind();
                let i = args.get_item(1)?.unbind();
                let default = args.get_item(2)?.unbind();
                rt_nth(py, coll, i, Some(default))
            }
            _ => Err(IllegalArgumentException::new_err("nth: 2 or 3 args")),
        }
    })?;

    // --- Bit operations. ---

    intern_fn(py, &rt_ns, "bit-not", |args, py| {
        need_args(args, 1, "bit-not")?;
        let r = args.get_item(0)?.call_method0("__invert__")?;
        let _ = py;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-and", |args, py| {
        need_args(args, 2, "bit-and")?;
        let r = args.get_item(0)?.call_method1("__and__", (args.get_item(1)?,))?;
        let _ = py;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-or", |args, py| {
        need_args(args, 2, "bit-or")?;
        let r = args.get_item(0)?.call_method1("__or__", (args.get_item(1)?,))?;
        let _ = py;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-xor", |args, py| {
        need_args(args, 2, "bit-xor")?;
        let r = args.get_item(0)?.call_method1("__xor__", (args.get_item(1)?,))?;
        let _ = py;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-and-not", |args, py| {
        need_args(args, 2, "bit-and-not")?;
        let inv = args.get_item(1)?.call_method0("__invert__")?;
        let r = args.get_item(0)?.call_method1("__and__", (inv,))?;
        let _ = py;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-shift-left", |args, py| {
        need_args(args, 2, "bit-shift-left")?;
        let r = args.get_item(0)?.call_method1("__lshift__", (args.get_item(1)?,))?;
        let _ = py;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-shift-right", |args, py| {
        need_args(args, 2, "bit-shift-right")?;
        let r = args.get_item(0)?.call_method1("__rshift__", (args.get_item(1)?,))?;
        let _ = py;
        Ok(r.unbind())
    })?;
    // JVM semantics: treat the operand as a 64-bit unsigned long before shifting.
    intern_fn(py, &rt_ns, "unsigned-bit-shift-right", |args, py| {
        need_args(args, 2, "unsigned-bit-shift-right")?;
        let mask: Py<pyo3::types::PyInt> = 0xFFFFFFFFFFFFFFFFu64.into_pyobject(py)?.into();
        let masked = args.get_item(0)?.call_method1("__and__", (mask.bind(py),))?;
        let r = masked.call_method1("__rshift__", (args.get_item(1)?,))?;
        Ok(r.unbind())
    })?;
    // bit-clear / bit-set / bit-flip / bit-test operate on a single bit index.
    intern_fn(py, &rt_ns, "bit-clear", |args, py| {
        need_args(args, 2, "bit-clear")?;
        let one = 1i64.into_pyobject(py)?;
        let shifted = one.call_method1("__lshift__", (args.get_item(1)?,))?;
        let mask = shifted.call_method0("__invert__")?;
        let r = args.get_item(0)?.call_method1("__and__", (mask,))?;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-set", |args, py| {
        need_args(args, 2, "bit-set")?;
        let one = 1i64.into_pyobject(py)?;
        let mask = one.call_method1("__lshift__", (args.get_item(1)?,))?;
        let r = args.get_item(0)?.call_method1("__or__", (mask,))?;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-flip", |args, py| {
        need_args(args, 2, "bit-flip")?;
        let one = 1i64.into_pyobject(py)?;
        let mask = one.call_method1("__lshift__", (args.get_item(1)?,))?;
        let r = args.get_item(0)?.call_method1("__xor__", (mask,))?;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "bit-test", |args, py| {
        need_args(args, 2, "bit-test")?;
        let shifted = args.get_item(0)?.call_method1("__rshift__", (args.get_item(1)?,))?;
        let one = 1i64.into_pyobject(py)?;
        let masked = shifted.call_method1("__and__", (&one,))?;
        let r: bool = masked.eq(&one)?;
        Ok(PyBool::new(py, r).to_owned().unbind().into_any())
    })?;

    // --- Transients (vanilla core.clj 3364-3430) ---
    //
    // Each `intern_fn` wraps a hot path; using `dispatch_cached_N` with a
    // per-callsite `OnceCell<Py<ProtocolFn>>` skips the repeated DashMap
    // lookup, Arc<str> allocation, PyTuple allocation, and Python `getattr`
    // that the legacy `dispatch::dispatch` shim would do per call.

    // (transient coll) — IEditableCollection/as_transient.
    intern_fn(py, &rt_ns, "transient", |args, py| {
        need_args(args, 1, "transient")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "IEditableCollection", "as_transient",
            args.get_item(0)?.unbind(),
        )
    })?;

    // (persistent! t) — ITransientCollection/persistent_bang.
    intern_fn(py, &rt_ns, "persistent-bang", |args, py| {
        need_args(args, 1, "persistent!")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "ITransientCollection", "persistent_bang",
            args.get_item(0)?.unbind(),
        )
    })?;

    // (conj! t x) — ITransientCollection/conj_bang.
    intern_fn(py, &rt_ns, "conj-bang", |args, py| {
        need_args(args, 2, "conj!")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "ITransientCollection", "conj_bang",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    // (assoc! t k v) — ITransientAssociative/assoc_bang.
    intern_fn(py, &rt_ns, "assoc-bang", |args, py| {
        need_args(args, 3, "assoc!")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_3(
            py, &PFN, "ITransientAssociative", "assoc_bang",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
            args.get_item(2)?.unbind(),
        )
    })?;

    // (dissoc! t k) — ITransientMap/dissoc_bang.
    intern_fn(py, &rt_ns, "dissoc-bang", |args, py| {
        need_args(args, 2, "dissoc!")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "ITransientMap", "dissoc_bang",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    // (pop! t) — ITransientVector/pop_bang.
    intern_fn(py, &rt_ns, "pop-bang", |args, py| {
        need_args(args, 1, "pop!")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "ITransientVector", "pop_bang",
            args.get_item(0)?.unbind(),
        )
    })?;

    // (disj! t k) — ITransientSet/disj_bang.
    intern_fn(py, &rt_ns, "disj-bang", |args, py| {
        need_args(args, 2, "disj!")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "ITransientSet", "disj_bang",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    // --- Reversible / rseq (vanilla 1600) ---
    intern_fn(py, &rt_ns, "rseq", |args, py| {
        need_args(args, 1, "rseq")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "Reversible", "rseq",
            args.get_item(0)?.unbind(),
        )
    })?;

    // --- Numeric: num (vanilla 3503) ---
    // Pass-through if Number, else ClassCastException/IAE. Python: int/float/bool/complex.
    intern_fn(py, &rt_ns, "num", |args, py| {
        need_args(args, 1, "num")?;
        let x_b = args.get_item(0)?;
        let b = x_b.clone();
        use pyo3::types::{PyInt, PyFloat, PyComplex};
        if b.cast::<PyInt>().is_ok() || b.cast::<PyFloat>().is_ok() || b.cast::<PyComplex>().is_ok() {
            return Ok(b.unbind());
        }
        let _ = py;
        Err(IllegalArgumentException::new_err("num: not a number"))
    })?;

    // --- Namespaces (vanilla 4156-4201) ---

    // (find-ns sym) — returns the Namespace module or nil.
    intern_fn(py, &rt_ns, "find-ns", |args, py| {
        need_args(args, 1, "find-ns")?;
        let sym_obj = args.get_item(0)?;
        let sym = sym_obj.cast::<Symbol>().map_err(|_| {
            IllegalArgumentException::new_err("find-ns: arg must be a Symbol")
        })?;
        Ok(crate::namespace::find_ns(py, sym.clone().unbind())?
            .unwrap_or_else(|| py.None()))
    })?;

    // (create-ns sym) — return an existing namespace or create a new one.
    intern_fn(py, &rt_ns, "create-ns", |args, py| {
        need_args(args, 1, "create-ns")?;
        let sym_obj = args.get_item(0)?;
        let sym = sym_obj.cast::<Symbol>().map_err(|_| {
            IllegalArgumentException::new_err("create-ns: arg must be a Symbol")
        })?;
        crate::namespace::create_ns(py, sym.clone().unbind())
    })?;

    // (the-ns ns) — coerce symbol/ns to ns; IAE if not found or not a ns.
    intern_fn(py, &rt_ns, "the-ns", |args, py| {
        need_args(args, 1, "the-ns")?;
        let x = args.get_item(0)?;
        if crate::namespace::is_clojure_namespace(py, &x)? {
            return Ok(x.unbind());
        }
        if let Ok(sym) = x.cast::<Symbol>() {
            match crate::namespace::find_ns(py, sym.clone().unbind())? {
                Some(v) => return Ok(v),
                None => {
                    return Err(IllegalArgumentException::new_err(format!(
                        "No namespace: {} found", sym.get().name
                    )));
                }
            }
        }
        Err(IllegalArgumentException::new_err("the-ns requires a namespace or symbol"))
    })?;

    // (ns-name ns) — return the ns's name Symbol.
    intern_fn(py, &rt_ns, "ns-name", |args, py| {
        need_args(args, 1, "ns-name")?;
        let x = args.get_item(0)?;
        let ns = if crate::namespace::is_clojure_namespace(py, &x)? {
            x.unbind()
        } else if let Ok(sym) = x.cast::<Symbol>() {
            match crate::namespace::find_ns(py, sym.clone().unbind())? {
                Some(v) => v,
                None => {
                    return Err(IllegalArgumentException::new_err(format!(
                        "No namespace: {} found", sym.get().name
                    )));
                }
            }
        } else {
            return Err(IllegalArgumentException::new_err("ns-name requires a namespace or symbol"));
        };
        let b = ns.bind(py);
        match b.getattr("__clj_ns__") {
            Ok(s) => Ok(s.unbind()),
            Err(_) => {
                let name = b.getattr("__name__")?.extract::<String>()?;
                let sym = Symbol::new(None, Arc::from(name));
                Ok(Py::new(py, sym)?.into_any())
            }
        }
    })?;

    // --- Vars (vanilla 4357-4370) ---

    // (find-var sym) — sym must be ns-qualified; returns the Var or nil.
    intern_fn(py, &rt_ns, "find-var", |args, py| {
        need_args(args, 1, "find-var")?;
        let sym_obj = args.get_item(0)?;
        let sym = sym_obj.cast::<Symbol>().map_err(|_| {
            IllegalArgumentException::new_err("find-var: arg must be a Symbol")
        })?;
        let s = sym.get();
        let ns_name = s.ns.as_deref().ok_or_else(|| {
            IllegalArgumentException::new_err("find-var: symbol must be namespace-qualified")
        })?;
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let Ok(ns) = modules.get_item(ns_name) else {
            return Ok(py.None());
        };
        let Ok(attr) = ns.getattr(s.name.as_ref()) else {
            return Ok(py.None());
        };
        if attr.cast::<crate::var::Var>().is_ok() {
            Ok(attr.unbind())
        } else {
            Ok(py.None())
        }
    })?;

    // (var-get v) — deref the Var.
    intern_fn(py, &rt_ns, "var-get", |args, py| {
        need_args(args, 1, "var-get")?;
        let v_obj = args.get_item(0)?;
        v_obj.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("var-get: arg must be a Var")
        })?;
        let r = v_obj.call_method0("deref")?;
        let _ = py;
        Ok(r.unbind())
    })?;

    // --- Ref / STM primitives (vanilla 2283-2533) ---
    //
    // The Clojure-layer `ref`, `alter`, `commute`, `ref-set`, `ensure`,
    // `sync`/`dosync`, `io!`, and ref-history accessors all route through
    // these RT shims. Transactional helpers live in `crate::stm::txn` and
    // require a running LockingTransaction.

    intern_fn(py, &rt_ns, "ref-new", |args, py| {
        if args.len() < 1 {
            return Err(IllegalArgumentException::new_err(
                "ref requires an initial value",
            ));
        }
        let initial = args.get_item(0)?.unbind();
        let r = crate::stm::ref_::Ref::new(py, initial);
        let r_py = Py::new(py, r)?;
        // Options parsing: keyword / value pairs after the initial value.
        if args.len() > 1 {
            let n = args.len();
            if (n - 1) % 2 != 0 {
                return Err(IllegalArgumentException::new_err(
                    "ref: options must be key/value pairs",
                ));
            }
            let mut i = 1;
            while i < n {
                let k = args.get_item(i)?;
                let v = args.get_item(i + 1)?;
                let kw = k.cast::<Keyword>().map_err(|_| {
                    IllegalArgumentException::new_err(
                        "ref: option keys must be keywords (:meta :validator :min-history :max-history)",
                    )
                })?;
                let name = kw.get().name.as_ref();
                match name {
                    "meta" => r_py.bind(py).get().meta.store(std::sync::Arc::new(
                        if v.is_none() { None } else { Some(v.unbind()) },
                    )),
                    "validator" => {
                        r_py.bind(py).get().install_validator(
                            py,
                            if v.is_none() { None } else { Some(v.unbind()) },
                        )?;
                    }
                    "min-history" => {
                        let n: usize = v.extract()?;
                        r_py.bind(py).get().min_history.store(n, std::sync::atomic::Ordering::Relaxed);
                    }
                    "max-history" => {
                        let n: usize = v.extract()?;
                        r_py.bind(py).get().max_history.store(n, std::sync::atomic::Ordering::Relaxed);
                    }
                    other => {
                        return Err(IllegalArgumentException::new_err(format!(
                            "ref: unknown option {:?}",
                            other
                        )));
                    }
                }
                i += 2;
            }
        }
        Ok(r_py.into_any())
    })?;

    intern_fn(py, &rt_ns, "ref-set-bang", |args, py| {
        need_args(args, 2, "ref-set")?;
        let r = args.get_item(0)?
            .cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("ref-set: first arg must be a Ref"))?
            .clone()
            .unbind();
        let v = args.get_item(1)?.unbind();
        crate::stm::txn::ref_set(py, r, v)
    })?;

    intern_fn(py, &rt_ns, "ref-alter", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "alter requires at least a ref and a function",
            ));
        }
        let r = args.get_item(0)?
            .cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("alter: first arg must be a Ref"))?
            .clone()
            .unbind();
        let f = args.get_item(1)?.unbind();
        // args[2..] may be nil (no extra) or a seq.
        let extras = if args.len() >= 3 {
            let rest = args.get_item(2)?.unbind();
            seq_to_vec(py, rest)?
        } else {
            Vec::new()
        };
        crate::stm::txn::alter(py, r, f, extras)
    })?;

    intern_fn(py, &rt_ns, "ref-commute", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "commute requires at least a ref and a function",
            ));
        }
        let r = args.get_item(0)?
            .cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("commute: first arg must be a Ref"))?
            .clone()
            .unbind();
        let f = args.get_item(1)?.unbind();
        let extras = if args.len() >= 3 {
            let rest = args.get_item(2)?.unbind();
            seq_to_vec(py, rest)?
        } else {
            Vec::new()
        };
        crate::stm::txn::commute(py, r, f, extras)
    })?;

    intern_fn(py, &rt_ns, "ref-ensure", |args, py| {
        need_args(args, 1, "ensure")?;
        let r = args.get_item(0)?
            .cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("ensure: arg must be a Ref"))?
            .clone()
            .unbind();
        crate::stm::txn::ensure(py, r)
    })?;

    intern_fn(py, &rt_ns, "ref-history-count", |args, py| {
        need_args(args, 1, "ref-history-count")?;
        let a0 = args.get_item(0)?;
        let r = a0.cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("ref-history-count: arg must be a Ref"))?;
        Ok((r.get().hist_count() as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "ref-min-history", |args, py| {
        if args.len() < 1 || args.len() > 2 {
            return Err(IllegalArgumentException::new_err(
                "ref-min-history: 1 or 2 args",
            ));
        }
        let a0 = args.get_item(0)?;
        let r = a0.cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("ref-min-history: first arg must be a Ref"))?;
        if args.len() == 1 {
            Ok((r.get().get_min_history() as i64).into_pyobject(py)?.unbind().into_any())
        } else {
            let n: usize = args.get_item(1)?.extract()?;
            r.get().min_history.store(n, std::sync::atomic::Ordering::Relaxed);
            Ok(r.clone().unbind().into_any())
        }
    })?;

    intern_fn(py, &rt_ns, "ref-max-history", |args, py| {
        if args.len() < 1 || args.len() > 2 {
            return Err(IllegalArgumentException::new_err(
                "ref-max-history: 1 or 2 args",
            ));
        }
        let a0 = args.get_item(0)?;
        let r = a0.cast::<crate::stm::ref_::Ref>()
            .map_err(|_| IllegalArgumentException::new_err("ref-max-history: first arg must be a Ref"))?;
        if args.len() == 1 {
            Ok((r.get().get_max_history() as i64).into_pyobject(py)?.unbind().into_any())
        } else {
            let n: usize = args.get_item(1)?.extract()?;
            r.get().max_history.store(n, std::sync::atomic::Ordering::Relaxed);
            Ok(r.clone().unbind().into_any())
        }
    })?;

    intern_fn(py, &rt_ns, "ref-run-in-txn", |args, py| {
        need_args(args, 1, "sync/dosync")?;
        let body = args.get_item(0)?.unbind();
        crate::stm::txn::run_in_transaction(py, body)
    })?;

    intern_fn(py, &rt_ns, "io-bang-check", |args, _py| {
        let _ = args;
        crate::stm::txn::assert_no_txn("io!")?;
        Ok(_py.None())
    })?;

    // --- Agents (vanilla 2075-2275) ---

    intern_fn(py, &rt_ns, "agent-new", |args, py| {
        if args.len() < 1 {
            return Err(IllegalArgumentException::new_err(
                "agent requires an initial state",
            ));
        }
        let initial = args.get_item(0)?.unbind();
        let agent = crate::agent::Agent::new(py, initial)?;
        let agent_py = Py::new(py, agent)?;
        // Options: (apply agent init options).  `options` may be nil or a seq
        // of :keyword value pairs.
        if args.len() >= 2 {
            let opts_any = args.get_item(1)?.unbind();
            if !opts_any.is_none(py) {
                let opts = seq_to_vec(py, opts_any)?;
                if opts.len() % 2 != 0 {
                    return Err(IllegalArgumentException::new_err(
                        "agent: options must be key/value pairs",
                    ));
                }
                let mut had_error_handler = false;
                let mut had_error_mode = false;
                let mut i = 0;
                while i < opts.len() {
                    let k = opts[i].bind(py);
                    let v = opts[i + 1].bind(py);
                    let kw = k.cast::<Keyword>().map_err(|_| {
                        IllegalArgumentException::new_err(
                            "agent: option keys must be keywords (:meta :validator :error-handler :error-mode)",
                        )
                    })?;
                    let name = kw.get().name.as_ref();
                    match name {
                        "meta" => agent_py.bind(py).get().meta.store(std::sync::Arc::new(
                            if v.is_none() { None } else { Some(v.clone().unbind()) },
                        )),
                        "validator" => {
                            agent_py.bind(py).get().install_validator(
                                py,
                                if v.is_none() { None } else { Some(v.clone().unbind()) },
                            )?;
                        }
                        "error-handler" => {
                            agent_py.bind(py).get().install_error_handler(
                                if v.is_none() { None } else { Some(v.clone().unbind()) },
                            );
                            had_error_handler = true;
                        }
                        "error-mode" => {
                            agent_py.bind(py).get().install_error_mode(v.clone().unbind());
                            had_error_mode = true;
                        }
                        other => {
                            return Err(IllegalArgumentException::new_err(format!(
                                "agent: unknown option {:?}",
                                other
                            )));
                        }
                    }
                    i += 2;
                }
                // Vanilla: :error-handler without explicit :error-mode shifts to :continue.
                // Explicit :error-mode always wins.
                if had_error_handler && !had_error_mode {
                    let continue_kw = crate::keyword::keyword(py, "continue", None)?;
                    agent_py.bind(py).get().install_error_mode(continue_kw.into_any());
                }
            }
        }
        Ok(agent_py.into_any())
    })?;

    fn do_send(py: Python<'_>, args: &Bound<'_, PyTuple>, exec: crate::agent::Executor) -> PyResult<PyObject> {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "send requires an agent and a function",
            ));
        }
        let a0 = args.get_item(0)?;
        let agent = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("send: first arg must be an Agent"))?
            .clone()
            .unbind();
        let f = args.get_item(1)?.unbind();
        let extras = if args.len() >= 3 {
            let rest = args.get_item(2)?.unbind();
            seq_to_vec(py, rest)?
        } else {
            Vec::new()
        };
        let a = crate::agent::dispatch(py, agent, exec, None, f, extras)?;
        Ok(a.into_any())
    }

    intern_fn(py, &rt_ns, "agent-send", |args, py| {
        do_send(py, args, crate::agent::Executor::Send)
    })?;

    intern_fn(py, &rt_ns, "agent-send-off", |args, py| {
        do_send(py, args, crate::agent::Executor::SendOff)
    })?;

    intern_fn(py, &rt_ns, "agent-send-via", |args, py| {
        if args.len() < 3 {
            return Err(IllegalArgumentException::new_err(
                "send-via requires an executor, an agent, and a function",
            ));
        }
        let exec_obj = args.get_item(0)?.unbind();
        let a1 = args.get_item(1)?;
        let agent = a1.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("send-via: second arg must be an Agent"))?
            .clone()
            .unbind();
        let f = args.get_item(2)?.unbind();
        let extras = if args.len() >= 4 {
            let rest = args.get_item(3)?.unbind();
            seq_to_vec(py, rest)?
        } else {
            Vec::new()
        };
        let a = crate::agent::dispatch(
            py,
            agent,
            crate::agent::Executor::Custom,
            Some(exec_obj),
            f,
            extras,
        )?;
        Ok(a.into_any())
    })?;

    intern_fn(py, &rt_ns, "agent-release-pending", |args, py| {
        let _ = args;
        // Drain the current txn's pending sends immediately. No-op if no txn.
        let count = if let Some(txn) = crate::stm::txn::current() {
            let sends: Vec<crate::stm::txn::PendingSend> =
                std::mem::take(&mut *txn.agent_sends.borrow_mut());
            let n = sends.len();
            for s in sends {
                crate::agent::dispatch_from_commit(py, s)?;
            }
            n
        } else {
            0
        };
        Ok((count as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "agent-await", |args, py| {
        need_args(args, 1, "await")?;
        let seq = args.get_item(0)?.unbind();
        let items = seq_to_vec(py, seq)?;
        let mut agents: Vec<Py<crate::agent::Agent>> = Vec::with_capacity(items.len());
        for it in items {
            let b = it.bind(py);
            let a = b.cast::<crate::agent::Agent>()
                .map_err(|_| IllegalArgumentException::new_err("await: argument must be an Agent"))?
                .clone()
                .unbind();
            agents.push(a);
        }
        crate::agent::agent_await(py, agents)?;
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "agent-await-for", |args, py| {
        need_args(args, 2, "await-for")?;
        let ms: u64 = args.get_item(0)?.extract()?;
        let seq = args.get_item(1)?.unbind();
        let items = seq_to_vec(py, seq)?;
        let mut agents: Vec<Py<crate::agent::Agent>> = Vec::with_capacity(items.len());
        for it in items {
            let b = it.bind(py);
            let a = b.cast::<crate::agent::Agent>()
                .map_err(|_| IllegalArgumentException::new_err("await-for: argument must be an Agent"))?
                .clone()
                .unbind();
            agents.push(a);
        }
        let ok = crate::agent::agent_await_for(py, ms, agents)?;
        Ok(pyo3::types::PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "agent-error", |args, py| {
        need_args(args, 1, "agent-error")?;
        let a0 = args.get_item(0)?;
        let a = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("agent-error: arg must be an Agent"))?;
        Ok(a.get().read_error(py))
    })?;

    intern_fn(py, &rt_ns, "agent-restart", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "restart-agent requires at least an agent and a new state",
            ));
        }
        let a0 = args.get_item(0)?;
        let agent = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("restart-agent: first arg must be an Agent"))?
            .clone()
            .unbind();
        let new_state = args.get_item(1)?.unbind();
        // Walk options for :clear-actions.
        let mut clear = false;
        if args.len() >= 3 {
            let opts_any = args.get_item(2)?.unbind();
            if !opts_any.is_none(py) {
                let opts = seq_to_vec(py, opts_any)?;
                let mut i = 0;
                while i + 1 < opts.len() {
                    let k = opts[i].bind(py);
                    if let Ok(kw) = k.cast::<Keyword>() {
                        if kw.get().name.as_ref() == "clear-actions" {
                            clear = opts[i + 1].bind(py).is_truthy()?;
                        }
                    }
                    i += 2;
                }
            }
        }
        crate::agent::restart_agent(py, agent, new_state, clear)
    })?;

    intern_fn(py, &rt_ns, "agent-set-error-handler", |args, _py| {
        need_args(args, 2, "set-error-handler!")?;
        let a0 = args.get_item(0)?;
        let a = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("set-error-handler!: first arg must be an Agent"))?;
        let v = args.get_item(1)?;
        a.get().install_error_handler(if v.is_none() { None } else { Some(v.unbind()) });
        Ok(_py.None())
    })?;

    intern_fn(py, &rt_ns, "agent-get-error-handler", |args, py| {
        need_args(args, 1, "error-handler")?;
        let a0 = args.get_item(0)?;
        let a = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("error-handler: arg must be an Agent"))?;
        Ok(a.get().read_error_handler(py))
    })?;

    intern_fn(py, &rt_ns, "agent-set-error-mode", |args, _py| {
        need_args(args, 2, "set-error-mode!")?;
        let a0 = args.get_item(0)?;
        let a = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("set-error-mode!: first arg must be an Agent"))?;
        let v = args.get_item(1)?;
        // Accept a Keyword (:fail/:continue).
        let _kw = v.cast::<Keyword>()
            .map_err(|_| IllegalArgumentException::new_err("set-error-mode!: mode must be a Keyword (:fail or :continue)"))?;
        a.get().install_error_mode(v.unbind());
        Ok(_py.None())
    })?;

    intern_fn(py, &rt_ns, "agent-get-error-mode", |args, py| {
        need_args(args, 1, "error-mode")?;
        let a0 = args.get_item(0)?;
        let a = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("error-mode: arg must be an Agent"))?;
        Ok(a.get().read_error_mode(py))
    })?;

    intern_fn(py, &rt_ns, "agent-clear-errors", |args, py| {
        need_args(args, 1, "clear-agent-errors")?;
        let a0 = args.get_item(0)?;
        let agent = a0.cast::<crate::agent::Agent>()
            .map_err(|_| IllegalArgumentException::new_err("clear-agent-errors: arg must be an Agent"))?
            .clone()
            .unbind();
        let _ = crate::agent::clear_agent_errors(py, agent);
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "agent-shutdown", |args, _py| {
        let _ = args;
        crate::agent::shutdown_agents();
        Ok(_py.None())
    })?;

    // --- Sorted collections (vanilla 400-427) ---

    intern_fn(py, &rt_ns, "sorted-map", |args, py| {
        crate::collections::ptreemap::sorted_map(py, args.clone())
            .map(|p| p.into_any())
    })?;

    intern_fn(py, &rt_ns, "sorted-map-by", |args, py| {
        if args.len() < 1 {
            return Err(IllegalArgumentException::new_err(
                "sorted-map-by requires a comparator",
            ));
        }
        let comparator = args.get_item(0)?.unbind();
        // Build the rest as a tuple.
        let rest_items: Vec<PyObject> = (1..args.len())
            .map(|i| -> PyResult<PyObject> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        let rest = PyTuple::new(py, &rest_items)?;
        crate::collections::ptreemap::sorted_map_by(py, comparator, rest)
            .map(|p| p.into_any())
    })?;

    intern_fn(py, &rt_ns, "sorted-set", |args, py| {
        crate::collections::ptreeset::sorted_set(py, args.clone())
            .map(|p| p.into_any())
    })?;

    intern_fn(py, &rt_ns, "sorted-set-by", |args, py| {
        if args.len() < 1 {
            return Err(IllegalArgumentException::new_err(
                "sorted-set-by requires a comparator",
            ));
        }
        let comparator = args.get_item(0)?.unbind();
        let rest_items: Vec<PyObject> = (1..args.len())
            .map(|i| -> PyResult<PyObject> { Ok(args.get_item(i)?.unbind()) })
            .collect::<PyResult<_>>()?;
        let rest = PyTuple::new(py, &rest_items)?;
        crate::collections::ptreeset::sorted_set_by(py, comparator, rest)
            .map(|p| p.into_any())
    })?;

    // (sorted? x) — true iff x implements the Sorted protocol.
    mk_protocol_pred(py, &rt_ns, "sorted?", get_proto(m, "Sorted")?)?;

    // (rseq coll) — dispatch through the Reversible protocol.
    intern_fn(py, &rt_ns, "rseq", |args, py| {
        need_args(args, 1, "rseq")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "Reversible", "rseq",
            args.get_item(0)?.unbind(),
        )
    })?;

    // Sorted protocol dispatch helpers — called by subseq/rsubseq and friends.
    intern_fn(py, &rt_ns, "sorted-seq", |args, py| {
        need_args(args, 2, "sorted-seq")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "Sorted", "sorted_seq",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    // Small helpers that subseq/rsubseq use.
    intern_fn(py, &rt_ns, "sorted-entry-key", |args, py| {
        need_args(args, 2, "sorted-entry-key")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_2(
            py, &PFN, "Sorted", "entry_key",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
        )
    })?;

    // (compare-values coll a b) — compare using coll's comparator, else default.
    intern_fn(py, &rt_ns, "compare-values", |args, py| {
        need_args(args, 3, "compare-values")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        let comp_obj: PyObject = crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "Sorted", "comparator_of",
            args.get_item(0)?.unbind(),
        )?;
        let a = args.get_item(1)?.unbind();
        let b = args.get_item(2)?.unbind();
        let n: i64 = if comp_obj.is_none(py) {
            crate::rt::compare(py, a, b)?
        } else {
            let r = comp_obj.bind(py).call1((a.clone_ref(py), b.clone_ref(py)))?;
            if r.cast::<pyo3::types::PyBool>().is_ok() {
                if r.is_truthy()? {
                    -1
                } else {
                    let r2 = comp_obj.bind(py).call1((b, a))?;
                    if r2.is_truthy()? { 1 } else { 0 }
                }
            } else if let Ok(i) = r.extract::<i64>() {
                i
            } else {
                return Err(IllegalArgumentException::new_err(
                    "Comparator must return int or bool",
                ));
            }
        };
        Ok((n as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    // --- Printing primitives (vanilla 3691-3826) ---
    //
    // `*out*` / `*in*` are defined at the Clojure layer (core.clj) as
    // dynamic Vars; the shims here just expose the Python-side access
    // needed to write/flush/readline through whatever object is bound.

    intern_fn(py, &rt_ns, "py-sys-stdout", |args, py| {
        let _ = args;
        Ok(py.import("sys")?.getattr("stdout")?.unbind())
    })?;

    intern_fn(py, &rt_ns, "py-sys-stdin", |args, py| {
        let _ = args;
        Ok(py.import("sys")?.getattr("stdin")?.unbind())
    })?;

    intern_fn(py, &rt_ns, "py-sys-stderr", |args, py| {
        let _ = args;
        Ok(py.import("sys")?.getattr("stderr")?.unbind())
    })?;

    // (writer-write writer s) — stringify s via __str__ if needed then
    // call writer.write. Returns nil.
    intern_fn(py, &rt_ns, "writer-write", |args, _py| {
        need_args(args, 2, "writer-write")?;
        let w = args.get_item(0)?;
        let s = args.get_item(1)?;
        w.call_method1("write", (s,))?;
        Ok(_py.None())
    })?;

    intern_fn(py, &rt_ns, "writer-flush", |args, _py| {
        need_args(args, 1, "writer-flush")?;
        let w = args.get_item(0)?;
        // Not every file-like has flush (e.g. StringIO does); best-effort.
        if w.hasattr("flush")? {
            w.call_method0("flush")?;
        }
        Ok(_py.None())
    })?;

    // (reader-readline r) — read one line. Python's readline includes the
    // trailing newline; strip it. Returns nil at EOF (empty string).
    intern_fn(py, &rt_ns, "reader-readline", |args, py| {
        need_args(args, 1, "reader-readline")?;
        let r = args.get_item(0)?;
        let line_obj = r.call_method0("readline")?;
        let line: String = line_obj.extract()?;
        if line.is_empty() {
            return Ok(py.None());
        }
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').to_string();
        Ok(pyo3::types::PyString::new(py, &trimmed).unbind().into_any())
    })?;

    // String-writer helpers used by the Clojure-layer `pr-str` implementation
    // to build a string via print-method dispatch. Backed by Python's io.StringIO.
    intern_fn(py, &rt_ns, "make-string-writer", |args, py| {
        let _ = args;
        let io_mod = py.import("io")?;
        let sio = io_mod.getattr("StringIO")?.call0()?;
        Ok(sio.unbind())
    })?;

    intern_fn(py, &rt_ns, "string-writer-value", |args, py| {
        need_args(args, 1, "string-writer-value")?;
        let w = args.get_item(0)?;
        let v = w.call_method0("getvalue")?;
        let _ = py;
        Ok(v.unbind())
    })?;

    // `print-str` — non-readable (strings un-quoted).
    intern_fn(py, &rt_ns, "print-str", |args, py| {
        need_args(args, 1, "print-str")?;
        let s = crate::printer::print::print_str(py, args.get_item(0)?.unbind())?;
        Ok(pyo3::types::PyString::new(py, &s).unbind().into_any())
    })?;

    // `pr-str` was previously only exposed as `clojure._core.pr_str`; add an
    // RT alias for the Clojure-layer `(defn pr-str ...)` form.
    intern_fn(py, &rt_ns, "pr-str", |args, py| {
        need_args(args, 1, "pr-str")?;
        let s = crate::printer::print::pr_str(py, args.get_item(0)?.unbind())?;
        Ok(pyo3::types::PyString::new(py, &s).unbind().into_any())
    })?;

    // --- I/O (vanilla 3771-3835, partial) ---

    // --- Futures / promises (vanilla 6800-7100) ---

    // --- ex-info / ex-data / ex-cause / ex-message (vanilla 5300) ---

    intern_fn(py, &rt_ns, "ex-info-impl", |args, py| {
        if args.len() < 2 || args.len() > 3 {
            return Err(IllegalArgumentException::new_err(
                "ex-info: 2 or 3 args (msg, data, cause?)",
            ));
        }
        let msg: PyObject = args.get_item(0)?.unbind();
        let data: PyObject = args.get_item(1)?.unbind();
        let cause: Option<PyObject> = if args.len() == 3 {
            let c = args.get_item(2)?;
            if c.is_none() { None } else { Some(c.unbind()) }
        } else {
            None
        };
        // Construct: ExceptionInfo(msg) then setattr data + __cause__.
        let exc_cls = py.import("clojure._core")?.getattr("ExceptionInfo")?;
        let exc = exc_cls.call1((msg,))?;
        exc.setattr("data", data)?;
        if let Some(c) = cause {
            exc.setattr("__cause__", c)?;
        }
        Ok(exc.unbind())
    })?;

    intern_fn(py, &rt_ns, "ex-data-impl", |args, py| {
        need_args(args, 1, "ex-data")?;
        let ex = args.get_item(0)?;
        match ex.getattr("data") {
            Ok(d) => Ok(d.unbind()),
            Err(_) => Ok(py.None()),
        }
    })?;

    intern_fn(py, &rt_ns, "ex-cause-impl", |args, py| {
        need_args(args, 1, "ex-cause")?;
        let ex = args.get_item(0)?;
        match ex.getattr("__cause__") {
            Ok(c) if !c.is_none() => Ok(c.unbind()),
            _ => Ok(py.None()),
        }
    })?;

    intern_fn(py, &rt_ns, "ex-message-impl", |args, py| {
        need_args(args, 1, "ex-message")?;
        let ex = args.get_item(0)?;
        // Python's `args[0]` if present, else str(ex).
        if let Ok(args_attr) = ex.getattr("args") {
            if let Ok(t) = args_attr.cast::<PyTuple>() {
                if t.len() > 0 {
                    return Ok(t.get_item(0)?.unbind());
                }
            }
        }
        Ok(ex.str()?.unbind().into_any())
    })?;

    // --- Regex (vanilla 7100) ---

    intern_fn(py, &rt_ns, "re-pattern", |args, py| {
        need_args(args, 1, "re-pattern")?;
        let arg = args.get_item(0)?;
        // If already a compiled pattern, return as-is. Test by hasattr "pattern".
        let re_mod = py.import("re")?;
        let pattern_cls = re_mod.getattr("Pattern")?;
        if arg.is_instance(&pattern_cls)? {
            return Ok(arg.unbind());
        }
        let s: String = arg.extract()?;
        Ok(re_mod.getattr("compile")?.call1((s,))?.unbind())
    })?;

    // (re-find pattern s) — first match. Returns nil if no match.
    // No groups → returns the matched string. With groups → vector of
    // [whole, group1, group2, …].
    intern_fn(py, &rt_ns, "re-find-impl", |args, py| {
        need_args(args, 2, "re-find")?;
        let pat = args.get_item(0)?;
        let s = args.get_item(1)?;
        let m = pat.call_method1("search", (s,))?;
        if m.is_none() {
            return Ok(py.None());
        }
        let groups_tuple = m.call_method0("groups")?;
        let groups: Bound<'_, PyTuple> = groups_tuple.cast_into()?;
        let whole = m.call_method1("group", (0i64,))?;
        if groups.len() == 0 {
            return Ok(whole.unbind());
        }
        let mut items: Vec<PyObject> = Vec::with_capacity(groups.len() + 1);
        items.push(whole.unbind());
        for g in groups.iter() {
            items.push(g.unbind());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, tup).map(|v| v.into_any())
    })?;

    intern_fn(py, &rt_ns, "re-matches-impl", |args, py| {
        need_args(args, 2, "re-matches")?;
        let pat = args.get_item(0)?;
        let s = args.get_item(1)?;
        let m = pat.call_method1("fullmatch", (s,))?;
        if m.is_none() {
            return Ok(py.None());
        }
        let groups_tuple = m.call_method0("groups")?;
        let groups: Bound<'_, PyTuple> = groups_tuple.cast_into()?;
        let whole = m.call_method1("group", (0i64,))?;
        if groups.len() == 0 {
            return Ok(whole.unbind());
        }
        let mut items: Vec<PyObject> = Vec::with_capacity(groups.len() + 1);
        items.push(whole.unbind());
        for g in groups.iter() {
            items.push(g.unbind());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, tup).map(|v| v.into_any())
    })?;

    intern_fn(py, &rt_ns, "re-seq-impl", |args, py| {
        need_args(args, 2, "re-seq")?;
        let pat = args.get_item(0)?;
        let s = args.get_item(1)?;
        let it = pat.call_method1("finditer", (s,))?;
        // Materialize into a Clojure list (eager — re-seq is documented as
        // lazy in vanilla but for simplicity we eagerify; user can take/drop).
        let mut items: Vec<PyObject> = Vec::new();
        let py_iter = it.try_iter()?;
        for m_res in py_iter {
            let m = m_res?;
            let groups_tuple = m.call_method0("groups")?;
            let groups: Bound<'_, PyTuple> = groups_tuple.cast_into()?;
            let whole = m.call_method1("group", (0i64,))?;
            if groups.len() == 0 {
                items.push(whole.unbind());
            } else {
                let mut sub: Vec<PyObject> = Vec::with_capacity(groups.len() + 1);
                sub.push(whole.unbind());
                for g in groups.iter() {
                    sub.push(g.unbind());
                }
                let tup = PyTuple::new(py, &sub)?;
                items.push(crate::collections::pvector::vector(py, tup)?.into_any());
            }
        }
        if items.is_empty() {
            return Ok(py.None());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // --- Parse helpers + random-uuid (vanilla 7300, Clojure 1.11+) ---

    intern_fn(py, &rt_ns, "parse-long-impl", |args, py| {
        need_args(args, 1, "parse-long")?;
        let arg = args.get_item(0)?;
        let s: String = match arg.extract() {
            Ok(v) => v,
            Err(_) => return Ok(py.None()),
        };
        let trimmed = s.trim();
        match trimmed.parse::<i64>() {
            Ok(n) => Ok((n as i64).into_pyobject(py)?.unbind().into_any()),
            Err(_) => Ok(py.None()),
        }
    })?;

    intern_fn(py, &rt_ns, "parse-double-impl", |args, py| {
        need_args(args, 1, "parse-double")?;
        let arg = args.get_item(0)?;
        let s: String = match arg.extract() {
            Ok(v) => v,
            Err(_) => return Ok(py.None()),
        };
        let trimmed = s.trim();
        match trimmed.parse::<f64>() {
            Ok(f) => Ok(f.into_pyobject(py)?.unbind().into_any()),
            Err(_) => Ok(py.None()),
        }
    })?;

    intern_fn(py, &rt_ns, "parse-boolean-impl", |args, py| {
        need_args(args, 1, "parse-boolean")?;
        let arg = args.get_item(0)?;
        let s: String = match arg.extract() {
            Ok(v) => v,
            Err(_) => return Ok(py.None()),
        };
        match s.as_str() {
            "true" => Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any()),
            "false" => Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any()),
            _ => Ok(py.None()),
        }
    })?;

    intern_fn(py, &rt_ns, "parse-uuid-impl", |args, py| {
        need_args(args, 1, "parse-uuid")?;
        let arg = args.get_item(0)?;
        let s: String = match arg.extract() {
            Ok(v) => v,
            Err(_) => return Ok(py.None()),
        };
        let uuid_mod = py.import("uuid")?;
        let uuid_cls = uuid_mod.getattr("UUID")?;
        match uuid_cls.call1((s,)) {
            Ok(u) => Ok(u.unbind()),
            Err(_) => Ok(py.None()),
        }
    })?;

    intern_fn(py, &rt_ns, "random-uuid-impl", |args, py| {
        let _ = args;
        let uuid_mod = py.import("uuid")?;
        Ok(uuid_mod.getattr("uuid4")?.call0()?.unbind())
    })?;

    // (bean-impl obj) — return a live Bean view. Property names are
    // captured at construction; values are read on each lookup so
    // mutations of obj are visible. See src/bean.rs.
    intern_fn(py, &rt_ns, "bean-impl", |args, py| {
        need_args(args, 1, "bean")?;
        let obj = args.get_item(0)?.unbind();
        let bean = crate::bean::Bean::create(py, obj)?;
        Ok(Py::new(py, bean)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "available-parallelism", |args, py| {
        let _ = args;
        let n = std::thread::available_parallelism()
            .map(|n| n.get() as i64)
            .unwrap_or(4);
        Ok((n).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "future-call", |args, py| {
        need_args(args, 1, "future-call")?;
        let f = args.get_item(0)?.unbind();
        let fut = crate::future_::Future::spawn(py, f)?;
        Ok(Py::new(py, fut)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "future?", |args, py| {
        need_args(args, 1, "future?")?;
        let x = args.get_item(0)?;
        let is = x.cast::<crate::future_::Future>().is_ok();
        Ok(pyo3::types::PyBool::new(py, is).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "future-cancel", |args, py| {
        need_args(args, 1, "future-cancel")?;
        let x = args.get_item(0)?;
        let f = x.cast::<crate::future_::Future>()
            .map_err(|_| IllegalArgumentException::new_err("future-cancel: arg must be a Future"))?;
        Ok(pyo3::types::PyBool::new(py, f.get().try_cancel()).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "future-cancelled?", |args, py| {
        need_args(args, 1, "future-cancelled?")?;
        let x = args.get_item(0)?;
        let f = x.cast::<crate::future_::Future>()
            .map_err(|_| IllegalArgumentException::new_err("future-cancelled?: arg must be a Future"))?;
        Ok(pyo3::types::PyBool::new(py, f.get().is_cancelled()).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "future-done?", |args, py| {
        need_args(args, 1, "future-done?")?;
        let x = args.get_item(0)?;
        let f = x.cast::<crate::future_::Future>()
            .map_err(|_| IllegalArgumentException::new_err("future-done?: arg must be a Future"))?;
        Ok(pyo3::types::PyBool::new(py, f.get().is_done()).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "promise", |args, py| {
        let _ = args;
        let p = crate::future_::Promise::create();
        Ok(Py::new(py, p)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "deliver", |args, py| {
        need_args(args, 2, "deliver")?;
        let p_any = args.get_item(0)?;
        let p = p_any.cast::<crate::future_::Promise>()
            .map_err(|_| IllegalArgumentException::new_err("deliver: first arg must be a Promise"))?
            .clone()
            .unbind();
        let v = args.get_item(1)?.unbind();
        Ok(crate::future_::Promise::try_deliver(p, py, v))
    })?;

    // (realized? x) — dispatch through IPending. Returns false for any
    // value that doesn't implement IPending (the protocol's :default).
    intern_fn(py, &rt_ns, "realized?", |args, py| {
        need_args(args, 1, "realized?")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        match crate::protocol_fn::dispatch_cached_1(
            py, &PFN, "IPending", "is_realized",
            args.get_item(0)?.unbind(),
        ) {
            Ok(v) => Ok(v),
            // Non-IPending → false (matches vanilla, which only defines
            // realized? for IPending types).
            Err(_) => Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any()),
        }
    })?;

    // (close-resource x) — best-effort cleanup. Tries Python context-
    // manager `__exit__(None, None, None)` first, then falls back to
    // `.close()`. No-op if neither is defined.
    intern_fn(py, &rt_ns, "close-resource", |args, _py| {
        need_args(args, 1, "close-resource")?;
        let x = args.get_item(0)?;
        if x.hasattr("__exit__")? {
            x.call_method1("__exit__", (_py.None(), _py.None(), _py.None()))?;
        } else if x.hasattr("close")? {
            x.call_method0("close")?;
        }
        Ok(_py.None())
    })?;

    // (extenders proto) — list of types that have direct impls registered.
    // Excludes promoted MRO copies so the result reflects what users
    // actually extended.
    intern_fn(py, &rt_ns, "extenders", |args, py| {
        need_args(args, 1, "extenders")?;
        let p_any = args.get_item(0)?;
        let proto = p_any.cast::<crate::protocol::Protocol>()
            .map_err(|_| IllegalArgumentException::new_err("extenders: arg must be a Protocol"))?;
        let mut types: Vec<PyObject> = Vec::new();
        for entry in proto.get().cache.entries.iter() {
            let table = entry.value();
            // Filter out promoted (MRO-cached) entries; only direct extends.
            if !table.promoted {
                let key_ptr = entry.key().0;
                // CacheKey holds a PyType pointer. Reconstruct as Bound.
                let ty_obj: Py<PyAny> = unsafe {
                    Py::from_borrowed_ptr(py, key_ptr as *mut pyo3::ffi::PyObject)
                };
                types.push(ty_obj);
            }
        }
        let tup = PyTuple::new(py, &types)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // (extends? proto x) — true iff x's type or an MRO ancestor has an
    // impl registered. (Same shape as `satisfies?`; vanilla distinguishes
    // them — `extends?` accepts a class, `satisfies?` accepts a value.)
    intern_fn(py, &rt_ns, "extends?", |args, py| {
        need_args(args, 2, "extends?")?;
        let p_any = args.get_item(0)?;
        let proto = p_any.cast::<crate::protocol::Protocol>()
            .map_err(|_| IllegalArgumentException::new_err("extends?: first arg must be a Protocol"))?;
        let x = args.get_item(1)?;
        // If x is itself a class, use it directly; else use its type.
        let ty = if let Ok(t) = x.cast::<pyo3::types::PyType>() {
            t.clone()
        } else {
            x.get_type()
        };
        let exact_key = crate::protocol::CacheKey::for_py_type(&ty);
        if proto.get().cache.lookup(exact_key).is_some() {
            return Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any());
        }
        let mro = ty.getattr("__mro__")?;
        let mro_tuple: Bound<'_, PyTuple> = mro.cast_into()?;
        for parent in mro_tuple.iter().skip(1) {
            let pt: Bound<'_, pyo3::types::PyType> = parent.cast_into()?;
            let pk = crate::protocol::CacheKey::for_py_type(&pt);
            if proto.get().cache.lookup(pk).is_some() {
                return Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any());
            }
        }
        Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any())
    })?;

    // (current-load-ns) — return the namespace currently being loaded
    // into, or `clojure.user` if no load is active. `require` / `use`
    // use this to install aliases / refers in the right place.
    intern_fn(py, &rt_ns, "current-load-ns", |args, py| {
        let _ = args;
        let cur = crate::eval::load::CURRENT_LOAD_NS.with(|c| {
            c.borrow().as_ref().map(|n| n.clone_ref(py))
        });
        match cur {
            Some(n) => Ok(n),
            None => {
                // Fall back to clojure.user (creating if missing).
                let sys = py.import("sys")?;
                let modules = sys.getattr("modules")?;
                match modules.get_item("clojure.user") {
                    Ok(n) => Ok(n.unbind()),
                    Err(_) => {
                        let sym = Symbol::new(None, Arc::from("clojure.user"));
                        let sym_py = Py::new(py, sym)?;
                        crate::namespace::create_ns(py, sym_py)
                    }
                }
            }
        }
    })?;

    // (in-ns sym) — find-or-create the named namespace and signal the
    // running `load_clj_string` to switch its target ns to it for
    // subsequent forms. Updates both LOAD_NS_OVERRIDE (for the
    // between-forms switch) and CURRENT_LOAD_NS (so subsequent code in
    // THIS form — e.g. require / use inside the `ns` macro body — sees
    // the new namespace immediately). Returns the namespace.
    intern_fn(py, &rt_ns, "in-ns", |args, py| {
        need_args(args, 1, "in-ns")?;
        let a0 = args.get_item(0)?;
        let sym = a0.cast::<Symbol>()
            .map_err(|_| IllegalArgumentException::new_err("in-ns: arg must be a Symbol"))?
            .clone()
            .unbind();
        let ns = match crate::namespace::find_ns(py, sym.clone_ref(py)) {
            Ok(Some(n)) => n,
            _ => crate::namespace::create_ns(py, sym.clone_ref(py))?,
        };
        crate::eval::load::LOAD_NS_OVERRIDE.with(|c| {
            *c.borrow_mut() = Some(ns.clone_ref(py));
        });
        crate::eval::load::CURRENT_LOAD_NS.with(|c| {
            *c.borrow_mut() = Some(ns.clone_ref(py));
        });
        // Update `*ns*` so subsequent code in this form (e.g. refer / use /
        // require inside the ns macro body) and the reader's ::kw auto-
        // resolution see the new namespace immediately. Prefer the thread-
        // binding path (active during load); fall back to bind_root for the
        // REPL case (no binding frame).
        if let Ok(ns_var) = crate::eval::load::ns_var(py) {
            let ns_var_py: PyObject = ns_var.clone_ref(py).into_any();
            if crate::binding::set_binding(py, &ns_var_py, ns.clone_ref(py)).is_err() {
                let _ = ns_var.bind(py).call_method1("bind_root", (ns.clone_ref(py),));
            }
        }
        Ok(ns)
    })?;

    // (read-file-text path) — read entire file as string.
    intern_fn(py, &rt_ns, "read-file-text", |args, py| {
        need_args(args, 1, "read-file-text")?;
        let path: String = args.get_item(0)?.extract()?;
        let contents = std::fs::read_to_string(&path).map_err(|e| {
            IllegalArgumentException::new_err(format!(
                "read-file-text: {}: {}", path, e
            ))
        })?;
        Ok(pyo3::types::PyString::new(py, &contents).unbind().into_any())
    })?;

    // (find-source-file ns-sym) — search sys.path for `ns/sym/path.clj`,
    // converting dots to slashes. Returns the absolute path or nil.
    intern_fn(py, &rt_ns, "find-source-file", |args, py| {
        need_args(args, 1, "find-source-file")?;
        let a0 = args.get_item(0)?;
        let sym = a0.cast::<Symbol>()
            .map_err(|_| IllegalArgumentException::new_err("find-source-file: arg must be a Symbol"))?;
        match crate::eval::load::find_source_file_path(py, sym.get())? {
            Some(s) => Ok(pyo3::types::PyString::new(py, &s).unbind().into_any()),
            None => Ok(py.None()),
        }
    })?;

    // (load-string s) — read+eval every top-level form in s against clojure.user.
    intern_fn(py, &rt_ns, "load-string", |args, py| {
        need_args(args, 1, "load-string")?;
        let src: String = args.get_item(0)?.extract()?;
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let ns = match modules.get_item("clojure.user") {
            Ok(n) => n.unbind(),
            Err(_) => {
                let sym = Symbol::new(None, Arc::from("clojure.user"));
                let sym_py = Py::new(py, sym)?;
                crate::namespace::create_ns(py, sym_py)?
            }
        };
        crate::eval::load::load_clj_string(py, &src, &ns)?;
        Ok(py.None())
    })?;

    // --- NS introspection (vanilla 4146-4311) ---

    // (all-ns) — list of all ClojureNamespaces currently loaded.
    intern_fn(py, &rt_ns, "all-ns", |args, py| {
        let _ = args;
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let d = modules.cast::<pyo3::types::PyDict>()?;
        let mut out: Vec<PyObject> = Vec::new();
        for (_k, v) in d.iter() {
            if crate::namespace::is_clojure_namespace(py, &v)? {
                out.push(v.unbind());
            }
        }
        let tup = PyTuple::new(py, &out)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // (remove-ns sym) — delete the namespace from sys.modules. Returns nil.
    intern_fn(py, &rt_ns, "remove-ns", |args, py| {
        need_args(args, 1, "remove-ns")?;
        let name = {
            let a0 = args.get_item(0)?;
            if let Ok(sym) = a0.cast::<Symbol>() {
                sym.get().name.to_string()
            } else {
                a0.extract::<String>()?
            }
        };
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        // `.pop(name, None)` via PyDict api.
        let d = modules.cast::<pyo3::types::PyDict>()?;
        let key = pyo3::types::PyString::new(py, &name);
        let _ = d.del_item(key);
        Ok(py.None())
    })?;

    // (ns-unmap ns sym) — remove the Var mapping (without removing from
    // __clj_refers__ — that's a separate concept).
    intern_fn(py, &rt_ns, "ns-unmap", |args, _py| {
        need_args(args, 2, "ns-unmap")?;
        let ns = args.get_item(0)?;
        let sym = args.get_item(1)?;
        let name = {
            if let Ok(s) = sym.cast::<Symbol>() {
                s.get().name.to_string()
            } else {
                sym.extract::<String>()?
            }
        };
        // Best-effort delattr; don't fail if absent.
        let _ = ns.delattr(name.as_str());
        // Also scrub from refers map.
        if let Ok(refers) = ns.getattr("__clj_refers__") {
            if let Ok(d) = refers.cast::<pyo3::types::PyDict>() {
                let _ = d.del_item(sym);
            }
        }
        Ok(_py.None())
    })?;

    // (ns-unalias ns sym) — remove alias. Returns nil.
    intern_fn(py, &rt_ns, "ns-unalias", |args, _py| {
        need_args(args, 2, "ns-unalias")?;
        let ns = args.get_item(0)?;
        let sym = args.get_item(1)?;
        if let Ok(aliases) = ns.getattr("__clj_aliases__") {
            if let Ok(d) = aliases.cast::<pyo3::types::PyDict>() {
                let _ = d.del_item(sym);
            }
        }
        Ok(_py.None())
    })?;

    // (var-create) — anonymous, dynamic Var. Used by `with-local-vars`.
    intern_fn(py, &rt_ns, "var-create", |args, py| {
        need_args(args, 0, "var-create")?;
        let _ = args;
        let v = crate::var::create_var(py)?;
        Ok(Py::new(py, v)?.into_any())
    })?;

    // (var-set-bang var val) — mutate the current binding frame's entry for var.
    intern_fn(py, &rt_ns, "var-set-bang", |args, _py| {
        need_args(args, 2, "var-set")?;
        let v_any = args.get_item(0)?;
        let var = v_any.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("var-set: first arg must be a Var")
        })?;
        let val = args.get_item(1)?.unbind();
        var.call_method1("set_bang", (val,))?;
        Ok(args.get_item(1)?.unbind())
    })?;

    // (clone-thread-binding-frame) / (reset-thread-binding-frame frame)
    // — full-stack snapshot + install, used by `binding-conveyor-fn`.
    intern_fn(py, &rt_ns, "clone-thread-binding-frame", |args, py| {
        need_args(args, 0, "clone-thread-binding-frame")?;
        let _ = args;
        let frame = crate::binding::clone_thread_binding_frame(py);
        Ok(Py::new(py, frame)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "reset-thread-binding-frame", |args, py| {
        need_args(args, 1, "reset-thread-binding-frame")?;
        let f_any = args.get_item(0)?;
        let frame = f_any.cast::<crate::binding::BindingFrame>().map_err(|_| {
            IllegalArgumentException::new_err(
                "reset-thread-binding-frame: arg must be a BindingFrame",
            )
        })?.clone().unbind();
        crate::binding::reset_thread_binding_frame(py, frame)?;
        Ok(py.None())
    })?;

    // (throw-assert msg) — used by the `assert` macro.
    intern_fn(py, &rt_ns, "throw-assert", |args, _py| {
        let msg = if args.len() == 0 {
            String::from("Assert failed")
        } else {
            args.get_item(0)?.str()?.to_string_lossy().into_owned()
        };
        Err(crate::exceptions::AssertionError::new_err(msg))
    })?;

    // (intern ns sym) — create or fetch a Var in `ns`.
    intern_fn(py, &rt_ns, "intern", |args, py| {
        need_args(args, 2, "intern")?;
        let ns = args.get_item(0)?.unbind();
        let sym_any = args.get_item(1)?;
        let sym = sym_any.cast::<Symbol>().map_err(|_| {
            IllegalArgumentException::new_err("intern: second arg must be a Symbol")
        })?.clone().unbind();
        let var = crate::ns_ops::intern(py, ns, sym)?;
        Ok(var.into_any())
    })?;

    // (bind-root var value) — install a new root value on a Var.
    intern_fn(py, &rt_ns, "bind-root", |args, _py| {
        need_args(args, 2, "bind-root")?;
        let v_any = args.get_item(0)?;
        let var = v_any.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("bind-root: first arg must be a Var")
        })?;
        let val = args.get_item(1)?.unbind();
        var.call_method1("bind_root", (val,))?;
        Ok(_py.None())
    })?;

    // (get-root var) — return the current ROOT value of a Var, bypassing
    // any thread-local `binding`. Matches vanilla's `.getRawRoot`. Used by
    // `with-redefs-fn` so it snapshots the root (not a bound override) for
    // restoration.
    intern_fn(py, &rt_ns, "get-root", |args, py| {
        need_args(args, 1, "get-root")?;
        let v_any = args.get_item(0)?;
        let var = v_any.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("get-root: arg must be a Var")
        })?;
        // Var.root is an `ArcSwap<Option<PyObject>>`; load, deref, clone.
        let slot = var.get().root.load();
        match (**slot).as_ref() {
            Some(v) => Ok(v.clone_ref(py)),
            None => Ok(py.None()),
        }
    })?;

    // (ns-map ns) / (ns-aliases ns) / (ns-refers ns) — dispatch to the
    // pyfunctions in `ns_ops`, wrap the raw PyDict in a PersistentArrayMap
    // so the result is seqable and behaves as a Clojure map.
    fn dict_to_map(py: Python<'_>, d: Py<pyo3::types::PyDict>) -> PyResult<PyObject> {
        let mut flat: Vec<PyObject> = Vec::with_capacity(d.bind(py).len() * 2);
        for (k, v) in d.bind(py).iter() {
            flat.push(k.unbind());
            flat.push(v.unbind());
        }
        let tup = pyo3::types::PyTuple::new(py, &flat)?;
        crate::collections::parraymap::array_map(py, tup)
    }

    intern_fn(py, &rt_ns, "ns-map", |args, py| {
        need_args(args, 1, "ns-map")?;
        let d = crate::ns_ops::ns_map(py, args.get_item(0)?.unbind())?;
        dict_to_map(py, d)
    })?;

    intern_fn(py, &rt_ns, "ns-aliases", |args, py| {
        need_args(args, 1, "ns-aliases")?;
        let obj = crate::ns_ops::ns_aliases(py, args.get_item(0)?.unbind())?;
        let d = obj.bind(py).clone().downcast_into::<pyo3::types::PyDict>()
            .map_err(|_| IllegalArgumentException::new_err("ns-aliases: expected a dict"))?
            .unbind();
        dict_to_map(py, d)
    })?;

    intern_fn(py, &rt_ns, "ns-refers", |args, py| {
        need_args(args, 1, "ns-refers")?;
        let obj = crate::ns_ops::ns_refers(py, args.get_item(0)?.unbind())?;
        let d = obj.bind(py).clone().downcast_into::<pyo3::types::PyDict>()
            .map_err(|_| IllegalArgumentException::new_err("ns-refers: expected a dict"))?
            .unbind();
        dict_to_map(py, d)
    })?;

    intern_fn(py, &rt_ns, "refer", |args, py| {
        need_args(args, 3, "refer")?;
        let ns = args.get_item(0)?.unbind();
        let sym_any = args.get_item(1)?;
        let sym = sym_any.cast::<Symbol>().map_err(|_| {
            IllegalArgumentException::new_err("refer: second arg must be a Symbol")
        })?.clone().unbind();
        let var_any = args.get_item(2)?;
        let var = var_any.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("refer: third arg must be a Var")
        })?.clone().unbind();
        crate::ns_ops::refer(py, ns, sym, var)?;
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "alias", |args, py| {
        need_args(args, 3, "alias")?;
        let ns = args.get_item(0)?.unbind();
        let sym_any = args.get_item(1)?;
        let sym = sym_any.cast::<Symbol>().map_err(|_| {
            IllegalArgumentException::new_err("alias: second arg must be a Symbol")
        })?.clone().unbind();
        let target = args.get_item(2)?.unbind();
        crate::ns_ops::alias(py, ns, sym, target)?;
        Ok(py.None())
    })?;

    // (ns-publics ns) / (ns-interns ns) — thin wrappers around ns-map that
    // filter by Var.meta (:private) and Var.ns-identity. Returns a dict
    // (will be wrapped as a PersistentArrayMap at the Clojure layer).
    intern_fn(py, &rt_ns, "ns-publics", |args, py| {
        need_args(args, 1, "ns-publics")?;
        let ns = args.get_item(0)?;
        let all = crate::ns_ops::ns_map(py, ns.unbind())?;
        let d = PyDict::new(py);
        for (k, v) in all.bind(py).iter() {
            if let Ok(_var) = v.cast::<crate::var::Var>() {
                let meta_any = v.getattr("meta")?;
                let is_private = if meta_any.is_none() {
                    false
                } else {
                    let kw = crate::keyword::keyword(py, "private", None)?;
                    let kw_obj: PyObject = kw.into_any();
                    // `val_at` on PersistentArrayMap/HashMap pymethods takes
                    // just the key (returns nil if absent); `val_at_default`
                    // takes (key, default). Use the 1-arg form and is_truthy.
                    match meta_any.call_method1("val_at", (kw_obj,)) {
                        Ok(r) => r.is_truthy().unwrap_or(false),
                        Err(_) => false,
                    }
                };
                if !is_private {
                    d.set_item(k, v)?;
                }
            }
        }
        dict_to_map(py, d.unbind())
    })?;

    intern_fn(py, &rt_ns, "ns-interns", |args, py| {
        need_args(args, 1, "ns-interns")?;
        let ns = args.get_item(0)?;
        let ns_obj: PyObject = ns.clone().unbind();
        let all = crate::ns_ops::ns_map(py, ns_obj.clone_ref(py))?;
        let d = PyDict::new(py);
        for (k, v) in all.bind(py).iter() {
            if let Ok(var) = v.cast::<crate::var::Var>() {
                // Keep only Vars whose owning namespace is `ns`.
                let var_ns = var.get().ns.clone_ref(py);
                let same = crate::rt::identical(py, var_ns, ns_obj.clone_ref(py));
                if same {
                    d.set_item(k, v)?;
                }
            }
        }
        dict_to_map(py, d.unbind())
    })?;

    // --- Arrays (vanilla 3928-4048) ---
    //
    // Python has no first-class "array of primitives" analogue to Java's
    // typed arrays. The array story here is: a `list` is the runtime
    // representation. `make-array` returns `[None] * len` (or nested for
    // multi-dim). `aset` mutates. Typed variants (`aset-int` / `aset-long`
    // etc.) all alias to `aset`; Python's int/float are already unbounded.
    // `aset-char` coerces to a 1-char string; `aset-boolean` to Python bool.

    intern_fn(py, &rt_ns, "array-make", |args, py| {
        // (make-array type & dims) or (make-array & dims) — vanilla requires a
        // Class arg; we accept it for API compat and ignore. If the first arg
        // is already an int, treat all args as dims.
        if args.len() == 0 {
            return Err(IllegalArgumentException::new_err(
                "make-array requires at least one dimension",
            ));
        }
        let first = args.get_item(0)?;
        let dims_start = if first.extract::<i64>().is_ok() { 0 } else { 1 };
        if args.len() <= dims_start {
            return Err(IllegalArgumentException::new_err(
                "make-array requires at least one dimension",
            ));
        }
        let dims: Vec<i64> = (dims_start..args.len())
            .map(|i| -> PyResult<i64> { args.get_item(i)?.extract::<i64>() })
            .collect::<PyResult<_>>()?;
        fn build(py: Python<'_>, dims: &[i64]) -> PyResult<PyObject> {
            if dims.is_empty() {
                return Ok(py.None());
            }
            let n = dims[0].max(0) as usize;
            let lst = pyo3::types::PyList::empty(py);
            if dims.len() == 1 {
                for _ in 0..n {
                    lst.append(py.None())?;
                }
            } else {
                let rest = &dims[1..];
                for _ in 0..n {
                    lst.append(build(py, rest)?)?;
                }
            }
            Ok(lst.unbind().into_any())
        }
        build(py, &dims)
    })?;

    intern_fn(py, &rt_ns, "array-alength", |args, py| {
        need_args(args, 1, "alength")?;
        let a = args.get_item(0)?;
        let n = a.len()?;
        Ok((n as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "array-aclone", |args, py| {
        need_args(args, 1, "aclone")?;
        let a = args.get_item(0)?;
        // Shallow clone via `list(a)` — works for list, tuple, or any iterable.
        let builtins = py.import("builtins")?;
        let list_ty = builtins.getattr("list")?;
        Ok(list_ty.call1((a,))?.unbind())
    })?;

    intern_fn(py, &rt_ns, "array-aget", |args, py| {
        // (aget array idx & more-idxs) — multi-dim access.
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "aget requires at least an array and an index",
            ));
        }
        let mut cur: PyObject = args.get_item(0)?.unbind();
        for i in 1..args.len() {
            let idx: isize = args.get_item(i)?.extract()?;
            cur = cur.bind(py).get_item(idx)?.unbind();
        }
        let _ = py;
        Ok(cur)
    })?;

    intern_fn(py, &rt_ns, "array-aset", |args, py| {
        // (aset array idx val) or (aset array i1 i2 ... val) — walks all but
        // the last two args as indices, last is value.
        if args.len() < 3 {
            return Err(IllegalArgumentException::new_err(
                "aset requires array, index, value",
            ));
        }
        let mut cur = args.get_item(0)?;
        let last_idx = args.len() - 2;
        for i in 1..last_idx {
            let idx: isize = args.get_item(i)?.extract()?;
            cur = cur.get_item(idx)?;
        }
        let idx: isize = args.get_item(last_idx)?.extract()?;
        let v = args.get_item(args.len() - 1)?;
        cur.set_item(idx, v.clone())?;
        let _ = py;
        Ok(v.unbind())
    })?;

    // to-array / into-array: Clojure seq → Python list.
    intern_fn(py, &rt_ns, "array-to-array", |args, py| {
        need_args(args, 1, "to-array")?;
        let coll = args.get_item(0)?.unbind();
        let items = seq_to_vec(py, coll)?;
        let lst = pyo3::types::PyList::empty(py);
        for it in items {
            lst.append(it)?;
        }
        Ok(lst.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "array-into-array", |args, py| {
        // (into-array aseq) or (into-array type aseq) — type ignored.
        let coll = if args.len() == 1 {
            args.get_item(0)?.unbind()
        } else if args.len() == 2 {
            args.get_item(1)?.unbind()
        } else {
            return Err(IllegalArgumentException::new_err(
                "into-array: 1 or 2 args (seq, or type+seq)",
            ));
        };
        let items = seq_to_vec(py, coll)?;
        let lst = pyo3::types::PyList::empty(py);
        for it in items {
            lst.append(it)?;
        }
        Ok(lst.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "array-to-array-2d", |args, py| {
        need_args(args, 1, "to-array-2d")?;
        let outer = args.get_item(0)?.unbind();
        let outer_items = seq_to_vec(py, outer)?;
        let lst = pyo3::types::PyList::empty(py);
        for inner in outer_items {
            let inner_items = seq_to_vec(py, inner)?;
            let inner_lst = pyo3::types::PyList::empty(py);
            for it in inner_items {
                inner_lst.append(it)?;
            }
            lst.append(inner_lst)?;
        }
        Ok(lst.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "sorted-seq-from", |args, py| {
        need_args(args, 3, "sorted-seq-from")?;
        static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
        crate::protocol_fn::dispatch_cached_3(
            py, &PFN, "Sorted", "sorted_seq_from",
            args.get_item(0)?.unbind(),
            args.get_item(1)?.unbind(),
            args.get_item(2)?.unbind(),
        )
    })?;

    // --- Watches & validators on IRef-ish types (Atom / Ref / Var) ---

    // (add-watch ref key f) — register a watch callable under key.
    intern_fn(py, &rt_ns, "add-watch", |args, py| {
        need_args(args, 3, "add-watch")?;
        let r = args.get_item(0)?;
        let k = args.get_item(1)?.unbind();
        let f = args.get_item(2)?.unbind();
        r.call_method1("add_watch", (k, f))?;
        Ok(r.unbind())
    })?;

    // (remove-watch ref key)
    intern_fn(py, &rt_ns, "remove-watch", |args, py| {
        need_args(args, 2, "remove-watch")?;
        let r = args.get_item(0)?;
        let k = args.get_item(1)?.unbind();
        r.call_method1("remove_watch", (k,))?;
        Ok(r.unbind())
    })?;

    // (set-validator! ref f)
    intern_fn(py, &rt_ns, "set-validator-bang", |args, py| {
        need_args(args, 2, "set-validator!")?;
        let r = args.get_item(0)?;
        let f = args.get_item(1)?.unbind();
        r.call_method1("set_validator", (f,))?;
        let _ = py;
        Ok(py.None())
    })?;

    // (get-validator ref)
    intern_fn(py, &rt_ns, "get-validator", |args, py| {
        need_args(args, 1, "get-validator")?;
        let r = args.get_item(0)?;
        let v = r.call_method0("get_validator")?;
        Ok(v.unbind())
    })?;

    // (alter-meta! ref f & args) — replace ref's meta with (apply f old-meta args).
    // Match vanilla: restricted to IReference types. We accept Atom/Var/Namespace.
    intern_fn(py, &rt_ns, "alter-meta-bang", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "alter-meta! requires at least 2 args"
            ));
        }
        let r = args.get_item(0)?;
        let f = args.get_item(1)?.unbind();
        // Get current meta.
        let old_meta = get_reference_meta(py, &r)?;
        // Build (f old-meta ...rest-args).
        let mut call_args: Vec<PyObject> = Vec::with_capacity(args.len() - 1);
        call_args.push(old_meta);
        for i in 2..args.len() {
            call_args.push(args.get_item(i)?.unbind());
        }
        let new_meta = crate::rt::invoke_n(py, f, &call_args)?;
        set_reference_meta(py, &r, new_meta.clone_ref(py))?;
        Ok(new_meta)
    })?;

    // (reset-meta! ref m) — install m as ref's meta; return m.
    intern_fn(py, &rt_ns, "reset-meta-bang", |args, py| {
        need_args(args, 2, "reset-meta!")?;
        let r = args.get_item(0)?;
        let m = args.get_item(1)?.unbind();
        set_reference_meta(py, &r, m.clone_ref(py))?;
        Ok(m)
    })?;

    // --- Reader / compiler hooks ---

    // (read-string s) — parse one Clojure form from the string.
    intern_fn(py, &rt_ns, "read-string", |args, py| {
        need_args(args, 1, "read-string")?;
        let s_obj = args.get_item(0)?;
        let s: &str = s_obj.extract()?;
        crate::reader::read_string_py(py, s)
    })?;

    // --- Protocol / record / type runtime support (vanilla ~5050-5850) ---

    // (protocol-new ns name method-keys via-metadata?)
    intern_fn(py, &rt_ns, "protocol-new", |args, py| {
        if args.len() < 3 || args.len() > 4 {
            return Err(IllegalArgumentException::new_err(
                "protocol-new: 3 or 4 args (ns, name, method-keys, via-metadata?)",
            ));
        }
        let ns: String = args.get_item(0)?.extract()?;
        let name: String = args.get_item(1)?.extract()?;
        let keys_seq = args.get_item(2)?.unbind();
        let keys: Vec<String> = seq_to_vec(py, keys_seq)?
            .into_iter()
            .map(|p| p.extract::<String>(py))
            .collect::<PyResult<_>>()?;
        let via_metadata = if args.len() == 4 {
            args.get_item(3)?.is_truthy()?
        } else {
            false
        };
        Ok(crate::protocol::create_protocol(py, ns, name, keys, via_metadata)?.into_any())
    })?;

    // (protocol-method-new protocol method-key)
    intern_fn(py, &rt_ns, "protocol-method-new", |args, py| {
        need_args(args, 2, "protocol-method-new")?;
        let p_any = args.get_item(0)?;
        let proto = p_any.cast::<crate::protocol::Protocol>()
            .map_err(|_| IllegalArgumentException::new_err("protocol-method-new: first arg must be a Protocol"))?
            .clone().unbind();
        let key: String = args.get_item(1)?.extract()?;
        Ok(crate::protocol::create_protocol_method(py, proto, key)?.into_any())
    })?;

    // (protocol-extend-type protocol target-class impls-map)
    intern_fn(py, &rt_ns, "protocol-extend-type", |args, py| {
        need_args(args, 3, "protocol-extend-type")?;
        let p_any = args.get_item(0)?;
        let proto = p_any.cast::<crate::protocol::Protocol>()
            .map_err(|_| IllegalArgumentException::new_err("protocol-extend-type: first arg must be a Protocol"))?;
        let ty_any = args.get_item(1)?;
        let ty = ty_any.cast::<pyo3::types::PyType>()
            .map_err(|_| IllegalArgumentException::new_err("protocol-extend-type: second arg must be a type/class"))?;
        // Impls: accept any associative (Clojure map or PyDict) of {name -> fn}.
        let impls_any = args.get_item(2)?;
        let d = PyDict::new(py);
        if let Ok(pd) = impls_any.downcast::<PyDict>() {
            for (k, v) in pd.iter() { d.set_item(k, v)?; }
        } else {
            let mut cur = crate::rt::seq(py, impls_any.unbind())?;
            while !cur.is_none(py) {
                let e = crate::rt::first(py, cur.clone_ref(py))?;
                let eb = e.bind(py);
                d.set_item(eb.get_item(0)?, eb.get_item(1)?)?;
                cur = crate::rt::next_(py, cur)?;
            }
        }
        // Populate the legacy `Protocol` MethodCache so the lazy mirror in
        // `ProtocolFn::dispatch_fallback_miss` can find Python-side impls
        // on first dispatch against a new type.
        proto.get().extend_type(py, ty.clone(), d.clone())?;
        // New path: for each (method, impl_fn), stash the impl in the
        // corresponding ProtocolFn's `generic` slot. dispatch_on_fns
        // calls it via CPython call1(target, *args) when typed slots
        // are absent.
        let proto_name: String = proto.get().name.bind(py).get().name.to_string();
        for (k, v) in d.iter() {
            let method_key: String = k.extract()?;
            if let Some(pfn) = crate::protocol_fn::get_protocol_fn(py, &proto_name, &method_key) {
                let mut fns = crate::protocol_fn::InvokeFns::empty();
                fns.generic = Some(v.unbind());
                pfn.bind(py).get().extend_with_native(ty.clone(), fns);
            }
        }
        Ok(py.None())
    })?;

    // (make-type name) — create a fresh Python class (subclass of object)
    // with no fields. Returns the class object. Callers use
    // `clojure.lang.RT/setattr` on each instance to set fields after
    // construction. Used by `deftype` / `defrecord` / `reify`.
    intern_fn(py, &rt_ns, "make-type", |args, py| {
        need_args(args, 1, "make-type")?;
        let name: String = args.get_item(0)?.extract()?;
        let builtins = py.import("builtins")?;
        let type_fn = builtins.getattr("type")?;
        let object_cls = builtins.getattr("object")?;
        let bases = PyTuple::new(py, &[object_cls])?;
        let attrs = PyDict::new(py);
        let cls = type_fn.call1((name, bases, attrs))?;
        Ok(cls.unbind())
    })?;

    // (satisfies? protocol x) — does x's type (or an MRO ancestor) have an impl?
    intern_fn(py, &rt_ns, "satisfies?", |args, py| {
        need_args(args, 2, "satisfies?")?;
        let p_any = args.get_item(0)?;
        let proto = p_any.cast::<crate::protocol::Protocol>()
            .map_err(|_| IllegalArgumentException::new_err("satisfies?: first arg must be a Protocol"))?;
        let target = args.get_item(1)?;
        let ty = target.get_type();
        let exact_key = crate::protocol::CacheKey::for_py_type(&ty);
        if proto.get().cache.lookup(exact_key).is_some() {
            return Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any());
        }
        let mro = ty.getattr("__mro__")?;
        let mro_tuple: Bound<'_, PyTuple> = mro.cast_into()?;
        for parent in mro_tuple.iter().skip(1) {
            let pt: Bound<'_, PyType> = parent.cast_into()?;
            let pk = crate::protocol::CacheKey::for_py_type(&pt);
            if proto.get().cache.lookup(pk).is_some() {
                return Ok(pyo3::types::PyBool::new(py, true).to_owned().unbind().into_any());
            }
        }
        Ok(pyo3::types::PyBool::new(py, false).to_owned().unbind().into_any())
    })?;

    // (subs s start [end]) — Python string slicing.
    intern_fn(py, &rt_ns, "subs", |args, py| {
        let s: String = args.get_item(0)?.extract()?;
        let start: isize = args.get_item(1)?.extract()?;
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len() as isize;
        let start = if start < 0 { (start + len).max(0) as usize } else { start.min(len) as usize };
        let end = if args.len() >= 3 {
            let e: isize = args.get_item(2)?.extract()?;
            if e < 0 { ((e + len).max(0) as usize).min(chars.len()) }
            else { (e as usize).min(chars.len()) }
        } else {
            chars.len()
        };
        let out: String = if start >= end { String::new() } else { chars[start..end].iter().collect() };
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    // (str-contains? haystack needle)
    intern_fn(py, &rt_ns, "str-contains?", |args, py| {
        need_args(args, 2, "str-contains?")?;
        let haystack: String = args.get_item(0)?.extract()?;
        let needle: String = args.get_item(1)?.extract()?;
        Ok(pyo3::types::PyBool::new(py, haystack.contains(needle.as_str()))
            .to_owned().unbind().into_any())
    })?;

    // --- clojure.string helpers (str-reverse, str-upper, etc.) ---

    intern_fn(py, &rt_ns, "str-reverse", |args, py| {
        need_args(args, 1, "str-reverse")?;
        let s: String = args.get_item(0)?.extract()?;
        let out: String = s.chars().rev().collect();
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-upper", |args, py| {
        need_args(args, 1, "str-upper")?;
        let s: String = args.get_item(0)?.extract()?;
        Ok(pyo3::types::PyString::new(py, &s.to_uppercase()).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-lower", |args, py| {
        need_args(args, 1, "str-lower")?;
        let s: String = args.get_item(0)?.extract()?;
        Ok(pyo3::types::PyString::new(py, &s.to_lowercase()).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-capitalize", |args, py| {
        need_args(args, 1, "str-capitalize")?;
        let s: String = args.get_item(0)?.extract()?;
        let mut chars = s.chars();
        let out = match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            None => String::new(),
        };
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-trim", |args, py| {
        need_args(args, 1, "str-trim")?;
        let s: String = args.get_item(0)?.extract()?;
        Ok(pyo3::types::PyString::new(py, s.trim()).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-triml", |args, py| {
        need_args(args, 1, "str-triml")?;
        let s: String = args.get_item(0)?.extract()?;
        Ok(pyo3::types::PyString::new(py, s.trim_start()).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-trimr", |args, py| {
        need_args(args, 1, "str-trimr")?;
        let s: String = args.get_item(0)?.extract()?;
        Ok(pyo3::types::PyString::new(py, s.trim_end()).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-blank?", |args, py| {
        need_args(args, 1, "str-blank?")?;
        let s: String = args.get_item(0)?.extract()?;
        let blank = s.chars().all(|c| c.is_whitespace());
        Ok(pyo3::types::PyBool::new(py, blank).to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-starts-with?", |args, py| {
        need_args(args, 2, "str-starts-with?")?;
        let s: String = args.get_item(0)?.extract()?;
        let prefix: String = args.get_item(1)?.extract()?;
        Ok(pyo3::types::PyBool::new(py, s.starts_with(&prefix))
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-ends-with?", |args, py| {
        need_args(args, 2, "str-ends-with?")?;
        let s: String = args.get_item(0)?.extract()?;
        let suffix: String = args.get_item(1)?.extract()?;
        Ok(pyo3::types::PyBool::new(py, s.ends_with(&suffix))
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-includes?", |args, py| {
        need_args(args, 2, "str-includes?")?;
        let s: String = args.get_item(0)?.extract()?;
        let substr: String = args.get_item(1)?.extract()?;
        Ok(pyo3::types::PyBool::new(py, s.contains(&substr))
            .to_owned().unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-index-of", |args, py| {
        need_args(args, 2, "str-index-of")?;
        let s: String = args.get_item(0)?.extract()?;
        let substr: String = args.get_item(1)?.extract()?;
        Ok(match s.find(&substr) {
            Some(i) => (i as i64).into_pyobject(py)?.unbind().into_any(),
            None => py.None(),
        })
    })?;

    intern_fn(py, &rt_ns, "str-index-of-from", |args, py| {
        need_args(args, 3, "str-index-of-from")?;
        let s: String = args.get_item(0)?.extract()?;
        let substr: String = args.get_item(1)?.extract()?;
        let from: usize = args.get_item(2)?.extract()?;
        if from > s.len() {
            return Ok(py.None());
        }
        Ok(match s[from..].find(&substr) {
            Some(i) => ((from + i) as i64).into_pyobject(py)?.unbind().into_any(),
            None => py.None(),
        })
    })?;

    intern_fn(py, &rt_ns, "str-join", |args, py| {
        need_args(args, 2, "str-join")?;
        let sep: String = args.get_item(0)?.extract()?;
        let coll = args.get_item(1)?.unbind();
        let mut out = String::new();
        let mut cur = crate::rt::seq(py, coll)?;
        let mut first = true;
        while !cur.is_none(py) {
            let head = crate::rt::first(py, cur.clone_ref(py))?;
            if !first {
                out.push_str(&sep);
            }
            first = false;
            let s_repr = head.bind(py).str()?;
            out.push_str(&s_repr.to_string_lossy());
            cur = crate::rt::next_(py, cur)?;
        }
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    // `s`, `re-or-str`, `replacement` — accepts regex pattern or plain string.
    intern_fn(py, &rt_ns, "str-replace", |args, py| {
        need_args(args, 3, "str-replace")?;
        let s_obj = args.get_item(0)?;
        let s: String = s_obj.extract()?;
        let m = args.get_item(1)?;
        let repl_obj = args.get_item(2)?;
        let repl: String = repl_obj.extract()?;
        let re = py.import("re")?;
        let pattern_cls = re.getattr("Pattern")?;
        let out = if m.is_instance(&pattern_cls)? {
            re.call_method1("sub", (m, &repl, &s))?.extract::<String>()?
        } else {
            let needle: String = m.extract()?;
            s.replace(&needle, &repl)
        };
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-replace-first", |args, py| {
        need_args(args, 3, "str-replace-first")?;
        let s: String = args.get_item(0)?.extract()?;
        let m = args.get_item(1)?;
        let repl: String = args.get_item(2)?.extract()?;
        let re = py.import("re")?;
        let pattern_cls = re.getattr("Pattern")?;
        let out = if m.is_instance(&pattern_cls)? {
            re.call_method1("sub", (m, &repl, &s, 1u32))?.extract::<String>()?
        } else {
            let needle: String = m.extract()?;
            match s.find(&needle) {
                Some(i) => {
                    let mut r = String::with_capacity(s.len() - needle.len() + repl.len());
                    r.push_str(&s[..i]);
                    r.push_str(&repl);
                    r.push_str(&s[i + needle.len()..]);
                    r
                }
                None => s,
            }
        };
        Ok(pyo3::types::PyString::new(py, &out).unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "str-split", |args, py| {
        need_args(args, 2, "str-split")?;
        let s: String = args.get_item(0)?.extract()?;
        let m = args.get_item(1)?;
        let re = py.import("re")?;
        let pattern_cls = re.getattr("Pattern")?;
        let parts: Vec<String> = if m.is_instance(&pattern_cls)? {
            re.call_method1("split", (m, &s))?.extract()?
        } else {
            let sep: String = m.extract()?;
            s.split(&sep).map(String::from).collect()
        };
        let items: Vec<Py<PyAny>> = parts
            .into_iter()
            .map(|p| pyo3::types::PyString::new(py, &p).unbind().into_any())
            .collect();
        let t = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, t).map(|v| v.into_any())
    })?;

    intern_fn(py, &rt_ns, "str-split-limit", |args, py| {
        need_args(args, 3, "str-split-limit")?;
        let s: String = args.get_item(0)?.extract()?;
        let m = args.get_item(1)?;
        let limit: i64 = args.get_item(2)?.extract()?;
        let re = py.import("re")?;
        let pattern_cls = re.getattr("Pattern")?;
        // Python's re.split maxsplit semantics: 0 = unlimited; we emulate
        // Clojure's `limit` (max number of elements) by maxsplit = limit-1
        // when limit > 0.
        let maxsplit: i64 = if limit > 0 { limit - 1 } else { 0 };
        let parts: Vec<String> = if m.is_instance(&pattern_cls)? {
            re.call_method1("split", (m, &s, maxsplit))?.extract()?
        } else {
            let sep: String = m.extract()?;
            if maxsplit > 0 {
                s.splitn(maxsplit as usize + 1, &sep).map(String::from).collect()
            } else {
                s.split(&sep).map(String::from).collect()
            }
        };
        let items: Vec<Py<PyAny>> = parts
            .into_iter()
            .map(|p| pyo3::types::PyString::new(py, &p).unbind().into_any())
            .collect();
        let t = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, t).map(|v| v.into_any())
    })?;

    intern_fn(py, &rt_ns, "str-split-lines", |args, py| {
        need_args(args, 1, "str-split-lines")?;
        let s: String = args.get_item(0)?.extract()?;
        let items: Vec<Py<PyAny>> = s
            .split_terminator('\n')
            .map(|line| {
                let line = line.strip_suffix('\r').unwrap_or(line);
                pyo3::types::PyString::new(py, line).unbind().into_any()
            })
            .collect();
        let t = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, t).map(|v| v.into_any())
    })?;

    // (read-string-prefix s) — parse one form; return [form consumed-bytes].
    intern_fn(py, &rt_ns, "read-string-prefix", |args, py| {
        need_args(args, 1, "read-string-prefix")?;
        let s_obj = args.get_item(0)?;
        let s: &str = s_obj.extract()?;
        let (form, consumed) = crate::reader::read_string_prefix_py(py, s)?;
        let tup = PyTuple::new(py, &[form, (consumed as i64).into_pyobject(py)?.unbind().into_any()])?;
        crate::collections::pvector::vector(py, tup).map(|v| v.into_any())
    })?;

    // (setattr obj name value) — mirror of getattr.
    intern_fn(py, &rt_ns, "setattr", |args, _py| {
        need_args(args, 3, "setattr")?;
        let obj = args.get_item(0)?;
        let name: String = args.get_item(1)?.extract()?;
        let value = args.get_item(2)?;
        obj.setattr(name.as_str(), value)?;
        Ok(_py.None())
    })?;

    // (hasattr obj name)
    intern_fn(py, &rt_ns, "hasattr", |args, py| {
        need_args(args, 2, "hasattr")?;
        let obj = args.get_item(0)?;
        let name: String = args.get_item(1)?.extract()?;
        Ok(pyo3::types::PyBool::new(py, obj.hasattr(name.as_str())?).to_owned().unbind().into_any())
    })?;

    // (macroexpand-1 form) — single-step macroexpansion.
    intern_fn(py, &rt_ns, "macroexpand-1", |args, py| {
        need_args(args, 1, "macroexpand-1")?;
        let form = args.get_item(0)?.unbind();
        // Use clojure.user as the default current-ns for macroexpansion.
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        let ns = match modules.get_item("clojure.user") {
            Ok(n) => n.unbind(),
            Err(_) => {
                let sym = Symbol::new(None, Arc::from("clojure.user"));
                let sym_py = Py::new(py, sym)?;
                crate::namespace::create_ns(py, sym_py)?
            }
        };
        crate::compiler::macroexpand_1(py, form, ns)
    })?;

    // --- Thread-binding ops ---

    intern_fn(py, &rt_ns, "push-thread-bindings", |args, py| {
        need_args(args, 1, "push-thread-bindings")?;
        crate::binding::push_thread_bindings(py, args.get_item(0)?.unbind())?;
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "pop-thread-bindings", |args, py| {
        need_args(args, 0, "pop-thread-bindings")?;
        crate::binding::pop_thread_bindings()?;
        let _ = py;
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "get-thread-bindings", |args, py| {
        need_args(args, 0, "get-thread-bindings")?;
        let _ = args;
        crate::binding::get_thread_bindings(py)
    })?;

    // --- array-map constructor ---
    intern_fn(py, &rt_ns, "array-map", |args, py| {
        crate::collections::parraymap::array_map(py, args.clone())
    })?;

    // --- Numeric tower: predicates (vanilla 3606-3642) ---

    // (ratio? x) — Python fractions.Fraction.
    intern_fn(py, &rt_ns, "instance-ratio?", |args, py| {
        need_args(args, 1, "ratio?")?;
        let x = args.get_item(0)?;
        let ok = x.is_instance(fraction_cls(py)?.as_any())?;
        Ok(PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    // (decimal? x) — Python decimal.Decimal.
    intern_fn(py, &rt_ns, "instance-decimal?", |args, py| {
        need_args(args, 1, "decimal?")?;
        let x = args.get_item(0)?;
        let ok = x.is_instance(decimal_cls(py)?.as_any())?;
        Ok(PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    // (float? x) — Python float. Excludes int.
    intern_fn(py, &rt_ns, "instance-float?", |args, py| {
        need_args(args, 1, "float?")?;
        let x = args.get_item(0)?;
        use pyo3::types::PyFloat;
        let ok = x.cast::<PyFloat>().is_ok();
        Ok(PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    // (rational? x) — int, Fraction, Decimal. Not float (float isn't exact).
    intern_fn(py, &rt_ns, "instance-rational?", |args, py| {
        need_args(args, 1, "rational?")?;
        let x = args.get_item(0)?;
        if is_exact_int(&x) {
            return Ok(PyBool::new(py, true).to_owned().unbind().into_any());
        }
        if x.is_instance(fraction_cls(py)?.as_any())? {
            return Ok(PyBool::new(py, true).to_owned().unbind().into_any());
        }
        if x.is_instance(decimal_cls(py)?.as_any())? {
            return Ok(PyBool::new(py, true).to_owned().unbind().into_any());
        }
        Ok(PyBool::new(py, false).to_owned().unbind().into_any())
    })?;

    // (numerator q) — Fraction/int → numerator as int.
    intern_fn(py, &rt_ns, "numerator", |args, py| {
        need_args(args, 1, "numerator")?;
        let x = args.get_item(0)?;
        if is_exact_int(&x) {
            return Ok(x.unbind());
        }
        Ok(x.getattr("numerator")?.unbind())
    })?;

    // (denominator q) — Fraction → denominator as int; int → 1.
    intern_fn(py, &rt_ns, "denominator", |args, py| {
        need_args(args, 1, "denominator")?;
        let x = args.get_item(0)?;
        if is_exact_int(&x) {
            return Ok(1i64.into_pyobject(py)?.unbind().into_any());
        }
        Ok(x.getattr("denominator")?.unbind())
    })?;

    // (bigint x) / (biginteger x) — Python int is already arbitrary
    // precision; these are identity coercions.
    intern_fn(py, &rt_ns, "bigint", |args, py| {
        need_args(args, 1, "bigint")?;
        let x = args.get_item(0)?;
        let r = py.import("builtins")?.getattr("int")?.call1((x,))?;
        Ok(r.unbind())
    })?;
    intern_fn(py, &rt_ns, "biginteger", |args, py| {
        need_args(args, 1, "biginteger")?;
        let x = args.get_item(0)?;
        let r = py.import("builtins")?.getattr("int")?.call1((x,))?;
        Ok(r.unbind())
    })?;

    // (bigdec x) — decimal.Decimal.
    intern_fn(py, &rt_ns, "bigdec", |args, py| {
        need_args(args, 1, "bigdec")?;
        let x = args.get_item(0)?;
        let d_cls = decimal_cls(py)?;
        // Fraction → Decimal: Decimal(numerator) / Decimal(denominator).
        // CPython's Decimal(Fraction(...)) raises TypeError directly.
        if x.is_instance(fraction_cls(py)?.as_any())? {
            let num = x.getattr("numerator")?;
            let denom = x.getattr("denominator")?;
            let n = d_cls.call1((num,))?;
            let dd = d_cls.call1((denom,))?;
            return Ok(n.div(dd)?.unbind());
        }
        // Decimal(float) is lossy; route floats through str first for the
        // common case of literal decimals (e.g. (bigdec 3.14)).
        use pyo3::types::PyFloat;
        if x.cast::<PyFloat>().is_ok() {
            let s = x.str()?;
            return Ok(d_cls.call1((s,))?.unbind());
        }
        Ok(d_cls.call1((x,))?.unbind())
    })?;

    // (class? x) — true if x is a Python type / class.
    intern_fn(py, &rt_ns, "class?", |args, py| {
        need_args(args, 1, "class?")?;
        let x = args.get_item(0)?;
        let is_cls = x.cast::<pyo3::types::PyType>().is_ok();
        Ok(PyBool::new(py, is_cls).to_owned().unbind().into_any())
    })?;

    // (class x) — type(x). For instances, returns their concrete class; for
    // classes, returns `type`.
    intern_fn(py, &rt_ns, "class", |args, py| {
        need_args(args, 1, "class")?;
        let x = args.get_item(0)?;
        Ok(x.get_type().unbind().into_any())
    })?;

    // (instance? cls x) — isinstance(x, cls).
    intern_fn(py, &rt_ns, "instance?", |args, py| {
        need_args(args, 2, "instance?")?;
        let cls = args.get_item(0)?;
        let x = args.get_item(1)?;
        let cls_ty = cls.cast::<pyo3::types::PyType>().map_err(|_| {
            IllegalArgumentException::new_err("instance?: first arg must be a class")
        })?;
        Ok(pyo3::types::PyBool::new(py, x.is_instance(cls_ty)?).to_owned().unbind().into_any())
    })?;

    // (multifn-create name dispatch-fn default-val hierarchy-var) — build
    // a new MultiFn. Called from the `defmulti` macro expansion.
    intern_fn(py, &rt_ns, "multifn-create", |args, py| {
        need_args(args, 4, "multifn-create")?;
        let name: String = args.get_item(0)?.extract()?;
        let dispatch_fn = args.get_item(1)?.unbind();
        let default_val = args.get_item(2)?.unbind();
        let hierarchy_var = args.get_item(3)?.unbind();
        let mf = crate::multifn::py_multifn_create(py, name, dispatch_fn, default_val, hierarchy_var)?;
        Ok(Py::new(py, mf)?.into_any())
    })?;

    // (instance-multifn? x)
    {
        let mf_cls = py.get_type::<crate::multifn::MultiFn>().unbind().into_any();
        mk_instance_pred(py, &rt_ns, "instance-multifn?", mf_cls)?;
    }

    // (class-bases cls) — direct superclasses as a Clojure seq.
    intern_fn(py, &rt_ns, "class-bases", |args, py| {
        need_args(args, 1, "class-bases")?;
        let cls = args.get_item(0)?;
        let bases = cls.getattr("__bases__")?;
        let items: Vec<PyObject> = bases.try_iter()?
            .map(|i| i.map(|b| b.unbind()))
            .collect::<PyResult<Vec<_>>>()?;
        let tuple = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tuple)
    })?;

    // (class-ancestors cls) — full MRO excluding cls itself, as a Clojure seq.
    intern_fn(py, &rt_ns, "class-ancestors", |args, py| {
        need_args(args, 1, "class-ancestors")?;
        let cls = args.get_item(0)?;
        let mro = cls.getattr("__mro__")?;
        let all: Vec<PyObject> = mro.try_iter()?
            .map(|i| i.map(|b| b.unbind()))
            .collect::<PyResult<Vec<_>>>()?;
        // Skip index 0 (cls itself).
        let tail: Vec<PyObject> = all.into_iter().skip(1).collect();
        let tuple = PyTuple::new(py, &tail)?;
        crate::collections::plist::list_(py, tuple)
    })?;

    // (isa-class? child parent) — safe Python issubclass. True only when
    // both args are classes and child is a (non-strict) subclass of parent.
    intern_fn(py, &rt_ns, "isa-class?", |args, py| {
        need_args(args, 2, "isa-class?")?;
        let child = args.get_item(0)?;
        let parent = args.get_item(1)?;
        if child.cast::<pyo3::types::PyType>().is_err()
            || parent.cast::<pyo3::types::PyType>().is_err()
        {
            return Ok(PyBool::new(py, false).to_owned().unbind().into_any());
        }
        let builtins = py.import("builtins")?;
        let issub = builtins.getattr("issubclass")?;
        let r = issub.call1((&child, &parent))?;
        let ok: bool = r.extract()?;
        Ok(PyBool::new(py, ok).to_owned().unbind().into_any())
    })?;

    // (float-coerce x) — Python float(x). Used by core.clj's `float` and
    // `double` casts (Python has only 64-bit float, so both collapse).
    intern_fn(py, &rt_ns, "float-coerce", |args, py| {
        need_args(args, 1, "float-coerce")?;
        let x = args.get_item(0)?;
        let r = py.import("builtins")?.getattr("float")?.call1((x,))?;
        Ok(r.unbind())
    })?;

    // (rationalize x) — returns an exact Fraction (or int when exact).
    intern_fn(py, &rt_ns, "rationalize", |args, py| {
        need_args(args, 1, "rationalize")?;
        let x = args.get_item(0)?;
        if is_exact_int(&x) {
            return Ok(x.unbind());
        }
        let fractions = py.import("fractions")?;
        if x.is_instance(&fractions.getattr("Fraction")?)? {
            return Ok(x.unbind());
        }
        // Fraction(float) gives the exact bit-pattern rational; matches
        // vanilla rationalize on JVM doubles.
        let frac = fractions.getattr("Fraction")?.call1((x,))?;
        // Reduce to int if denominator is 1.
        let denom = frac.getattr("denominator")?;
        let one = 1i64.into_pyobject(py)?;
        if denom.eq(&one)? {
            return Ok(frac.getattr("numerator")?.unbind());
        }
        Ok(frac.unbind())
    })?;

    // --- Monitors (vanilla ~2900) ---

    intern_fn(py, &rt_ns, "monitor-enter", |args, py| {
        need_args(args, 1, "monitor-enter")?;
        let x = args.get_item(0)?.unbind();
        crate::monitor::monitor_enter(py, x.clone_ref(py))?;
        Ok(x)
    })?;

    intern_fn(py, &rt_ns, "monitor-exit", |args, py| {
        need_args(args, 1, "monitor-exit")?;
        let x = args.get_item(0)?.unbind();
        crate::monitor::monitor_exit(py, x)?;
        Ok(py.None())
    })?;

    // --- Additional protocol predicates (used by coll?/empty?/etc.) ---

    mk_protocol_pred(py, &rt_ns, "instance-coll?",        get_proto(m, "IPersistentCollection")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-counted?",     get_proto(m, "Counted")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-seqable?",     get_proto(m, "ISeqable")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-reversible?",  get_proto(m, "Reversible")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-indexed?",     get_proto(m, "Indexed")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-associative?", get_proto(m, "Associative")?)?;
    mk_protocol_pred(py, &rt_ns, "instance-list?",        get_proto(m, "IPersistentList")?)?;

    // --- Var introspection ---

    intern_fn(py, &rt_ns, "var-bound?", |args, py| {
        need_args(args, 1, "bound?")?;
        let raw = args.get_item(0)?;
        let v = raw.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("bound?: arg must be a Var")
        })?;
        let has_root = v.get().root.load().is_some();
        if has_root {
            return Ok(true_py(py));
        }
        let v_obj: PyObject = v.clone().unbind().into_any();
        let bound = crate::binding::lookup_binding(py, &v_obj).is_some();
        Ok(if bound { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "var-thread-bound?", |args, py| {
        need_args(args, 1, "thread-bound?")?;
        let raw = args.get_item(0)?;
        let v = raw.cast::<crate::var::Var>().map_err(|_| {
            IllegalArgumentException::new_err("thread-bound?: arg must be a Var")
        })?;
        let v_obj: PyObject = v.clone().unbind().into_any();
        let bound = crate::binding::lookup_binding(py, &v_obj).is_some();
        Ok(if bound { true_py(py) } else { false_py(py) })
    })?;

    // (alter-var-root v f & args) — atomically set v's root to (apply f
    // current-root args). Routes through the existing Var.alter_root
    // pymethod (CAS loop via ArcSwap; validates + fires watches).
    intern_fn(py, &rt_ns, "alter-var-root", |args, py| {
        if args.len() < 2 {
            return Err(IllegalArgumentException::new_err(
                "alter-var-root: at least 2 args required",
            ));
        }
        let v = args.get_item(0)?;
        let f = args.get_item(1)?.unbind();
        let mut all: Vec<PyObject> = Vec::with_capacity(args.len() - 1);
        all.push(f);
        for i in 2..args.len() {
            all.push(args.get_item(i)?.unbind());
        }
        let all_tup = PyTuple::new(py, &all)?;
        let result = v.call_method1("alter_root", all_tup)?;
        Ok(result.unbind())
    })?;

    // --- eval (Clojure-layer) ---

    intern_fn(py, &rt_ns, "eval-form", |args, py| {
        need_args(args, 1, "eval")?;
        let form = args.get_item(0)?.unbind();
        crate::eval::py_eval(py, form)
    })?;

    // --- Hash / equality helpers ---

    intern_fn(py, &rt_ns, "hash-eq", |args, py| {
        need_args(args, 1, "hash")?;
        let x = args.get_item(0)?;
        if x.is_none() {
            return Ok(0i64.into_pyobject(py)?.unbind().into_any());
        }
        // Dispatch through IHashEq so primitives (bool/int/float) get their
        // vanilla-Clojure hash instead of Python's `__hash__` (which
        // collapses 1/True/1.0).
        let h = crate::rt::hash_eq(py, x.unbind())?;
        Ok(h.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "mix-collection-hash", |args, py| {
        need_args(args, 2, "mix-collection-hash")?;
        let h: i64 = args.get_item(0)?.extract()?;
        let n: i64 = args.get_item(1)?.extract()?;
        // Mirrors Murmur3.mixCollHash on the JVM.
        let m = crate::murmur3::mix_coll_hash(h as i32, n as i32) as i64;
        Ok(m.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "hash-ordered-coll-impl", |args, py| {
        need_args(args, 1, "hash-ordered-coll")?;
        let coll = args.get_item(0)?;
        let mixed = if coll.is_none() {
            crate::murmur3::mix_coll_hash(1, 0) as i64
        } else {
            crate::murmur3::hash_ordered_seq(py, coll.unbind())? as i64
        };
        Ok(mixed.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "hash-unordered-coll-impl", |args, py| {
        need_args(args, 1, "hash-unordered-coll")?;
        let coll = args.get_item(0)?;
        let mixed = if coll.is_none() {
            crate::murmur3::mix_coll_hash(0, 0) as i64
        } else {
            crate::murmur3::hash_unordered_seq(py, coll.unbind())? as i64
        };
        Ok(mixed.into_pyobject(py)?.unbind().into_any())
    })?;

    intern_fn(py, &rt_ns, "hash-combine", |args, py| {
        need_args(args, 2, "hash-combine")?;
        let a: i64 = args.get_item(0)?.extract()?;
        let b: i64 = args.get_item(1)?.extract()?;
        let r = crate::murmur3::hash_combine(a as i32, b as i32);
        Ok((r as i64).into_pyobject(py)?.unbind().into_any())
    })?;

    // --- Class introspection ---

    intern_fn(py, &rt_ns, "bases-impl", |args, py| {
        need_args(args, 1, "bases")?;
        let cls = args.get_item(0)?;
        let bs = cls.getattr("__bases__")?;
        let tup: Bound<'_, PyTuple> = bs.cast_into()?;
        if tup.len() == 0 {
            return Ok(py.None());
        }
        let items: Vec<PyObject> = tup.iter().map(|b| b.unbind()).collect();
        let py_tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, py_tup)
    })?;

    intern_fn(py, &rt_ns, "supers-impl", |args, py| {
        need_args(args, 1, "supers")?;
        let cls = args.get_item(0)?;
        let mro = cls.getattr("__mro__")?;
        let tup: Bound<'_, PyTuple> = mro.cast_into()?;
        // skip self (index 0) — vanilla `supers` excludes the class itself.
        let items: Vec<PyObject> = tup.iter().skip(1).map(|b| b.unbind()).collect();
        if items.is_empty() {
            return Ok(py.None());
        }
        // Return a hash-set (vanilla's supers returns a set of classes).
        // hash_set takes positional varargs.
        let py_tup = PyTuple::new(py, &items)?;
        let hash_set_fn = py.import("clojure._core")?.getattr("hash_set")?;
        Ok(hash_set_fn.call1(py_tup)?.unbind())
    })?;

    // --- Random / shuffle ---

    intern_fn(py, &rt_ns, "rand-impl", |args, py| {
        let _ = args;
        let random = py.import("random")?;
        Ok(random.getattr("random")?.call0()?.unbind())
    })?;

    intern_fn(py, &rt_ns, "rand-int-impl", |args, py| {
        need_args(args, 1, "rand-int")?;
        let n: i64 = args.get_item(0)?.extract()?;
        let random = py.import("random")?;
        let r = random.getattr("randrange")?.call1((n,))?;
        Ok(r.unbind())
    })?;

    intern_fn(py, &rt_ns, "shuffle-impl", |args, py| {
        need_args(args, 1, "shuffle")?;
        let coll = args.get_item(0)?;
        let random = py.import("random")?;
        // Materialize coll as a Python list, shuffle in-place, wrap as PVector.
        let py_list = pyo3::types::PyList::empty(py);
        for item in coll.try_iter()? {
            py_list.append(item?)?;
        }
        random.getattr("shuffle")?.call1((py_list.clone(),))?;
        let tup = PyTuple::new(py, &py_list.iter().map(|x| x.unbind()).collect::<Vec<_>>())?;
        crate::collections::pvector::vector(py, tup).map(|v| v.into_any())
    })?;

    intern_fn(py, &rt_ns, "random-sample-impl", |args, py| {
        need_args(args, 2, "random-sample")?;
        let prob: f64 = args.get_item(0)?.extract()?;
        let coll = args.get_item(1)?;
        let random = py.import("random")?;
        let mut kept: Vec<PyObject> = Vec::new();
        for item in coll.try_iter()? {
            let r: f64 = random.getattr("random")?.call0()?.extract()?;
            if r < prob {
                kept.push(item?.unbind());
            }
        }
        if kept.is_empty() {
            return Ok(py.None());
        }
        let tup = PyTuple::new(py, &kept)?;
        crate::collections::plist::list_(py, tup)
    })?;

    // --- I/O ---

    intern_fn(py, &rt_ns, "format-impl", |args, py| {
        if args.is_empty() {
            return Err(IllegalArgumentException::new_err(
                "format requires at least 1 arg",
            ));
        }
        let fmt: String = args.get_item(0)?.extract()?;
        // Translate Java printf-style %s/%d/%f to Python's % operator. Both
        // syntaxes are similar enough that we forward directly — `%n` (Java
        // newline) needs explicit translation to `\n`.
        let fmt_py = fmt.replace("%n", "\n");
        let mut rest: Vec<PyObject> = Vec::with_capacity(args.len() - 1);
        for i in 1..args.len() {
            rest.push(args.get_item(i)?.unbind());
        }
        let tup = PyTuple::new(py, &rest)?;
        let fmt_str = pyo3::types::PyString::new(py, &fmt_py);
        // Python: `fmt_str % tup` (or single value if 1 arg).
        let result = if rest.len() == 1 {
            fmt_str.call_method1("__mod__", (rest[0].clone_ref(py),))?
        } else {
            fmt_str.call_method1("__mod__", (tup,))?
        };
        Ok(result.unbind())
    })?;

    intern_fn(py, &rt_ns, "slurp-impl", |args, py| {
        need_args(args, 1, "slurp")?;
        let path: String = args.get_item(0)?.extract()?;
        let builtins = py.import("builtins")?;
        // Always read as UTF-8 — vanilla Clojure JVM defaults to UTF-8 since
        // JEP 400. Without `encoding="utf-8"` Python falls back to the
        // locale encoding (cp1252 on Windows), which can't decode many
        // common multi-byte characters.
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("encoding", "utf-8")?;
        let f = builtins.getattr("open")?.call((path, "r"), Some(&kwargs))?;
        let content = f.call_method0("read")?;
        f.call_method0("close")?;
        Ok(content.unbind())
    })?;

    // (subs-impl s start) / (subs-impl s start end) — Python str slicing.
    intern_fn(py, &rt_ns, "subs-impl", |args, py| {
        if args.len() != 2 && args.len() != 3 {
            return Err(IllegalArgumentException::new_err("subs: 2 or 3 args"));
        }
        let s: String = args.get_item(0)?.extract()?;
        let start: usize = args.get_item(1)?.extract::<i64>()? as usize;
        let end: usize = if args.len() == 3 {
            args.get_item(2)?.extract::<i64>()? as usize
        } else {
            s.chars().count()
        };
        // Vanilla operates on UTF-16 indices; Python strings are codepoint-
        // indexed. We use Rust char indexing which matches Python's str
        // slicing semantics.
        let chars: Vec<char> = s.chars().collect();
        if start > chars.len() || end > chars.len() || start > end {
            return Err(IllegalArgumentException::new_err("subs: index out of range"));
        }
        let slice: String = chars[start..end].iter().collect();
        Ok(pyo3::types::PyString::new(py, &slice).unbind().into_any())
    })?;

    // (string-io) → fresh io.StringIO. (string-io s) → io.StringIO(s).
    intern_fn(py, &rt_ns, "string-io", |args, py| {
        let io = py.import("io")?;
        let cls = io.getattr("StringIO")?;
        if args.is_empty() {
            Ok(cls.call0()?.unbind())
        } else if args.len() == 1 {
            Ok(cls.call1((args.get_item(0)?,))?.unbind())
        } else {
            Err(IllegalArgumentException::new_err("string-io: 0 or 1 args"))
        }
    })?;

    intern_fn(py, &rt_ns, "spit-impl", |args, py| {
        need_args(args, 2, "spit")?;
        let path: String = args.get_item(0)?.extract()?;
        let content_obj = args.get_item(1)?;
        let content: String = content_obj.str()?.to_string_lossy().into_owned();
        let builtins = py.import("builtins")?;
        // Always write UTF-8 — see slurp-impl for the rationale.
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("encoding", "utf-8")?;
        let f = builtins.getattr("open")?.call((path, "w"), Some(&kwargs))?;
        f.call_method1("write", (content,))?;
        f.call_method0("close")?;
        Ok(py.None())
    })?;

    // --- Regex matcher state ---
    //
    // Vanilla's re-matcher returns a stateful java.util.regex.Matcher that
    // tracks position across re-find calls. Python's re module doesn't have
    // an exact equivalent — re.finditer returns an iterator. We model
    // Matcher as a small pyclass holding (pattern, string, last-match);
    // calling re-find on it advances. re-groups extracts groups from the
    // last match.

    intern_fn(py, &rt_ns, "re-matcher-impl", |args, py| {
        need_args(args, 2, "re-matcher")?;
        let pat = args.get_item(0)?;
        let s = args.get_item(1)?;
        let m = crate::regex::Matcher::new(pat.unbind(), s.unbind(), py)?;
        Ok(Py::new(py, m)?.into_any())
    })?;

    intern_fn(py, &rt_ns, "re-find-matcher-impl", |args, py| {
        need_args(args, 1, "re-find")?;
        let m_obj = args.get_item(0)?;
        let m = m_obj.downcast::<crate::regex::Matcher>().map_err(|_| {
            IllegalArgumentException::new_err(
                "re-find: expected a Matcher (from re-matcher) or pattern + string",
            )
        })?;
        let next = m.get().advance(py)?;
        if next.is_none(py) {
            return Ok(py.None());
        }
        let next_b = next.bind(py);
        let groups_tuple = next_b.call_method0("groups")?;
        let groups: Bound<'_, PyTuple> = groups_tuple.cast_into()?;
        let whole = next_b.call_method1("group", (0i64,))?;
        if groups.len() == 0 {
            return Ok(whole.unbind());
        }
        let mut items: Vec<PyObject> = Vec::with_capacity(groups.len() + 1);
        items.push(whole.unbind());
        for g in groups.iter() {
            items.push(g.unbind());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, tup).map(|v| v.into_any())
    })?;

    intern_fn(py, &rt_ns, "re-groups-impl", |args, py| {
        need_args(args, 1, "re-groups")?;
        let m_obj = args.get_item(0)?;
        // m_obj may be either a Matcher (our pyclass) or a re.Match.
        let last_match: PyObject = if let Ok(matcher) = m_obj.downcast::<crate::regex::Matcher>() {
            let last = matcher.get().last(py);
            if last.is_none(py) {
                return Err(IllegalStateException::new_err("No match found"));
            }
            last
        } else {
            m_obj.unbind()
        };
        let m = last_match.bind(py);
        let groups_tuple = m.call_method0("groups")?;
        let groups: Bound<'_, PyTuple> = groups_tuple.cast_into()?;
        let whole = m.call_method1("group", (0i64,))?;
        if groups.len() == 0 {
            return Ok(whole.unbind());
        }
        let mut items: Vec<PyObject> = Vec::with_capacity(groups.len() + 1);
        items.push(whole.unbind());
        for g in groups.iter() {
            items.push(g.unbind());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::pvector::vector(py, tup).map(|v| v.into_any())
    })?;

    // --- inst / NaN / infinite predicates ---

    intern_fn(py, &rt_ns, "inst-q", |args, py| {
        need_args(args, 1, "inst?")?;
        let x = args.get_item(0)?;
        let datetime = py.import("datetime")?;
        let dt_cls = datetime.getattr("datetime")?;
        let date_cls = datetime.getattr("date")?;
        let ok = x.is_instance(&dt_cls)? || x.is_instance(&date_cls)?;
        Ok(if ok { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "inst-ms-impl", |args, py| {
        need_args(args, 1, "inst-ms")?;
        let x = args.get_item(0)?;
        // datetime.timestamp() → seconds since epoch; * 1000 → ms.
        let secs: f64 = x.call_method0("timestamp")?.extract()?;
        let ms = (secs * 1000.0) as i64;
        Ok(ms.into_pyobject(py)?.unbind().into_any())
    })?;

    // (cast c x) — return x if isinstance(x, c) else raise TypeError.
    intern_fn(py, &rt_ns, "cast-impl", |args, _py| {
        need_args(args, 2, "cast")?;
        let c = args.get_item(0)?;
        let x = args.get_item(1)?;
        if x.is_instance(&c)? {
            Ok(x.unbind())
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Cannot cast {} to {}",
                x.get_type().name()?.to_string(),
                c.getattr("__name__").map(|n| n.to_string()).unwrap_or_else(|_| "<class>".into())
            )))
        }
    })?;

    intern_fn(py, &rt_ns, "bytes-q", |args, py| {
        need_args(args, 1, "bytes?")?;
        let x = args.get_item(0)?;
        let py_bytes = py.import("builtins")?.getattr("bytes")?;
        let py_bytearray = py.import("builtins")?.getattr("bytearray")?;
        let ok = x.is_instance(&py_bytes)? || x.is_instance(&py_bytearray)?;
        Ok(if ok { true_py(py) } else { false_py(py) })
    })?;

    // (datetime-class) → Python's datetime.datetime class. Used by
    // core.clj to extend the Inst protocol.
    // (xml-children elem) — return Python's list(elem), or nil if no children.
    intern_fn(py, &rt_ns, "xml-children", |args, py| {
        need_args(args, 1, "xml-children")?;
        let elem = args.get_item(0)?;
        let builtins = py.import("builtins")?;
        let lst = builtins.getattr("list")?.call1((elem,))?;
        let py_list: Bound<'_, pyo3::types::PyList> = lst.cast_into()?;
        if py_list.len() == 0 {
            return Ok(py.None());
        }
        let items: Vec<PyObject> = py_list.iter().map(|x| x.unbind()).collect();
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    intern_fn(py, &rt_ns, "datetime-class", |args, py| {
        let _ = args;
        Ok(py.import("datetime")?.getattr("datetime")?.unbind())
    })?;

    intern_fn(py, &rt_ns, "tagged-literal-q", |args, py| {
        need_args(args, 1, "tagged-literal?")?;
        let x = args.get_item(0)?;
        let ok = x.cast::<crate::tagged::TaggedLiteral>().is_ok();
        Ok(if ok { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "reader-conditional-q", |args, py| {
        need_args(args, 1, "reader-conditional?")?;
        let x = args.get_item(0)?;
        let ok = x.cast::<crate::tagged::ReaderConditional>().is_ok();
        Ok(if ok { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "uri-q", |args, py| {
        need_args(args, 1, "uri?")?;
        let x = args.get_item(0)?;
        // Vanilla checks java.net.URI. Python has no canonical URI type;
        // recognize urllib.parse.ParseResult and SplitResult.
        let urlparse = py.import("urllib.parse")?;
        let pr = urlparse.getattr("ParseResult")?;
        let sr = urlparse.getattr("SplitResult")?;
        let ok = x.is_instance(&pr)? || x.is_instance(&sr)?;
        Ok(if ok { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "uuid-q", |args, py| {
        need_args(args, 1, "uuid?")?;
        let x = args.get_item(0)?;
        let uuid_mod = py.import("uuid")?;
        let uuid_cls = uuid_mod.getattr("UUID")?;
        let ok = x.is_instance(&uuid_cls)?;
        Ok(if ok { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "nan-q", |args, py| {
        need_args(args, 1, "NaN?")?;
        let x = args.get_item(0)?;
        let f: f64 = match x.extract() {
            Ok(v) => v,
            Err(_) => return Ok(false_py(py)),
        };
        Ok(if f.is_nan() { true_py(py) } else { false_py(py) })
    })?;

    intern_fn(py, &rt_ns, "infinite-q", |args, py| {
        need_args(args, 1, "infinite?")?;
        let x = args.get_item(0)?;
        let f: f64 = match x.extract() {
            Ok(v) => v,
            Err(_) => return Ok(false_py(py)),
        };
        Ok(if f.is_infinite() { true_py(py) } else { false_py(py) })
    })?;

    // --- Throwable->map ---

    intern_fn(py, &rt_ns, "throwable-to-map", |args, py| {
        need_args(args, 1, "Throwable->map")?;
        let ex = args.get_item(0)?;
        // Build a Clojure map: {:cause msg :via [...] :trace [...] [:data m if ex-info]}
        let msg_kw = crate::keyword::keyword(py, "cause", None)?;
        let via_kw = crate::keyword::keyword(py, "via", None)?;
        let trace_kw = crate::keyword::keyword(py, "trace", None)?;
        let data_kw = crate::keyword::keyword(py, "data", None)?;
        let msg = if let Ok(args_attr) = ex.getattr("args") {
            if let Ok(t) = args_attr.cast::<PyTuple>() {
                if t.len() > 0 {
                    t.get_item(0)?.str()?.unbind().into_any()
                } else {
                    py.None()
                }
            } else {
                py.None()
            }
        } else {
            py.None()
        };
        let mut entries: Vec<(PyObject, PyObject)> = Vec::new();
        entries.push((msg_kw.into_any(), msg));
        // via: empty for now.
        let empty_vec = {
            let tup = PyTuple::new(py, &Vec::<PyObject>::new())?;
            crate::collections::pvector::vector(py, tup)?.into_any()
        };
        entries.push((via_kw.into_any(), empty_vec.clone_ref(py)));
        entries.push((trace_kw.into_any(), empty_vec));
        if let Ok(d) = ex.getattr("data") {
            if !d.is_none() {
                entries.push((data_kw.into_any(), d.unbind()));
            }
        }
        // Build map via array-map (positional varargs: k1 v1 k2 v2 …).
        let mut flat: Vec<PyObject> = Vec::with_capacity(entries.len() * 2);
        for (k, v) in entries {
            flat.push(k);
            flat.push(v);
        }
        let tup = PyTuple::new(py, &flat)?;
        let arr_map_fn = py.import("clojure._core")?.getattr("array_map")?;
        Ok(arr_map_fn.call1(tup)?.unbind())
    })?;

    // --- Tap system ---
    //
    // Vanilla maintains a global set of tap-fns and an unbounded async queue.
    // We keep the same shape: a Mutex<Vec<PyObject>> of fns; tap> calls each
    // with the value (synchronously — the queue is simulated; vanilla's queue
    // mostly hides slow taps from the caller, but behaviour is the same for
    // fast taps).

    intern_fn(py, &rt_ns, "add-tap-impl", |args, py| {
        need_args(args, 1, "add-tap")?;
        let f = args.get_item(0)?.unbind();
        crate::tap::add(py, f);
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "remove-tap-impl", |args, py| {
        need_args(args, 1, "remove-tap")?;
        let f = args.get_item(0)?.unbind();
        crate::tap::remove(py, f);
        Ok(py.None())
    })?;

    intern_fn(py, &rt_ns, "tap-bang-impl", |args, py| {
        need_args(args, 1, "tap>")?;
        let v = args.get_item(0)?.unbind();
        let fired = crate::tap::fire(py, v)?;
        Ok(if fired { true_py(py) } else { false_py(py) })
    })?;

    // --- File / iter seq ---

    intern_fn(py, &rt_ns, "iterator-seq-impl", |args, py| {
        need_args(args, 1, "iterator-seq")?;
        let it = args.get_item(0)?;
        // Materialize lazily by wrapping in a Python iterator that already
        // yields the elements; convert to a Clojure list eagerly (vanilla
        // is lazy; future work to make this lazy too).
        let items: Vec<PyObject> = it.try_iter()?.map(|x| x.map(|y| y.unbind())).collect::<PyResult<Vec<_>>>()?;
        if items.is_empty() {
            return Ok(py.None());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    intern_fn(py, &rt_ns, "file-seq-impl", |args, py| {
        need_args(args, 1, "file-seq")?;
        let root: String = args.get_item(0)?.extract()?;
        let os = py.import("os")?;
        let walk = os.getattr("walk")?;
        let walker = walk.call1((root.clone(),))?;
        let mut items: Vec<PyObject> = Vec::new();
        items.push(pyo3::types::PyString::new(py, &root).unbind().into_any());
        for entry in walker.try_iter()? {
            let triple: Bound<'_, PyTuple> = entry?.cast_into()?;
            let dirpath = triple.get_item(0)?;
            let dirnames: Bound<'_, pyo3::types::PyList> = triple.get_item(1)?.cast_into()?;
            let filenames: Bound<'_, pyo3::types::PyList> = triple.get_item(2)?.cast_into()?;
            let join = py.import("os")?.getattr("path")?.getattr("join")?;
            for d in dirnames.iter() {
                let p = join.call1((dirpath.clone(), d))?;
                items.push(p.unbind());
            }
            for f in filenames.iter() {
                let p = join.call1((dirpath.clone(), f))?;
                items.push(p.unbind());
            }
        }
        if items.is_empty() {
            return Ok(py.None());
        }
        let tup = PyTuple::new(py, &items)?;
        crate::collections::plist::list_(py, tup)
    })?;

    Ok(())
}

/// Read meta off an IReference-ish (Var, Namespace, Atom, Ref, Agent). IAE otherwise.
fn get_reference_meta(py: Python<'_>, r: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    if let Ok(a) = r.cast::<crate::atom::Atom>() {
        let g = a.get().meta.load();
        let opt: &Option<PyObject> = &g;
        return Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()));
    }
    if let Ok(rf) = r.cast::<crate::stm::ref_::Ref>() {
        let g = rf.get().meta.load();
        let opt: &Option<PyObject> = &g;
        return Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()));
    }
    if let Ok(ag) = r.cast::<crate::agent::Agent>() {
        let g = ag.get().meta.load();
        let opt: &Option<PyObject> = &g;
        return Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()));
    }
    if let Ok(v) = r.cast::<crate::var::Var>() {
        let g = v.get().meta.load();
        let opt: &Option<PyObject> = &g;
        return Ok(opt.as_ref().map(|m| m.clone_ref(py)).unwrap_or_else(|| py.None()));
    }
    if crate::namespace::is_clojure_namespace(py, r)? {
        return match r.getattr("__clj_ns_meta__") {
            Ok(m) => Ok(m.unbind()),
            Err(_) => Ok(py.None()),
        };
    }
    Err(IllegalArgumentException::new_err(
        "alter-meta!/reset-meta! requires an IReference (Var/Atom/Ref/Namespace)"
    ))
}

fn set_reference_meta(py: Python<'_>, r: &Bound<'_, PyAny>, m: PyObject) -> PyResult<()> {
    let mv = if m.is_none(py) { None } else { Some(m) };
    if let Ok(a) = r.cast::<crate::atom::Atom>() {
        a.get().meta.store(Arc::new(mv));
        return Ok(());
    }
    if let Ok(rf) = r.cast::<crate::stm::ref_::Ref>() {
        rf.get().meta.store(Arc::new(mv));
        return Ok(());
    }
    if let Ok(ag) = r.cast::<crate::agent::Agent>() {
        ag.get().meta.store(Arc::new(mv));
        return Ok(());
    }
    if let Ok(v) = r.cast::<crate::var::Var>() {
        v.get().set_meta(mv);
        return Ok(());
    }
    if crate::namespace::is_clojure_namespace(py, r)? {
        let val = mv.unwrap_or_else(|| py.None());
        r.setattr("__clj_ns_meta__", val)?;
        return Ok(());
    }
    Err(IllegalArgumentException::new_err(
        "alter-meta!/reset-meta! requires an IReference (Var/Atom/Ref/Namespace)"
    ))
}

/// `RT/nth` implementation. Fast path: dispatch through `Indexed`; on
/// IllegalArgumentException (no Indexed impl), walk via seq+next.
fn rt_nth(py: Python<'_>, coll: PyObject, i: PyObject, default: Option<PyObject>) -> PyResult<PyObject> {
    if coll.is_none(py) {
        return Ok(default.unwrap_or_else(|| py.None()));
    }
    let result = match &default {
        Some(d) => {
            static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
            crate::protocol_fn::dispatch_cached_3(
                py, &PFN, "Indexed", "nth_or_default",
                coll.clone_ref(py), i.clone_ref(py), d.clone_ref(py),
            )
        }
        None => {
            static PFN: once_cell::sync::OnceCell<Py<crate::protocol_fn::ProtocolFn>> = once_cell::sync::OnceCell::new();
            crate::protocol_fn::dispatch_cached_2(
                py, &PFN, "Indexed", "nth",
                coll.clone_ref(py), i.clone_ref(py),
            )
        }
    };
    match result {
        Ok(v) => Ok(v),
        Err(e) if e.is_instance_of::<crate::exceptions::IllegalArgumentException>(py) => {
            // Fallback: seq-walk to the ith element.
            nth_seq_walk(py, coll, i, default)
        }
        Err(e) => Err(e),
    }
}

fn nth_seq_walk(py: Python<'_>, coll: PyObject, i: PyObject, default: Option<PyObject>) -> PyResult<PyObject> {
    let idx: i64 = i.bind(py).extract()?;
    if idx < 0 {
        return match default {
            Some(d) => Ok(d),
            None => Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "nth index {idx} out of bounds"
            ))),
        };
    }
    let mut cur = crate::rt::seq(py, coll)?;
    let mut n: i64 = 0;
    while !cur.is_none(py) {
        if n == idx {
            return crate::rt::first(py, cur);
        }
        cur = crate::rt::next_(py, cur)?;
        n += 1;
    }
    match default {
        Some(d) => Ok(d),
        None => Err(pyo3::exceptions::PyIndexError::new_err(format!(
            "nth index {idx} out of bounds"
        ))),
    }
}

/// True for Python int that isn't a bool. Bool is int's subclass, but
/// Clojure treats it as a separate scalar type for arithmetic purposes.
fn is_exact_int(x: &Bound<'_, PyAny>) -> bool {
    use pyo3::types::PyInt;
    x.cast::<PyInt>().is_ok() && x.cast::<PyBool>().is_err()
}

/// Reduce `Fraction(n, 1)` to plain int. Pass-through for non-Fraction values.
fn normalize_ratio<'py>(py: Python<'py>, x: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::types::{PyFloat, PyInt};
    if x.is_instance_of::<PyInt>() || x.is_instance_of::<PyFloat>() {
        return Ok(x);
    }
    let frac_cls = fraction_cls(py)?;
    if x.is_instance(&frac_cls)? {
        let one = 1i64.into_pyobject(py)?;
        let denom = x.getattr("denominator")?;
        if denom.eq(&one)? {
            return Ok(x.getattr("numerator")?);
        }
    }
    Ok(x)
}

/// Reject non-numeric arguments to arithmetic ops. Vanilla raises
/// `ClassCastException` from `Numbers.ops`; we surface it as
/// `IllegalArgumentException`. Numeric types we accept: int, float,
/// `fractions.Fraction`, `decimal.Decimal`.
fn ensure_numeric(x: &Bound<'_, PyAny>, op: &str) -> PyResult<()> {
    use pyo3::types::{PyFloat, PyInt};
    if x.cast::<PyInt>().is_ok() || x.cast::<PyFloat>().is_ok() {
        return Ok(());
    }
    // Fraction / Decimal — Python types we treat as numeric.
    let py = x.py();
    if let Ok(numbers_mod) = py.import("numbers") {
        if let Ok(num_cls) = numbers_mod.getattr("Number") {
            if x.is_instance(&num_cls)? {
                return Ok(());
            }
        }
    }
    let cls = x.get_type();
    let cls_name: String = cls
        .getattr("__name__")
        .and_then(|n| n.extract())
        .unwrap_or_else(|_| String::from("<unknown>"));
    Err(crate::exceptions::IllegalArgumentException::new_err(
        format!("{}: cannot operate on non-numeric type {}", op, cls_name),
    ))
}

fn m_ref<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    Ok(py.import("clojure._core")?)
}
