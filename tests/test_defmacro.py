"""User-defined macros via defmacro."""

from clojure._core import eval_string


def test_unless_macro():
    eval_string("(defmacro unless [c & body] (list (quote if) c nil (cons (quote do) body)))")
    assert eval_string("(unless false :yes)").name == "yes"
    assert eval_string("(unless true :yes)") is None


def test_macro_returns_cons_expansion():
    # `(cons 'do ...)` produces a Cons, not a PersistentList. The compiler
    # must handle non-list seq forms as call forms.
    eval_string("(defmacro wrap-do [& body] (cons (quote do) body))")
    assert eval_string("(wrap-do 1 2 3)") == 3


def test_nested_macro_invocations():
    eval_string("(defmacro dbl-macro-xx [x] (list (quote +) x x))")
    eval_string("(defmacro quad-macro-xx [x] (list (quote dbl-macro-xx) (list (quote dbl-macro-xx) x)))")
    assert eval_string("(quad-macro-xx 5)") == 20


def test_macro_persists_across_eval_calls():
    eval_string("(defmacro later [x] (list (quote +) x 100))")
    assert eval_string("(later 1)") == 101


def test_macro_sees_form_and_env():
    # &form is the full macro call. Return it as a list so we can inspect.
    eval_string("(defmacro show-form [_] (list (quote quote) &form))")
    # (show-form 42) → '(show-form 42), which evals to itself.
    result = eval_string("(show-form 42)")
    # result is a PersistentList ((quote show-form) 42) printed as (show-form 42).
    # Coerce to str for a loose check.
    assert "show-form" in str(result)
    assert "42" in str(result)
