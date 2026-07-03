use crate::BlsPublicKey;
use blst::BLST_ERROR::BLST_SUCCESS;
use blst::min_pk::Signature;

/// The byte-length of a BLS signature when serialized in compressed form.
pub const SIGNATURE_BYTES_LEN: usize = 96;

/// The domain separation tag used in BLS domain separation.
pub const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

#[derive(Clone)]
pub struct BlsSignature(pub(crate) blst::min_pk::Signature);

impl std::fmt::Debug for BlsSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BlsSignature({})",
            alloy_primitives::hex::encode_prefixed(self.serialize())
        )
    }
}

impl serde::Serialize for BlsSignature {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hex_str = alloy_primitives::hex::encode_prefixed(self.serialize());
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> serde::Deserialize<'de> for BlsSignature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = alloy_primitives::hex::decode(&s).map_err(serde::de::Error::custom)?;
        Self::deserialize(&bytes).map_err(serde::de::Error::custom)
    }
}

impl BlsSignature {
    pub fn serialize(&self) -> [u8; 96] {
        self.0.compress()
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != SIGNATURE_BYTES_LEN {
            return Err("invalid signature bytes length".into());
        }

        let arr: [u8; 96] = bytes
            .try_into()
            .map_err(|_| "length mismatch".to_string())?;

        Signature::from_bytes(&arr)
            .map(Self)
            .map_err(|e| format!("{:?}", e))
    }

    pub fn verify(&self, pubkey: &BlsPublicKey, msg: &[u8]) -> bool {
        self.0.verify(true, msg, DST, &[], &pubkey.0, true) == BLST_SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlsSecretKey;

    #[test]
    fn test_verify_correct() {
        let sk = BlsSecretKey::random();
        let pk = sk.public_key();
        let msg = b"hello world";
        let sig = BlsSignature(sk.0.sign(msg, DST, &[]));

        assert!(sig.verify(&pk, msg));
    }

    #[test]
    fn test_verify_wrong_key() {
        let sk = BlsSecretKey::random();
        let msg = b"some message";
        let sig = BlsSignature(sk.0.sign(msg, DST, &[]));
        let other_pk = BlsSecretKey::random().public_key();
        assert!(!sig.verify(&other_pk, msg));
    }

    #[test]
    fn test_serialize_round_trip() {
        let sk = BlsSecretKey::random();
        let msg = b"roundtrip_test";
        let sig = BlsSignature(sk.0.sign(msg, DST, &[]));
        let pk = sk.public_key();
        let bytes = sig.serialize();
        let sig2 = BlsSignature::deserialize(&bytes).unwrap();
        assert!(sig2.verify(&pk, msg));
    }

    #[test]
    fn test_deserialize_invalid_length() {
        let result = BlsSignature::deserialize(&[0u8; 10]);
        assert!(result.is_err());
    }
}
