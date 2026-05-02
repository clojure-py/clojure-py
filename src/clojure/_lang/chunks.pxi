# Chunks — fixed-size batches of elements used by chunked seqs.
# Ports clojure.lang.ArrayChunk, ChunkBuffer, ChunkedCons.


cdef class ArrayChunk:
    """Concrete IChunk over a Python list slice [off, end)."""

    cdef list _array
    cdef int _off
    cdef int _end
    cdef object __weakref__

    def __cinit__(self, list array, int off=0, end=None):
        self._array = array
        self._off = off
        self._end = len(array) if end is None else end

    def nth(self, int i, not_found=NOT_FOUND):
        cdef int n = self._end - self._off
        if 0 <= i < n:
            return self._array[self._off + i]
        if not_found is NOT_FOUND:
            raise IndexError(i)
        return not_found

    def count(self):
        return self._end - self._off

    def __len__(self):
        return self._end - self._off

    def drop_first(self):
        if self._off == self._end:
            raise IndexError("dropFirst of empty chunk")
        return ArrayChunk(self._array, self._off + 1, self._end)

    def reduce(self, f, start):
        # NB: returns Reduced wrapper unwrapped — the caller (PV.reduce, etc.)
        # is expected to detect and unwrap. Java's ArrayChunk returns the
        # Reduced wrapper directly without dereffing.
        ret = f(start, self._array[self._off])
        if isinstance(ret, Reduced):
            return ret
        cdef int x
        for x in range(self._off + 1, self._end):
            ret = f(ret, self._array[x])
            if isinstance(ret, Reduced):
                return ret
        return ret


IChunk.register(ArrayChunk)
Indexed.register(ArrayChunk)
Counted.register(ArrayChunk)


cdef class ChunkBuffer:
    """Mutable builder; finalize with .chunk() to produce an ArrayChunk."""

    cdef list _buffer
    cdef int _end
    cdef object __weakref__

    def __cinit__(self, int capacity):
        self._buffer = [None] * capacity
        self._end = 0

    def add(self, o):
        if self._buffer is None:
            raise RuntimeError("ChunkBuffer already finalized")
        self._buffer[self._end] = o
        self._end += 1

    def chunk(self):
        ret = ArrayChunk(self._buffer, 0, self._end)
        self._buffer = None  # detach so adds-after-chunk fail loudly
        return ret

    def count(self):
        return self._end

    def __len__(self):
        return self._end


Counted.register(ChunkBuffer)


cdef class ChunkedCons(ASeq):
    """Prepends an IChunk to an ISeq for use in lazy-seq chains. Iterating
    walks the chunk's elements first, then the rest of the seq."""

    cdef object _chunk     # IChunk
    cdef object _more      # ISeq or None

    def __cinit__(self, chunk, more):
        self._chunk = chunk
        self._more = more

    def first(self):
        return self._chunk.nth(0)

    def next(self):
        if self._chunk.count() > 1:
            return ChunkedCons(self._chunk.drop_first(), self._more)
        return self.chunked_next()

    def more(self):
        if self._chunk.count() > 1:
            return ChunkedCons(self._chunk.drop_first(), self._more)
        if self._more is None:
            return _empty_list
        return self._more

    def chunked_first(self):
        return self._chunk

    def chunked_next(self):
        return self.chunked_more().seq()

    def chunked_more(self):
        if self._more is None:
            return _empty_list
        return self._more

    def with_meta(self, meta):
        if self._meta is meta:
            return self
        cdef ChunkedCons cc = ChunkedCons(self._chunk, self._more)
        cc._meta = meta
        return cc


IChunkedSeq.register(ChunkedCons)
