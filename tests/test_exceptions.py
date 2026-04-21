import pytest
from clojure._core import (
    ArityException, IllegalStateException, IllegalArgumentException,
)

def test_arity_exception_is_subclass_of_typeerror():
    assert issubclass(ArityException, TypeError)

def test_arity_exception_message():
    with pytest.raises(ArityException) as ei:
        raise ArityException("Wrong number of args (3) passed to: foo")
    assert "Wrong number of args" in str(ei.value)

def test_illegal_state_exception():
    assert issubclass(IllegalStateException, RuntimeError)

def test_illegal_argument_exception():
    assert issubclass(IllegalArgumentException, ValueError)
