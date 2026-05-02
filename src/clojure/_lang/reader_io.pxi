# PushbackReader + LineNumberingPushbackReader.
#
# Java has java.io.PushbackReader / LineNumberReader in the stdlib; Python
# does not. Both classes here wrap a Python text stream (anything with a
# .read(n) method) and present a per-character API with arbitrary unread
# pushback plus line/column tracking.
#
# CR / LF / CRLF are normalized to a single '\n' the way Java's
# LineNumberReader does.


cdef class PushbackReader:
    """One-character (or more) pushback over a Python text stream."""

    cdef object _stream
    cdef list _pushback
    cdef object __weakref__

    def __cinit__(self, stream):
        self._stream = stream
        self._pushback = []

    cdef str _raw_read(self):
        """Read one char from the underlying stream, or '' for EOF."""
        return self._stream.read(1)

    def read(self):
        """Return the next character, or '' at EOF.

        CR, LF, and CRLF are all collapsed to a single '\\n'."""
        cdef str ch
        if self._pushback:
            return self._pushback.pop()
        ch = self._raw_read()
        if ch == "\r":
            nxt = self._raw_read()
            if nxt == "\n" or nxt == "":
                ch = "\n"
            else:
                self._pushback.append(nxt)
                ch = "\n"
        return ch

    def unread(self, ch):
        if ch == "" or ch is None:
            raise ValueError("Cannot unread EOF")
        self._pushback.append(ch)

    def close(self):
        if hasattr(self._stream, "close"):
            self._stream.close()


cdef class LineNumberingPushbackReader(PushbackReader):
    """Tracks line and column numbers (1-based) and supports a capture-string
    mode used by the reader for source preservation."""

    cdef int _line_number
    cdef int _column_number
    cdef bint _at_line_start
    cdef bint _prev_at_line_start
    cdef int _prev_line_number
    cdef int _prev_column_number
    cdef list _capture_chars

    def __cinit__(self, stream):
        # PushbackReader.__cinit__ has run.
        self._line_number = 1
        self._column_number = 1
        self._at_line_start = True
        self._prev_at_line_start = False
        self._prev_line_number = 1
        self._prev_column_number = 1
        self._capture_chars = None

    def read(self):
        cdef str ch = PushbackReader.read(self)
        self._prev_at_line_start = self._at_line_start
        self._prev_line_number = self._line_number
        self._prev_column_number = self._column_number
        if ch == "\n" or ch == "":
            if ch == "\n":
                self._line_number += 1
            self._at_line_start = True
            self._column_number = 1
        else:
            self._at_line_start = False
            self._column_number += 1
        if self._capture_chars is not None and ch != "":
            self._capture_chars.append(ch)
        return ch

    def unread(self, ch):
        PushbackReader.unread(self, ch)
        self._at_line_start = self._prev_at_line_start
        self._line_number = self._prev_line_number
        self._column_number = self._prev_column_number
        if self._capture_chars is not None and self._capture_chars:
            self._capture_chars.pop()

    def get_line_number(self):
        return self._line_number

    def set_line_number(self, n):
        self._line_number = n

    def get_column_number(self):
        return self._column_number

    def at_line_start(self):
        return self._at_line_start

    def capture_string(self):
        self._capture_chars = []

    def get_string(self):
        if self._capture_chars is None:
            return None
        ret = "".join(self._capture_chars)
        self._capture_chars = None
        return ret

    def read_line(self):
        """Read characters up to (and consuming) the next newline. Returns
        the line content without the newline, or None at EOF."""
        cdef str ch
        cdef list buf = []
        while True:
            ch = self.read()
            if ch == "":
                return None if not buf else "".join(buf)
            if ch == "\n":
                return "".join(buf)
            buf.append(ch)


def reader_from_string(s):
    """Convenience: a LineNumberingPushbackReader over a string."""
    import io
    return LineNumberingPushbackReader(io.StringIO(s))
