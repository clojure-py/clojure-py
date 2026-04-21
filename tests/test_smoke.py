import clojure
from clojure import _core

def test_extension_loads():
    assert _core is not None
    assert hasattr(_core, "__name__")
