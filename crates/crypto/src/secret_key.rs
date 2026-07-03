use crate::BlsPublicKey;
use crate::signature::{BlsSignature, DST};
use blst::min_pk::SecretKey;
use rand::Rng;

pub const SECRET_KEY_BYTES_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct BlsSecretKey(pub(crate) SecretKey);

impl BlsSecretKey {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        let mut ikm = [0u8; 32];
        rng.fill(&mut ikm);
        Self(SecretKey::key_gen(&ikm, &[]).unwrap())
    }

    pub fn serialize(&self) -> [u8; SECRET_KEY_BYTES_LEN] {
        self.0.to_bytes()
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != SECRET_KEY_BYTES_LEN {
            return Err("invalid secret key length".into());
        }
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "length mismatch".to_string())?;
        SecretKey::from_bytes(&arr)
            .map(Self)
            .map_err(|e| format!("{:?}", e))
    }

    pub fn public_key(&self) -> BlsPublicKey {
        BlsPublicKey::from_secret_key(self)
    }

    pub fn sign(&self, msg: &[u8]) -> BlsSignature {
        BlsSignature(self.0.sign(msg, DST, &[]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_public_key_not_zero() {
        let sk = BlsSecretKey::random();
        let pk = sk.public_key();
        assert_ne!(pk.serialize(), [0u8; 48]);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let sk = BlsSecretKey::random();
        let bytes = sk.serialize();
        let sk2 = BlsSecretKey::deserialize(&bytes).unwrap();
        assert_eq!(sk.serialize(), sk2.serialize());
    }

    #[test]
    fn test_different_random_keys() {
        let sk1 = BlsSecretKey::random();
        let sk2 = BlsSecretKey::random();
        assert_ne!(sk1.serialize(), sk2.serialize());
    }

    #[test]
    fn test_deserialize_invalid_length() {
        let result = BlsSecretKey::deserialize(&[0u8; 10]);
        assert!(result.is_err());
    }
}
