"""Phase 2 verification: macros emit ProtocolFns with typed impls in parallel
with the old ProtocolMethod path.

These tests poke at the `_pfn:<name>` hidden module attrs that Phase 2 emits
alongside the primary ProtocolMethod bindings. When Phase 3 flips the primary
bindings to ProtocolFn, these hidden attrs go away — this test file moves
with them.
"""

import pytest
import clojure  # registers importer
import clojure._core as c
from clojure._core import ProtocolFn, ProtocolMethod, eval_string


def _pfn(name: str) -> ProtocolFn:
    pf = getattr(c, f"_pfn:{name}", None)
    assert isinstance(pf, ProtocolFn), (
        f"no ProtocolFn for method '{name}'; Phase 2 #[protocol] macro "
        f"should have registered one."
    )
    return pf


def test_every_protocol_method_has_a_matching_protocol_fn():
    pms = {n for n, v in vars(c).items() if isinstance(v, ProtocolMethod)}
    pfns = {n.removeprefix("_pfn:") for n, v in vars(c).items()
            if isinstance(v, ProtocolFn) and n.startswith("_pfn:")}
    missing = pms - pfns
    assert not missing, (
        f"Method(s) without ProtocolFn: {sorted(missing)}. "
        f"Check #[protocol] macro output."
    )


def test_counted_count_on_vector():
    count_pf = _pfn("count")
    v = eval_string("[1 2 3 4 5]")
    assert count_pf(v) == 5


def test_ilookup_val_at_on_map():
    val_at_pf = _pfn("val_at")
    m = eval_string("{:a 1 :b 2}")
    kw = eval_string(":a")
    nf = eval_string("nil")
    assert val_at_pf(m, kw, nf) == 1


def test_iseq_first_on_list():
    first_pf = _pfn("first")
    lst = eval_string("'(10 20 30)")
    assert first_pf(lst) == 10


def test_iseq_next_on_list():
    next_pf = _pfn("next")
    lst = eval_string("'(10 20 30)")
    nx = next_pf(lst)
    # Verify via `first` that it's the expected seq.
    first_pf = _pfn("first")
    assert first_pf(nx) == 20


def test_protocol_fn_matches_protocol_method_for_same_call():
    """For a few sample (method, target) pairs, calling via ProtocolFn and
    via the primary-binding ProtocolMethod yields the same result."""
    v = eval_string("[1 2 3]")
    assert c.count(v) == _pfn("count")(v)

    m = eval_string("{:a 1}")
    kw = eval_string(":a")
    nf = eval_string("nil")
    assert c.val_at(m, kw, nf) == _pfn("val_at")(m, kw, nf)
