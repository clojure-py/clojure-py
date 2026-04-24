"""ProtocolFn: direct construction + Python-level dispatch.

These tests bypass the macros entirely — we want to confirm the new
pyclass's dispatch behaves correctly in isolation. Dispatch tests with
registered impls arrive in Phase 2 along with the macro work; right now
extend-type is the piece being designed, so we stop at the surface.
"""

import pytest
import clojure  # registers importer
from clojure._core import ProtocolFn, IllegalArgumentException


def test_empty_protocol_raises_on_call():
    pf = ProtocolFn("first", "ISeq", False)
    with pytest.raises(IllegalArgumentException, match="No implementation"):
        pf([1, 2, 3])


def test_call_with_no_args_raises():
    pf = ProtocolFn("first", "ISeq", False)
    with pytest.raises(IllegalArgumentException, match="requires at least one arg"):
        pf()


def test_repr():
    pf = ProtocolFn("conj", "IPersistentCollection", False)
    assert repr(pf) == "#<ProtocolFn IPersistentCollection/conj>"


def test_repr_marker_protocol():
    # Via-metadata flag set; still no impls, same rendering.
    pf = ProtocolFn("seq", "ISeqable", True)
    assert repr(pf) == "#<ProtocolFn ISeqable/seq>"


def test_native_extend_api_reachable():
    # P2.1 adds extend_with_native + registry. A direct Python-facing
    # exercise of extend_with_native requires typed fn-pointers that only
    # the macro-generated thunks produce — the real tests live in
    # test_protocol_fn_macros.py (added in P2.4). This placeholder just
    # confirms the module imports cleanly after the rebuild.
    pf = ProtocolFn("test", "Proto", False)
    assert pf is not None
