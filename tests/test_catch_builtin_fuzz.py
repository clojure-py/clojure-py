"""Property-based fuzzing of bare-`Exception` catch."""

from hypothesis import given, strategies as st
from clojure._core import eval_string as e


# All exception classes that should be caught by bare `Exception`.
exc_classes = st.sampled_from([
    "clojure._core/IllegalStateException",
    "clojure._core/IllegalArgumentException",
    "clojure._core/ArityException",
    "builtins/ValueError",
    "builtins/TypeError",
    "builtins/RuntimeError",
    "builtins/ZeroDivisionError",
    "builtins/StopIteration",
])


@given(cls=exc_classes, msg=st.text(max_size=20))
def test_exception_catches_all_project_exception_classes(cls, msg):
    msg_lit = msg.replace("\\", "\\\\").replace('"', '\\"')
    src = f'''
    (try
      (throw ({cls} "{msg_lit}"))
      (catch Exception ex :caught))
    '''
    assert e(src) == e(":caught")
