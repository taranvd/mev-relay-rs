use relay_crypto::{BlsPublicKey, BlsSecretKey, BlsSignature, ForkDatas, SignedRoot};

#[derive(Debug, Clone)]
pub struct BlsSigner {
    secret_key: BlsSecretKey,
    fork_data: ForkDatas,
}

impl Default for BlsSigner {
    fn default() -> Self {
        Self {
            secret_key: BlsSecretKey::random(),
            fork_data: ForkDatas::default(),
        }
    }
}

impl BlsSigner {
    pub fn new(secret_key: BlsSecretKey, fork_data: ForkDatas) -> Self {
        Self {
            secret_key,
            fork_data,
        }
    }

    pub fn sign<T: SignedRoot>(&self, msg: &T) -> BlsSignature {
        let domain = self.fork_data.compute_builder_domain();
        let signing_root = msg.signing_root(domain.into());
        self.secret_key.sign(signing_root.as_ref())
    }

    pub fn verify<T: SignedRoot>(
        &self,
        sig: &BlsSignature,
        pubkey: &BlsPublicKey,
        msg: &T,
    ) -> bool {
        let domain = self.fork_data.compute_builder_domain();
        let signing_root = msg.signing_root(domain.into());
        sig.verify(pubkey, signing_root.as_ref())
    }

    pub fn public_key(&self) -> BlsPublicKey {
        self.secret_key.public_key()
    }

    pub fn fork_data(&self) -> &ForkDatas {
        &self.fork_data
    }
}

#[cfg(test)]
mod tests {
    use tree_hash_derive::TreeHash;

    use super::*;

    #[derive(TreeHash)]
    struct TestMsg {
        value: u64,
    }

    impl SignedRoot for TestMsg {}

    #[test]
    fn test_sign_and_verify() {
        let signer = BlsSigner::default();
        let msg = TestMsg { value: 42 };
        let sig = signer.sign(&msg);
        assert!(signer.verify(&sig, &signer.public_key(), &msg));
    }

    #[test]
    fn test_verify_wrong_key() {
        let signer = BlsSigner::default();
        let msg = TestMsg { value: 42 };
        let sig = signer.sign(&msg);
        let other_signer = BlsSigner::default();
        assert!(!signer.verify(&sig, &other_signer.public_key(), &msg));
    }

    #[test]
    fn test_verify_wrong_message() {
        let signer = BlsSigner::default();
        let msg1 = TestMsg { value: 1 };
        let msg2 = TestMsg { value: 2 };
        let sig = signer.sign(&msg1);
        assert!(!signer.verify(&sig, &signer.public_key(), &msg2));
    }
}
