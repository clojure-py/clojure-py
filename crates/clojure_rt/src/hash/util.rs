//! Hash combination helpers, separate from Murmur3 because they're a
//! different algorithm family (Boost-style XOR mixing).

/// Boost-style hash combine: `seed ^= h + 0x9e3779b9 + (seed << 6) + (seed >> 2)`.
/// Matches `clojure.lang.Util.hashCombine`. Used by Symbol/Keyword to
/// mix component hashes (name + namespace) into a single value.
#[inline]
pub fn hash_combine(seed: i32, hash: i32) -> i32 {
    let s = seed as u32;
    let h = hash as u32;
    let combined = s
        ^ h.wrapping_add(0x9e3779b9_u32)
            .wrapping_add(s << 6)
            .wrapping_add(s >> 2);
    combined as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_combine_is_deterministic() {
        assert_eq!(hash_combine(1, 2), hash_combine(1, 2));
    }

    #[test]
    fn hash_combine_is_not_commutative() {
        // Sanity check that the operation distinguishes argument order
        // (otherwise it'd be a poor combine for ordered pairs like
        // (name, namespace)).
        assert_ne!(hash_combine(1, 2), hash_combine(2, 1));
    }

    #[test]
    fn hash_combine_zero_zero_is_golden_ratio() {
        // (0 ^ (0 + 0x9e3779b9 + 0 + 0)) = 0x9e3779b9
        assert_eq!(hash_combine(0, 0), 0x9e3779b9_u32 as i32);
    }
}
