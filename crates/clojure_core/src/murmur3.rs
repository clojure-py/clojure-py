//! Port of `clojure.lang.Murmur3` (MurmurHash3 x86_32 variant) for use by
//! Clojure-style `hasheq`. All ops are i32-precision with wrapping arithmetic
//! to match the JVM bit-for-bit.

const SEED: u32 = 0;
const C1: u32 = 0xcc9e2d51;
const C2: u32 = 0x1b873593;

#[inline]
fn mix_k1(mut k1: u32) -> u32 {
    k1 = k1.wrapping_mul(C1);
    k1 = k1.rotate_left(15);
    k1 = k1.wrapping_mul(C2);
    k1
}

#[inline]
fn mix_h1(mut h1: u32, k1: u32) -> u32 {
    h1 ^= k1;
    h1 = h1.rotate_left(13);
    h1 = h1.wrapping_mul(5).wrapping_add(0xe6546b64);
    h1
}

#[inline]
fn fmix(mut h1: u32, length: u32) -> u32 {
    h1 ^= length;
    h1 ^= h1 >> 16;
    h1 = h1.wrapping_mul(0x85ebca6b);
    h1 ^= h1 >> 13;
    h1 = h1.wrapping_mul(0xc2b2ae35);
    h1 ^= h1 >> 16;
    h1
}

pub fn hash_int(input: i32) -> i32 {
    if input == 0 {
        return 0;
    }
    let k1 = mix_k1(input as u32);
    let h1 = mix_h1(SEED, k1);
    fmix(h1, 4) as i32
}

pub fn hash_long(input: i64) -> i32 {
    if input == 0 {
        return 0;
    }
    let low = input as u32;
    let high = ((input as u64) >> 32) as u32;
    let k1 = mix_k1(low);
    let h1 = mix_h1(SEED, k1);
    let k1 = mix_k1(high);
    let h1 = mix_h1(h1, k1);
    fmix(h1, 8) as i32
}

/// Vanilla `Murmur3.mixCollHash` — finalizes a per-element accumulator into
/// the collection-level hash code consumed by `hash-ordered-coll` /
/// `hash-unordered-coll`.
pub fn mix_coll_hash(hash: i32, count: i32) -> i32 {
    let h1 = SEED;
    let k1 = mix_k1(hash as u32);
    let h1 = mix_h1(h1, k1);
    fmix(h1, count as u32) as i32
}

/// Vanilla `Murmur3.hashOrdered` — collection hash for ordered collections
/// (lists, vectors, seqs). Walks the collection via `seq`/`first`/`next_`,
/// folds `31 * acc + hasheq(elem)` in i32 wrapping arithmetic, then mixes.
pub fn hash_ordered_seq(py: pyo3::Python<'_>, coll: pyo3::Py<pyo3::types::PyAny>) -> pyo3::PyResult<i32> {
    let mut h: i32 = 1;
    let mut n: i32 = 0;
    let mut cur = crate::rt::seq(py, coll)?;
    while !cur.is_none(py) {
        let f = crate::rt::first(py, cur.clone_ref(py))?;
        let fh = crate::rt::hash_eq(py, f)? as i32;
        h = h.wrapping_mul(31).wrapping_add(fh);
        n = n.wrapping_add(1);
        cur = crate::rt::next_(py, cur)?;
    }
    Ok(mix_coll_hash(h, n))
}

/// Vanilla `Murmur3.hashUnordered` — collection hash for unordered
/// collections (sets, maps). Sum of element hash_eq in i32 wrapping, then mixes.
pub fn hash_unordered_seq(py: pyo3::Python<'_>, coll: pyo3::Py<pyo3::types::PyAny>) -> pyo3::PyResult<i32> {
    let mut h: i32 = 0;
    let mut n: i32 = 0;
    let mut cur = crate::rt::seq(py, coll)?;
    while !cur.is_none(py) {
        let f = crate::rt::first(py, cur.clone_ref(py))?;
        let fh = crate::rt::hash_eq(py, f)? as i32;
        h = h.wrapping_add(fh);
        n = n.wrapping_add(1);
        cur = crate::rt::next_(py, cur)?;
    }
    Ok(mix_coll_hash(h, n))
}
