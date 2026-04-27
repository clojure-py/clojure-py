//! The fat 16-byte Value: tag (= type_id) + 64-bit payload.

use core::mem::{align_of, size_of};

pub type TypeId = u32;

pub const TYPE_NIL:        TypeId = 0;
pub const TYPE_BOOL:       TypeId = 1;
pub const TYPE_INT64:      TypeId = 2;
pub const TYPE_FLOAT64:    TypeId = 3;
pub const TYPE_CHAR:       TypeId = 4;
pub const TYPE_PYOBJECT:   TypeId = 5;
pub const FIRST_HEAP_TYPE: TypeId = 16;

#[repr(C, align(16))]
#[derive(Copy, Clone)]
pub struct Value {
    pub tag:     TypeId,
    pub _pad:    u32,
    pub payload: u64,
}

impl Value {
    pub const NIL:   Value = Value { tag: TYPE_NIL,  _pad: 0, payload: 0 };
    pub const TRUE:  Value = Value { tag: TYPE_BOOL, _pad: 0, payload: 1 };
    pub const FALSE: Value = Value { tag: TYPE_BOOL, _pad: 0, payload: 0 };

    #[inline(always)]
    pub fn int(n: i64) -> Value {
        Value { tag: TYPE_INT64, _pad: 0, payload: n as u64 }
    }

    #[inline(always)]
    pub fn float(x: f64) -> Value {
        Value { tag: TYPE_FLOAT64, _pad: 0, payload: x.to_bits() }
    }

    #[inline(always)]
    pub fn char(c: char) -> Value {
        Value { tag: TYPE_CHAR, _pad: 0, payload: c as u64 }
    }

    #[inline(always)]
    pub fn as_int(self) -> Option<i64> {
        if self.tag == TYPE_INT64 { Some(self.payload as i64) } else { None }
    }

    #[inline(always)]
    pub fn as_float(self) -> Option<f64> {
        if self.tag == TYPE_FLOAT64 { Some(f64::from_bits(self.payload)) } else { None }
    }

    #[inline(always)]
    pub fn as_bool(self) -> Option<bool> {
        if self.tag == TYPE_BOOL { Some(self.payload != 0) } else { None }
    }

    #[inline(always)]
    pub fn is_heap(self) -> bool { self.tag >= FIRST_HEAP_TYPE }

    #[inline(always)]
    pub fn is_nil(self) -> bool { self.tag == TYPE_NIL }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_layout() {
        assert_eq!(size_of::<Value>(),  16);
        assert_eq!(align_of::<Value>(), 16);
    }

    #[test]
    fn primitive_roundtrip() {
        assert_eq!(Value::int(42).as_int(), Some(42));
        assert_eq!(Value::int(-1).as_int(), Some(-1));
        assert_eq!(Value::int(i64::MAX).as_int(), Some(i64::MAX));
        assert_eq!(Value::int(i64::MIN).as_int(), Some(i64::MIN));
        assert_eq!(Value::float(3.14).as_float(), Some(3.14));
        assert_eq!(Value::char('λ').as_int(), None);
        assert_eq!(Value::TRUE.as_bool(), Some(true));
        assert_eq!(Value::FALSE.as_bool(), Some(false));
        assert!(Value::NIL.is_nil());
    }

    #[test]
    fn is_heap_only_for_heap_tags() {
        assert!(!Value::NIL.is_heap());
        assert!(!Value::int(0).is_heap());
        assert!(!Value::FALSE.is_heap());
        let heap = Value { tag: FIRST_HEAP_TYPE, _pad: 0, payload: 0xdead_beef };
        assert!(heap.is_heap());
    }
}
