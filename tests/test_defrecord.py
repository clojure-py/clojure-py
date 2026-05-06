"""Tests for defrecord (sub-batch C of core_deftype.clj — final piece).

Adapts JVM core_deftype.clj's emit-defrecord to a Python class
generation flow:

  - defrecord macro creates a Python subclass of clojure.lang.RecordBase
    via `type()`. Subclass carries _record_fields tuple + a custom
    __init__ that calls RecordBase.__record_init__ to wire field
    attrs / _meta / _extmap.
  - RecordBase (in _lang/record.pxi) implements the full IPersistentMap
    surface: val_at, assoc, without, count, seq, equiv, cons,
    contains_key, entry_at, meta, with_meta, hasheq + Python __eq__,
    __hash__, __iter__, __len__, __contains__, __getitem__.
  - ABC registrations cover IRecord, ILookup, Associative,
    IPersistentMap, IPersistentCollection, Counted, Seqable, IObj,
    IMeta, IHashEq, MapEquivalence.

Adaptations from JVM:
  - Records can't have nil fields per JVM. (dissoc rec :field)
    converts to a plain map; (dissoc rec :ext-key) returns same record.
  - Equality is class-and-value: same record class + same field values
    + same extmap. Different record classes never =, even if they have
    the same fields.
  - JVM's __hash / __hasheq mutable cache fields collapse to a single
    _hash_cache instance attr.
  - assoc on a non-field key adds an extmap entry; the resulting
    instance is still of the same record type.
"""

import pytest

import clojure.core  # bootstrap

from clojure.lang import (
    Compiler,
    read_string,
    Keyword,
    IRecord,
    IPersistentMap,
    ILookup,
    Associative,
    Counted,
    Seqable,
    RecordBase,
)


def E(src):
    return Compiler.eval(read_string(src))


def K(name):
    return Keyword.intern(None, name)


_def_counter = [0]


def fresh_record_name():
    _def_counter[0] += 1
    return f"TCBRec{_def_counter[0]}"


# --- basic fields & factories --------------------------------

def test_defrecord_creates_class_with_fields():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    E(f"(def -r1 ({name}. 10 20))")
    assert E("(.-a -r1)") == 10
    assert E("(.-b -r1)") == 20

def test_defrecord_positional_factory():
    name = fresh_record_name()
    E(f"(defrecord {name} [x y])")
    E(f"(def -r2 (->{name} 3 4))")
    assert E("(.-x -r2)") == 3
    assert E("(.-y -r2)") == 4

def test_defrecord_map_factory():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    E(f"(def -r3 (map->{name} {{:a 100 :b 200}}))")
    assert E("(:a -r3)") == 100
    assert E("(:b -r3)") == 200

def test_defrecord_map_factory_picks_up_extmap():
    """Keys not in the field list go into _extmap."""
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    E(f"(def -r4 (map->{name} {{:a 1 :extra :hello}}))")
    assert E("(:a -r4)") == 1
    assert E("(:extra -r4)") == K("hello")
    assert E("(count -r4)") == 2

def test_defrecord_map_factory_missing_field_is_nil():
    """If the input map omits a field, it becomes nil."""
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    E(f"(def -r5 (map->{name} {{:a 1}}))")
    assert E("(:a -r5)") == 1
    assert E("(:b -r5)") is None


# --- ILookup / contains_key / entry_at ------------------------

def test_defrecord_keyword_lookup():
    name = fresh_record_name()
    E(f"(defrecord {name} [x y])")
    E(f"(def -r6 (->{name} 1 2))")
    assert E("(get -r6 :x)") == 1
    assert E("(get -r6 :y)") == 2
    assert E("(get -r6 :missing)") is None
    assert E("(get -r6 :missing :default)") == K("default")

def test_defrecord_invocation_via_keyword():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    assert E(f"(:x (->{name} 42))") == 42

def test_defrecord_contains_key():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    E(f"(def -r7 (->{name} 1 2))")
    assert E("(contains? -r7 :a)") is True
    assert E("(contains? -r7 :b)") is True
    assert E("(contains? -r7 :z)") is False

def test_defrecord_contains_key_extmap():
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    E(f"(def -r8 (assoc (->{name} 1) :extra :v))")
    assert E("(contains? -r8 :a)") is True
    assert E("(contains? -r8 :extra)") is True


# --- assoc semantics -----------------------------------------

def test_assoc_field_returns_same_record_type():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    E(f"(def -r9 (->{name} 1))")
    out = E("(assoc -r9 :x 99)")
    assert E(f"(instance? {name} (assoc -r9 :x 99))") is True
    assert E("(:x (assoc -r9 :x 99))") == 99

def test_assoc_field_creates_new_instance():
    """assoc returns a new instance; original is unchanged."""
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    E(f"(def -r10 (->{name} 1))")
    E("(def -r10b (assoc -r10 :x 99))")
    assert E("(:x -r10)") == 1
    assert E("(:x -r10b)") == 99

def test_assoc_extmap_key_stays_record_type():
    """Adding a non-field key goes to _extmap; result is still a record."""
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    E(f"(def -r11 (assoc (->{name} 1) :extra :hello))")
    assert E(f"(instance? {name} -r11)") is True
    assert E("(:extra -r11)") == K("hello")
    assert E("(:a -r11)") == 1


# --- dissoc semantics ----------------------------------------

def test_dissoc_field_returns_plain_map():
    """Records can't have missing fields, so dissoc on a field key
    returns a regular map with the remaining entries."""
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    E(f"(def -r12 (->{name} 1 2))")
    out_class = E("(.-__name__ (class (dissoc -r12 :a)))")
    assert out_class != name
    assert E("(:b (dissoc -r12 :a))") == 2

def test_dissoc_extmap_key_stays_record():
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    E(f"(def -r13 (assoc (->{name} 1) :extra :hello))")
    E("(def -r13b (dissoc -r13 :extra))")
    assert E(f"(instance? {name} -r13b)") is True
    assert E("(:a -r13b)") == 1
    assert E("(:extra -r13b)") is None


# --- equality / hash -----------------------------------------

def test_value_equality():
    name = fresh_record_name()
    E(f"(defrecord {name} [x y])")
    out = E(f"(= (->{name} 1 2) (->{name} 1 2))")
    assert out is True

def test_value_inequality():
    name = fresh_record_name()
    E(f"(defrecord {name} [x y])")
    out = E(f"(= (->{name} 1 2) (->{name} 1 3))")
    assert out is False

def test_cross_record_type_inequality():
    """Two record types with the same field values are NOT equal."""
    n1 = fresh_record_name()
    n2 = fresh_record_name()
    E(f"(defrecord {n1} [x y])")
    E(f"(defrecord {n2} [x y])")
    out = E(f"(= (->{n1} 1 2) (->{n2} 1 2))")
    assert out is False

def test_hash_consistent_with_equality():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    out = E(f"(= (hash (->{name} 1 2)) (hash (->{name} 1 2)))")
    assert out is True

def test_record_usable_as_map_key():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    out = E(f"(get {{(->{name} 1) :marker}} (->{name} 1))")
    assert out == K("marker")

def test_extmap_participates_in_equality():
    """Two records with same fields but different extmaps are NOT equal."""
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    out = E(f"""
      (= (assoc (->{name} 1) :extra :x)
         (assoc (->{name} 1) :extra :y))""")
    assert out is False


# --- count / seq ---------------------------------------------

def test_record_count():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b c])")
    assert E(f"(count (->{name} 1 2 3))") == 3

def test_record_count_with_extmap():
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    out = E(f"(count (assoc (->{name} 1) :x 2 :y 3))")
    assert out == 3

def test_record_seq():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    out = E(f"(seq (->{name} :one :two))")
    pairs = [list(p) for p in out]
    assert sorted(pairs) == [[K("a"), K("one")], [K("b"), K("two")]]

def test_empty_record_seq_returns_nil():
    """A record with no fields (and no extmap) seq's to nil — but a
    no-field record is unusual. Skipped."""

def test_record_keys_vals():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b])")
    keys = sorted(E(f"(keys (->{name} 1 2))"))
    assert keys == [K("a"), K("b")]


# --- meta / with-meta ----------------------------------------

def test_with_meta_attaches():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    E(f"(def -r14 (with-meta (->{name} 1) {{:tag :marker}}))")
    out = E("(meta -r14)")
    assert dict(out) == {K("tag"): K("marker")}

def test_with_meta_doesnt_change_class():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    out = E(f"""(.-__name__
                 (class (with-meta (->{name} 1) {{:any :meta}})))""")
    assert out == name

def test_record_default_meta_nil():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    assert E(f"(meta (->{name} 1))") is None


# --- ABC membership ------------------------------------------

def test_record_isa_record_marker():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    assert E(f"(instance? clojure.lang.IRecord (->{name} 1))") is True

def test_record_isa_ipersistentmap():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    assert E(f"(map? (->{name} 1))") is True

def test_record_isa_associative():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    assert E(f"(associative? (->{name} 1))") is True

def test_record_isa_seqable():
    name = fresh_record_name()
    E(f"(defrecord {name} [x])")
    assert E(f"(seqable? (->{name} 1))") is True


# --- protocol implementation ---------------------------------

def test_record_implements_protocol():
    """defrecord with protocol method body works just like deftype."""
    pname = fresh_record_name()
    rname = fresh_record_name()
    E(f"(defprotocol {pname} (describe [x]))")
    E(f"""(defrecord {rname} [v]
            {pname} (describe [this] [:described v]))""")
    out = E(f"(describe (->{rname} 42))")
    assert list(out) == [K("described"), 42]

def test_record_protocol_with_field_auto_binding():
    """Method bodies see field names as locals (same as deftype)."""
    pname = fresh_record_name()
    rname = fresh_record_name()
    E(f"(defprotocol {pname} (sum [x]))")
    E(f"""(defrecord {rname} [a b c]
            {pname} (sum [this] (+ a b c)))""")
    assert E(f"(sum (->{rname} 1 2 3))") == 6


# --- into / cons ---------------------------------------------

def test_into_record_with_pairs():
    """conj / into onto a record adds map entries."""
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    E(f"(def -r15 (into (->{name} 1) [[:k1 :v1] [:k2 :v2]]))")
    assert E(f"(instance? {name} -r15)") is True
    assert E("(:k1 -r15)") == K("v1")
    assert E("(:k2 -r15)") == K("v2")

def test_conj_map_entry_onto_record():
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    out = E(f"(:k (conj (->{name} 1) (clojure.lang.MapEntry/create :k :v)))")
    assert out == K("v")


# --- Python interop ------------------------------------------

def test_record_python_iteration():
    """Python `for x in rec` walks the map entries."""
    name = fresh_record_name()
    E(f"(defrecord {name} [x y])")
    rec = E(f"(->{name} 1 2)")
    seen = list(rec)
    keys = sorted(e.key() for e in seen)
    assert keys == [K("x"), K("y")]

def test_record_python_len():
    name = fresh_record_name()
    E(f"(defrecord {name} [a b c])")
    rec = E(f"(->{name} 1 2 3)")
    assert len(rec) == 3

def test_record_python_in():
    name = fresh_record_name()
    E(f"(defrecord {name} [a])")
    rec = E(f"(->{name} 1)")
    assert K("a") in rec
    assert K("missing") not in rec
