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
