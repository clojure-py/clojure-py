# clojure.lang.* interface ABCs.
#
# Each I* / single-method-marker interface in clojure.lang becomes an
# abc.ABC subclass with @abstractmethod stubs. The hierarchy mirrors the
# Java `extends` / `implements` graph exactly, including diamonds.
#
# Method names are snake_case'd (Pythonic), so Java's `valAt` -> `val_at`,
# `assocEx` -> `assoc_ex`, etc. The class names stay PascalCase to match Java.
#
# A module-level sentinel `NOT_FOUND` distinguishes "no default supplied"
# from "default was None" — used by ILookup.val_at and Indexed.nth, the
# two methods Java models with overloaded signatures.

from abc import ABC, abstractmethod


NOT_FOUND = object()


# --- markers (no methods) ---

class Sequential(ABC):
    """Marker. Sequential collections (lists, vectors, seqs) participate in
    sequential equality. Maps and sets do not."""


class IRecord(ABC):
    """Marker for defrecord-generated types."""


class IType(ABC):
    """Marker for deftype-generated types."""


class MapEquivalence(ABC):
    """Marker. Non-IPersistentMap collections that are still map-equal to maps."""


# --- single-method interfaces ---

class Counted(ABC):
    @abstractmethod
    def count(self): ...

    def __len__(self):
        return self.count()


class Seqable(ABC):
    @abstractmethod
    def seq(self): ...


class IHashEq(ABC):
    @abstractmethod
    def hasheq(self): ...


class IMeta(ABC):
    @abstractmethod
    def meta(self): ...


class Named(ABC):
    @abstractmethod
    def get_namespace(self): ...

    @abstractmethod
    def get_name(self): ...


class Reversible(ABC):
    @abstractmethod
    def rseq(self): ...


class Settable(ABC):
    @abstractmethod
    def do_set(self, val): ...

    @abstractmethod
    def do_reset(self, val): ...


class IDeref(ABC):
    @abstractmethod
    def deref(self): ...


class IBlockingDeref(ABC):
    @abstractmethod
    def deref(self, ms, timeout_value): ...


class IReference(IMeta):
    """A reference type that supports altering / resetting its metadata."""

    @abstractmethod
    def alter_meta(self, alter, args): ...

    @abstractmethod
    def reset_meta(self, m): ...


class IRef(IDeref):
    """A reference type with validator + watch support (Atom, Var, Ref, Agent)."""

    @abstractmethod
    def set_validator(self, vf): ...

    @abstractmethod
    def get_validator(self): ...

    @abstractmethod
    def get_watches(self): ...

    @abstractmethod
    def add_watch(self, key, callback): ...

    @abstractmethod
    def remove_watch(self, key): ...


class IAtom(ABC):
    """An atomic, validated, watch-able cell. Java has 4 overloads of swap;
    we collapse to one *args form."""

    @abstractmethod
    def swap(self, f, *args): ...

    @abstractmethod
    def compare_and_set(self, oldv, newv): ...

    @abstractmethod
    def reset(self, newval): ...


class IAtom2(IAtom):
    """IAtom plus the *vals variants that return [old, new] vectors."""

    @abstractmethod
    def swap_vals(self, f, *args): ...

    @abstractmethod
    def reset_vals(self, newv): ...


class IPending(ABC):
    @abstractmethod
    def is_realized(self): ...


class IDrop(ABC):
    @abstractmethod
    def drop(self, n): ...


class IChunk(ABC):
    """A counted, indexed chunk of values — typically a backing array slice
    used for chunked seq traversal."""

    @abstractmethod
    def nth(self, i, not_found=NOT_FOUND): ...

    @abstractmethod
    def count(self): ...

    @abstractmethod
    def drop_first(self): ...

    @abstractmethod
    def reduce(self, f, start): ...


class IReduceInit(ABC):
    @abstractmethod
    def reduce(self, f, start): ...


class IKVReduce(ABC):
    @abstractmethod
    def kv_reduce(self, f, init): ...


# --- multi-method ---

class Indexed(Counted):
    @abstractmethod
    def nth(self, i, not_found=NOT_FOUND):
        """Java has nth(i) and nth(i, notFound). Combined with sentinel:
        when not_found is NOT_FOUND, raise IndexError on out-of-range;
        otherwise return not_found."""


class IObj(IMeta):
    @abstractmethod
    def with_meta(self, meta): ...


class IFn(ABC):
    """Java IFn has invoke(arg0..arg20) plus applyTo(ISeq). In Python we
    collapse to __call__(*args). apply_to is a default impl that walks an
    ISeq into args and forwards to __call__; concrete impls may override
    for performance."""

    @abstractmethod
    def __call__(self, *args): ...

    def apply_to(self, arglist):
        args = []
        s = arglist.seq() if arglist is not None else None
        while s is not None:
            args.append(s.first())
            s = s.next()
        return self(*args)


class ILookup(ABC):
    @abstractmethod
    def val_at(self, key, not_found=NOT_FOUND):
        """Java has valAt(k) and valAt(k, notFound). Combined: when
        not_found is NOT_FOUND, returns None for missing; otherwise
        returns not_found. (Distinct from Indexed.nth which raises.)"""


class Sorted(ABC):
    @abstractmethod
    def comparator(self): ...

    @abstractmethod
    def entry_key(self, entry): ...

    @abstractmethod
    def seq_with_comparator(self, ascending): ...

    @abstractmethod
    def seq_from(self, key, ascending): ...


class IMapEntry(ABC):
    @abstractmethod
    def key(self): ...

    @abstractmethod
    def val(self): ...


class IPersistentCollection(Seqable):
    @abstractmethod
    def count(self): ...

    @abstractmethod
    def cons(self, o): ...

    @abstractmethod
    def empty(self): ...

    @abstractmethod
    def equiv(self, o): ...


class ISeq(IPersistentCollection):
    @abstractmethod
    def first(self): ...

    @abstractmethod
    def next(self): ...

    @abstractmethod
    def more(self): ...


class IReduce(IReduceInit):
    @abstractmethod
    def reduce(self, f, start=NOT_FOUND):
        """Java has reduce(f) (no init) and reduce(f, start). Combined:
        when start is NOT_FOUND, reduce without init seed."""


class IPersistentStack(IPersistentCollection):
    @abstractmethod
    def peek(self): ...

    @abstractmethod
    def pop(self): ...


class IChunkedSeq(ISeq, Sequential):
    """ISeq that exposes its underlying chunks for batched traversal."""

    @abstractmethod
    def chunked_first(self): ...

    @abstractmethod
    def chunked_next(self): ...

    @abstractmethod
    def chunked_more(self): ...


class Associative(IPersistentCollection, ILookup):
    @abstractmethod
    def contains_key(self, key): ...

    @abstractmethod
    def entry_at(self, key): ...

    @abstractmethod
    def assoc(self, key, val): ...


class IPersistentList(Sequential, IPersistentStack):
    pass


class IPersistentVector(Associative, Sequential, IPersistentStack, Reversible, Indexed, IFn):
    @abstractmethod
    def length(self): ...

    @abstractmethod
    def assoc_n(self, i, val): ...


class IPersistentMap(Associative, Counted):
    @abstractmethod
    def assoc_ex(self, key, val): ...

    @abstractmethod
    def without(self, key): ...


class IPersistentSet(IPersistentCollection, Counted):
    @abstractmethod
    def disjoin(self, key): ...

    @abstractmethod
    def contains(self, key): ...

    @abstractmethod
    def get(self, key): ...


# --- transients ---

class ITransientCollection(ABC):
    @abstractmethod
    def conj(self, val): ...

    @abstractmethod
    def persistent(self): ...


class ITransientAssociative(ITransientCollection, ILookup):
    @abstractmethod
    def assoc(self, key, val): ...


class ITransientAssociative2(ITransientAssociative):
    @abstractmethod
    def contains_key(self, key): ...

    @abstractmethod
    def entry_at(self, key): ...


class ITransientMap(ITransientAssociative, Counted):
    @abstractmethod
    def without(self, key): ...


class ITransientSet(ITransientCollection, Counted):
    @abstractmethod
    def disjoin(self, key): ...

    @abstractmethod
    def contains(self, key): ...

    @abstractmethod
    def get(self, key): ...


class ITransientVector(ITransientAssociative, Indexed):
    @abstractmethod
    def assoc_n(self, i, val): ...

    @abstractmethod
    def pop(self): ...


class IEditableCollection(ABC):
    @abstractmethod
    def as_transient(self): ...


class IExceptionInfo(ABC):
    """Marker interface for exceptions that carry a data map."""

    @abstractmethod
    def getData(self): ...
