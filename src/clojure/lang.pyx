# cython: language_level=3
# cython: freethreading_compatible=True

# clojure.lang — single Cython extension that aggregates the entire
# clojure.lang.* port. Each Java class becomes a section (.pxi) included here.
# Order matters when later pieces reference earlier ones.

from libc.stdint cimport int32_t, uint32_t
from decimal import Decimal
from threading import Lock

include "_lang/interfaces.pxi"
include "_lang/murmur3.pxi"
include "_lang/hash_helpers.pxi"
include "_lang/bigint.pxi"
include "_lang/bigdecimal.pxi"
include "_lang/ratio.pxi"
include "_lang/numbers.pxi"
include "_lang/util.pxi"
include "_lang/symbol.pxi"
include "_lang/keyword.pxi"
include "_lang/empty_list.pxi"
include "_lang/aseq.pxi"
include "_lang/cons.pxi"
include "_lang/iterator_seq.pxi"
include "_lang/lazy_seq.pxi"
include "_lang/range.pxi"
include "_lang/iterate_cycle_repeat.pxi"
include "_lang/reduced.pxi"
include "_lang/persistent_list.pxi"
include "_lang/chunks.pxi"
include "_lang/persistent_vector.pxi"
include "_lang/map_entry.pxi"
include "_lang/persistent_array_map.pxi"
