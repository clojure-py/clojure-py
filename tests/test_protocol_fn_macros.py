"""Phase 3 verification: every protocol method is now a ProtocolFn at the
primary module-level name.

Before Phase 3 this file asserted against `_pfn:<name>` transitional attrs
(ProtocolFn exposed alongside ProtocolMethod). Phase 3 promoted ProtocolFn
to the primary binding, so we check the primary names directly and confirm
dispatch equivalence between the two paths is no longer observable (they
ARE the same path now).
"""

import pytest
import clojure  # registers importer
import clojure._core as c
from clojure._core import ProtocolFn, eval_string


def test_primary_method_names_are_protocol_fns():
    # Sample: count, val_at, first, next. Post-Phase-3 these are all
    # ProtocolFns at the module-level name.
    for name in ("count", "val_at", "first", "next", "conj"):
        obj = getattr(c, name)
        assert isinstance(obj, ProtocolFn), f"{name!r} is {type(obj).__name__}, not ProtocolFn"


def test_counted_count_on_vector():
    v = eval_string("[1 2 3 4 5]")
    assert c.count(v) == 5


def test_ilookup_val_at_on_map():
    m = eval_string("{:a 1 :b 2}")
    kw = eval_string(":a")
    nf = eval_string("nil")
    assert c.val_at(m, kw, nf) == 1


def test_iseq_first_on_list():
    lst = eval_string("'(10 20 30)")
    assert c.first(lst) == 10


def test_iseq_next_on_list():
    lst = eval_string("'(10 20 30)")
    nx = c.next(lst)
    assert c.first(nx) == 20


def test_counted_fallback_on_str():
    # Phase 3.1's fall-through in action: `str` has no direct Counted impl
    # in the ProtocolFn's typed table, but the old Protocol's __len__
    # fallback catches it.
    assert eval_string('(count "hello")') == 5
