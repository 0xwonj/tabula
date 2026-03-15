//! KoalaBear field helpers: NativeDigest, domain tags, limb encoding.

use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;

use tabula_core::Digest;
use tabula_core::error::TabulaError;

// ── Domain separation tags ──────────────────────────────────────────────────

/// SSMC commitment domain tag.
pub const DOMAIN_SSMC: u32 = 0x00;
/// SMT internal node domain tag.
pub const DOMAIN_SMT: u32 = 0x01;
/// SMT leaf (ColumnMeta) domain tag.
pub const DOMAIN_LEAF: u32 = 0x10;
/// SMT_tables node domain tag.
pub const DOMAIN_TABLE: u32 = 0x11;
/// SMT_cols node domain tag.
pub const DOMAIN_COL: u32 = 0x12;
/// Hash-IR (in-program hash instruction) domain tag.
pub const DOMAIN_HASH_IR: u32 = 0x02;

// ── SMT depth constants ─────────────────────────────────────────────────────

/// Depth for per-column data SMTs (row-level key space, 2^32 keys).
pub const COL_DATA_SMT_DEPTH: usize = 32;

/// Depth for the column-level state SMT (`SMT_cols`).
pub const COL_STATE_SMT_DEPTH: usize = 16;

/// Depth for the table-level state SMT (`SMT_tables`).
///
/// 2^30 ≈ 1B tables, sufficient. 2^31 > KoalaBear p, so depth 32 is unsafe.
pub const TABLE_STATE_SMT_DEPTH: usize = 30;

// ── NativeDigest ────────────────────────────────────────────────────────────

/// 8 KoalaBear field elements — canonical Poseidon2 output.
///
/// This is the primary hash representation inside the commitment layer.
/// Convert to `Digest` (`[u8; 32]`) at system boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NativeDigest(pub [KoalaBear; 8]);

impl Default for NativeDigest {
    fn default() -> Self {
        Self([KoalaBear::ZERO; 8])
    }
}

impl NativeDigest {
    /// The all-zeros digest.
    pub const ZERO: Self = Self([KoalaBear::ZERO; 8]);

    /// Convert to byte-level Digest (32 bytes, 4 LE bytes per FE).
    pub fn to_bytes(&self) -> Digest {
        let mut bytes = [0u8; 32];
        for (i, fe) in self.0.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&fe.as_canonical_u32().to_le_bytes());
        }
        bytes
    }

    /// Convert from byte-level Digest. Rejects non-canonical values (>= p).
    pub fn from_bytes(bytes: &Digest) -> Result<Self, TabulaError> {
        let mut fes = [KoalaBear::ZERO; 8];
        for i in 0..8 {
            let chunk: [u8; 4] = bytes[i * 4..i * 4 + 4]
                .try_into()
                .expect("slice is exactly 4 bytes");
            let val = u32::from_le_bytes(chunk);
            if val >= KoalaBear::ORDER_U32 {
                return Err(TabulaError::FieldEncodingError(format!(
                    "non-canonical KoalaBear at index {i}: {val} >= {}",
                    KoalaBear::ORDER_U32
                )));
            }
            fes[i] = KoalaBear::new(val);
        }
        Ok(NativeDigest(fes))
    }
}

// ── U64 limb encoding ───────────────────────────────────────────────────────

/// Encode a u64 into 3 KoalaBear limbs.
///
/// Decomposition (30+30+4 bits):
/// - x0 = bits \[0..30)  in \[0, 2^30)
/// - x1 = bits \[30..60) in \[0, 2^30)
/// - x2 = bits \[60..64) in \[0, 16)
///
/// All three are < p (2^30 = 1073741824 < p = 2130706433), so no modular
/// reduction occurs in KoalaBear::new(). This guarantees round-trip correctness.
///
/// See proof-spec §4.2.R for the normative definition and rationale.
pub fn encode_u64_limbs(val: u64) -> [KoalaBear; 3] {
    let x0 = (val & 0x3FFF_FFFF) as u32;
    let x1 = ((val >> 30) & 0x3FFF_FFFF) as u32;
    let x2 = (val >> 60) as u32;
    [KoalaBear::new(x0), KoalaBear::new(x1), KoalaBear::new(x2)]
}

/// Decode 3 KoalaBear limbs back to u64.
///
/// Returns an error if limb values are out of the expected ranges.
pub fn decode_u64_limbs(limbs: &[KoalaBear; 3]) -> Result<u64, TabulaError> {
    let x0 = limbs[0].as_canonical_u32();
    let x1 = limbs[1].as_canonical_u32();
    let x2 = limbs[2].as_canonical_u32();

    if x0 >= (1 << 30) {
        return Err(TabulaError::FieldEncodingError(format!(
            "limb 0 out of range: {x0} >= 2^30"
        )));
    }
    if x1 >= (1 << 30) {
        return Err(TabulaError::FieldEncodingError(format!(
            "limb 1 out of range: {x1} >= 2^30"
        )));
    }
    if x2 > 15 {
        return Err(TabulaError::FieldEncodingError(format!(
            "limb 2 out of range: {x2} > 15"
        )));
    }

    Ok((x0 as u64) | ((x1 as u64) << 30) | ((x2 as u64) << 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_digest_zero_is_default() {
        assert_eq!(NativeDigest::ZERO, NativeDigest::default());
    }

    #[test]
    fn native_digest_round_trip() {
        let fes = [
            KoalaBear::new(0),
            KoalaBear::new(1),
            KoalaBear::new(2),
            KoalaBear::new(100),
            KoalaBear::new(999_999),
            KoalaBear::new(KoalaBear::ORDER_U32 - 1),
            KoalaBear::new(0x1234_5678 % KoalaBear::ORDER_U32),
            KoalaBear::new(42),
        ];
        let digest = NativeDigest(fes);
        let bytes = digest.to_bytes();
        let recovered = NativeDigest::from_bytes(&bytes).unwrap();
        assert_eq!(digest, recovered);
    }

    #[test]
    fn native_digest_from_bytes_rejects_non_canonical() {
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&KoalaBear::ORDER_U32.to_le_bytes());
        assert!(NativeDigest::from_bytes(&bytes).is_err());
    }

    #[test]
    fn native_digest_from_bytes_rejects_u32_max() {
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(NativeDigest::from_bytes(&bytes).is_err());
    }

    #[test]
    fn encode_u64_limbs_zero() {
        let limbs = encode_u64_limbs(0);
        assert_eq!(limbs[0].as_canonical_u32(), 0);
        assert_eq!(limbs[1].as_canonical_u32(), 0);
        assert_eq!(limbs[2].as_canonical_u32(), 0);
    }

    #[test]
    fn encode_u64_limbs_max() {
        let limbs = encode_u64_limbs(u64::MAX);
        assert_eq!(limbs[0].as_canonical_u32(), 0x3FFF_FFFF); // 30 bits all 1
        assert_eq!(limbs[1].as_canonical_u32(), 0x3FFF_FFFF); // 30 bits all 1
        assert_eq!(limbs[2].as_canonical_u32(), 15); // 4 bits all 1
    }

    #[test]
    fn u64_limbs_round_trip() {
        let test_values = [
            0u64,
            1,
            42,
            u64::MAX,
            (1 << 31) - 1,
            1 << 31,
            1 << 62,
            (1 << 62) - 1,
        ];
        for val in test_values {
            let limbs = encode_u64_limbs(val);
            let recovered = decode_u64_limbs(&limbs).unwrap();
            assert_eq!(val, recovered, "round-trip failed for {val}");
        }
    }

    #[test]
    fn domain_tags_are_distinct() {
        let tags = [
            DOMAIN_SSMC,
            DOMAIN_SMT,
            DOMAIN_HASH_IR,
            DOMAIN_LEAF,
            DOMAIN_TABLE,
            DOMAIN_COL,
        ];
        for (i, &a) in tags.iter().enumerate() {
            for (j, &b) in tags.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "domain tags {i} and {j} collide");
                }
            }
        }
    }
}
