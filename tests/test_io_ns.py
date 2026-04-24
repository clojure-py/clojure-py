"""I/O (read-line, load-string, load-reader, line-seq, simple read) and
namespace-introspection (all-ns, remove-ns, ns-map, ns-publics, ns-interns,
ns-unmap, refer, ns-refers, alias, ns-aliases, ns-unalias) tests."""

import io
import sys
import pytest
from clojure._core import eval_string


def _ev(s):
    return eval_string(s)


# --- I/O ---


def test_load_string_defines_var():
    _ev('(load-string "(def --ls-x 777)")')
    assert _ev("--ls-x") == 777


def test_load_string_multiple_forms():
    _ev('(load-string "(def --ls-a 1) (def --ls-b 2)")')
    assert _ev("--ls-a") == 1
    assert _ev("--ls-b") == 2


def _inject(name, obj):
    """Make `obj` accessible to Clojure code under `name`.
    Defines a Var via `(def name nil)` then binds its root to `obj`."""
    _ev("(def %s nil)" % name)
    user_ns = sys.modules["clojure.user"]
    var = user_ns.__dict__[name]
    var.bind_root(obj)


def test_read_line_from_stringio():
    buf = io.StringIO("hello\nworld\n")
    ns = sys.modules["clojure.core"]
    star_in = ns.__dict__["*in*"]
    old_root = star_in.deref() if star_in.deref() is not None else sys.stdin
    star_in.bind_root(buf)
    try:
        assert _ev("(read-line)") == "hello"
        assert _ev("(read-line)") == "world"
        assert _ev("(read-line)") is None  # EOF
    finally:
        star_in.bind_root(old_root)


def test_line_seq():
    buf = io.StringIO("a\nb\nc\n")
    _inject("--ls-buf", buf)
    assert list(_ev("(line-seq --ls-buf)")) == ["a", "b", "c"]


def test_simple_read_from_stream():
    buf = io.StringIO("[1 2 3]\n:kw\n")
    _inject("--r-buf", buf)
    assert list(_ev("(read --r-buf)")) == [1, 2, 3]
    assert _ev("(read --r-buf)") == _ev(":kw")


def test_read_multi_line_form():
    buf = io.StringIO("(+ 1\n  2\n  3)\n")
    _inject("--r-ml", buf)
    assert _ev("(= (read --r-ml) (quote (+ 1 2 3)))") is True


def test_read_multiple_forms_on_one_line():
    # Three forms packed on one physical line — each (read) returns one.
    buf = io.StringIO("(+ 1 2) (+ 3 4) :end\n")
    _inject("--r-multi", buf)
    assert _ev("(= (read --r-multi) (quote (+ 1 2)))") is True
    assert _ev("(= (read --r-multi) (quote (+ 3 4)))") is True
    assert _ev("(read --r-multi)") == _ev(":end")
    assert _ev("(read --r-multi)") is None


def test_read_form_spanning_lines_mixed_with_one_liner():
    buf = io.StringIO("(vec\n [1 2 3])\n :kw\n")
    _inject("--r-span", buf)
    # The spanning form is read as a list; compare by its printed form.
    assert _ev("(pr-str (read --r-span))") == "(vec [1 2 3])"
    assert _ev("(read --r-span)") == _ev(":kw")


def test_read_deeply_nested_across_lines():
    buf = io.StringIO("{:a 1\n :b {:c\n      [1 2\n       3]}}\n")
    _inject("--r-deep", buf)
    assert _ev("(= (read --r-deep) {:a 1 :b {:c [1 2 3]}})") is True


def test_read_eof_mid_form_raises():
    # Unterminated list — true EOF with partial form should raise.
    buf = io.StringIO("(+ 1 2")
    _inject("--r-bad", buf)
    with pytest.raises(Exception):
        _ev("(read --r-bad)")


def test_load_reader_from_stringio():
    buf = io.StringIO("(def --lr-x 42) (def --lr-y :k)")
    _inject("--lr-buf", buf)
    _ev("(load-reader --lr-buf)")
    assert _ev("--lr-x") == 42
    assert _ev("--lr-y") == _ev(":k")


# --- NS-introspection ---


def test_all_ns_nonempty():
    assert _ev("(pos? (count (all-ns)))") is True


def test_all_ns_includes_clojure_core():
    assert _ev("(some (fn* [n] (= (ns-name n) (symbol \"clojure.core\"))) (all-ns))") is True


def test_ns_map_returns_vars():
    n = _ev("(count (ns-map (find-ns (symbol \"clojure.core\"))))")
    assert n > 400


def _map_key_strings(m):
    """Return a set of str(key) for a Clojure persistent map (iterable of keys)."""
    return {str(k) for k in m}


def test_ns_publics_excludes_private():
    # Define a private var and confirm it's not in publics.
    _ev("(def ^{:private true} --priv-foo 1)")
    _ev("(def --pub-bar 2)")
    pub_keys = _map_key_strings(_ev("(keys (ns-publics (find-ns (symbol \"clojure.user\"))))"))
    int_keys = _map_key_strings(_ev("(keys (ns-interns (find-ns (symbol \"clojure.user\"))))"))
    assert "--pub-bar" in pub_keys, pub_keys
    assert "--priv-foo" in int_keys
    assert "--priv-foo" not in pub_keys, pub_keys


def test_ns_interns_only_home_ns():
    # ns-interns should filter OUT vars referred from other namespaces.
    vars_seq = _ev("(vals (ns-interns (find-ns (symbol \"clojure.core\"))))")
    core_ns = sys.modules["clojure.core"]
    for var in vars_seq:
        assert var.ns is core_ns


def test_ns_unmap_removes_mapping():
    _ev("(def --um-foo 42)")
    assert _ev("--um-foo") == 42
    _ev("(ns-unmap (find-ns (symbol \"clojure.user\")) (symbol \"--um-foo\"))")
    # Referencing now should fail.
    with pytest.raises(Exception):
        _ev("--um-foo")


def test_remove_ns_deletes_namespace():
    _ev("(clojure.lang.RT/create-ns (symbol \"temp.ns-to-delete\"))")
    assert _ev("(find-ns (symbol \"temp.ns-to-delete\"))") is not None
    _ev("(remove-ns (symbol \"temp.ns-to-delete\"))")
    assert _ev("(find-ns (symbol \"temp.ns-to-delete\"))") is None


def test_alias_and_ns_aliases():
    # Create a target namespace with a var, then alias it from clojure.user.
    _ev("(clojure.lang.RT/create-ns (symbol \"a.b.c\"))")
    _ev("(alias (symbol \"abc\") (symbol \"a.b.c\"))")
    keys = _map_key_strings(_ev("(keys (ns-aliases (find-ns (symbol \"clojure.user\"))))"))
    assert "abc" in keys


def test_ns_unalias_removes_alias():
    _ev("(clojure.lang.RT/create-ns (symbol \"x.y.z\"))")
    _ev("(alias (symbol \"xyz-al\") (symbol \"x.y.z\"))")
    _ev("(ns-unalias (find-ns (symbol \"clojure.user\")) (symbol \"xyz-al\"))")
    keys = _map_key_strings(_ev("(keys (ns-aliases (find-ns (symbol \"clojure.user\"))))"))
    assert "xyz-al" not in keys


def _intern_var_in(ns_name: str, sym_name: str, value, private: bool = False):
    """Helper: create (or get) `ns_name`, intern `sym_name` there with `value`."""
    ns_obj = _ev("(clojure.lang.RT/create-ns (symbol %r))" % ns_name)
    _ev("(clojure.lang.RT/intern (find-ns (symbol %r)) (symbol %r))"
        % (ns_name, sym_name))
    _ev(
        "(clojure.lang.RT/bind-root "
        "  (clojure.lang.RT/getattr (find-ns (symbol %r)) %r nil) "
        "  %s)"
        % (ns_name, sym_name, value)
    )
    if private:
        _ev(
            "(clojure.lang.RT/set-reference-meta "
            "  (clojure.lang.RT/getattr (find-ns (symbol %r)) %r nil) "
            "  {:private true})"
            % (ns_name, sym_name)
        ) if False else None
        # Use alter-meta! at the Clojure level.
        _ev(
            "(alter-meta! (clojure.lang.RT/getattr (find-ns (symbol %r)) %r nil) "
            "             (fn* [_] {:private true}))"
            % (ns_name, sym_name)
        )


def test_refer_installs_public_vars():
    # Build a source namespace with one public and one private var by hand.
    _ev("(clojure.lang.RT/create-ns (symbol \"src.for.refer\"))")
    _ev("(clojure.lang.RT/intern (find-ns (symbol \"src.for.refer\")) (symbol \"visible\"))")
    _ev("(clojure.lang.RT/intern (find-ns (symbol \"src.for.refer\")) (symbol \"hidden\"))")
    # Set roots.
    _ev("(clojure.lang.RT/bind-root "
        "  (clojure.lang.RT/getattr (find-ns (symbol \"src.for.refer\")) \"visible\" nil) :ok)")
    _ev("(clojure.lang.RT/bind-root "
        "  (clojure.lang.RT/getattr (find-ns (symbol \"src.for.refer\")) \"hidden\" nil) :secret)")
    # Mark `hidden` private.
    _ev("(alter-meta! "
        "  (clojure.lang.RT/getattr (find-ns (symbol \"src.for.refer\")) \"hidden\" nil) "
        "  (fn* [_] {:private true}))")
    _ev("(refer (symbol \"src.for.refer\"))")
    # visible should now be accessible from clojure.user
    assert _ev("visible") == _ev(":ok")
    # hidden should NOT be referred (private filter)
    with pytest.raises(Exception):
        _ev("hidden")


def test_ns_refers_tracks_referred_vars():
    _ev("(clojure.lang.RT/create-ns (symbol \"provider.ns\"))")
    _ev("(clojure.lang.RT/intern (find-ns (symbol \"provider.ns\")) (symbol \"exported\"))")
    _ev("(clojure.lang.RT/bind-root "
        "  (clojure.lang.RT/getattr (find-ns (symbol \"provider.ns\")) \"exported\" nil) 7)")
    _ev("(refer (symbol \"provider.ns\"))")
    keys = _map_key_strings(_ev("(keys (ns-refers (find-ns (symbol \"clojure.user\"))))"))
    assert "exported" in keys
