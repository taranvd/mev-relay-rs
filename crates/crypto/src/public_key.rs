use crate::BlsSecretKey;
use blst::min_pk::PublicKey;

pub const PUBLIC_KEY_BYTES_LEN: usize = 48;

#[derive(Debug, Clone, PartialEq)]
pub struct BlsPublicKey(pub(crate) PublicKey);

impl BlsPublicKey {
    pub fn serialize(&self) -> [u8; PUBLIC_KEY_BYTES_LEN] {
        self.0.compress()
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != PUBLIC_KEY_BYTES_LEN {
            return Err("invalid length".into());
        }

        PublicKey::key_validate(bytes)
            .map(Self)
            .map_err(|e| format!("{:?}", e))
    }

    pub fn from_secret_key(sk: &BlsSecretKey) -> Self {
        Self(sk.0.sk_to_pk())
    }
}

impl Eq for BlsPublicKey {}

impl std::hash::Hash for BlsPublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.serialize().hash(state);
    }
}

impl serde::Serialize for BlsPublicKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hex_str = alloy_primitives::hex::encode_prefixed(self.serialize());
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> serde::Deserialize<'de> for BlsPublicKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = alloy_primitives::hex::decode(&s)
            .map_err(serde::de::Error::custom)?;
        Self::deserialize(&bytes).map_err(serde::de::Error::custom)
    }
}

impl tree_hash::TreeHash for BlsPublicKey {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        <[u8; PUBLIC_KEY_BYTES_LEN] as tree_hash::TreeHash>::tree_hash_type()
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        self.serialize().tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        <[u8; PUBLIC_KEY_BYTES_LEN] as tree_hash::TreeHash>::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        self.serialize().tree_hash_root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlsSecretKey;

    #[test]
    fn test_serialize_roundtrip() {
        let sk = BlsSecretKey::random();
        let pk = sk.public_key();
        let bytes = pk.serialize();
        let pk2 = BlsPublicKey::deserialize(&bytes).unwrap();
        assert_eq!(pk, pk2);
    }

    #[test]
    fn test_deserialize_invalid_length() {
        let result = BlsPublicKey::deserialize(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_zero_bytes() {
        let result = BlsPublicKey::deserialize(&[0u8; 48]);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys_different_serialization() {
        let pk1 = BlsSecretKey::random().public_key();
        let pk2 = BlsSecretKey::random().public_key();
        assert_ne!(pk1.serialize(), pk2.serialize());
    }

    #[test]
    fn test_from_secret_key_consistency() {
        let sk = BlsSecretKey::random();
        let pk1 = sk.public_key();
        let pk2 = BlsPublicKey::from_secret_key(&sk);
        assert_eq!(pk1, pk2);
    }
}
