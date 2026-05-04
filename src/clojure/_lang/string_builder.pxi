# Stand-in for java.lang.StringBuilder — mutable string buffer used by
# clojure.core/str's variadic arity.
#
# Only the surface core.clj reaches for: .append (returns self for
# chaining) and .toString (returns the joined string). Python's __str__
# also returns the joined string for ergonomic interop.


class StringBuilder:
    """Mirrors the JVM java.lang.StringBuilder API used by core.clj."""

    __slots__ = ("_parts",)

    def __init__(self, s=""):
        self._parts = [s] if s else []

    def append(self, s):
        if s is None:
            self._parts.append("")
        else:
            self._parts.append(s if isinstance(s, str) else str(s))
        return self

    def __str__(self):
        return "".join(self._parts)

    def toString(self):
        return "".join(self._parts)
