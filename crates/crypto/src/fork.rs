use alloy_primitives::hex;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use std::str::FromStr;
use tree_hash::{Hash256, TreeHash};
use tree_hash_derive::TreeHash;

pub type Domain = [u8; 32];

/// The signing domain of the beacon proposer
#[derive(Debug, Clone, Default)]
pub struct ProposerDomain([u8; 4]);

/// The domain of the builder.
#[derive(Debug, Clone)]
pub struct BuilderDomain([u8; 4]);

impl AsRef<[u8]> for BuilderDomain {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Default for BuilderDomain {
    fn default() -> Self {
        BuilderDomain([0, 0, 0, 1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Chain {
    Mainnet,
    Goerli,
    Holesky,
    Sepolia,
    Dev,
}

impl Chain {
    pub fn fork_version(&self) -> [u8; 4] {
        match self {
            Chain::Mainnet => [0x00, 0x00, 0x00, 0x00],
            Chain::Goerli => [0x00, 0x00, 0x10, 0x20],
            Chain::Holesky => [0x01, 0x01, 0x70, 0x00],
            Chain::Sepolia => [0x90, 0x00, 0x00, 0x69],
            Chain::Dev => [0x20, 0x00, 0x00, 0x89],
        }
    }

    pub fn from_fork_version(bytes: [u8; 4]) -> Option<Self> {
        match bytes {
            [0x00, 0x00, 0x00, 0x00] => Some(Chain::Mainnet),
            [0x00, 0x00, 0x10, 0x20] => Some(Chain::Goerli),
            [0x01, 0x01, 0x70, 0x00] => Some(Chain::Holesky),
            [0x90, 0x00, 0x00, 0x69] => Some(Chain::Sepolia),
            [0x20, 0x00, 0x00, 0x89] => Some(Chain::Dev),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ForkVersion([u8; 4]);

impl AsRef<[u8]> for ForkVersion {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ForkVersion {
    pub fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub fn try_from_chain(chain: Chain) -> Result<Self, String> {
        Ok(Self(chain.fork_version()))
    }
}

impl FromStr for ForkVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim_start_matches("0x");
        let bytes = hex::decode(s).map_err(|e| format!("Invalid fork version: {}", e))?;
        if bytes.len() != 4 {
            return Err("Invalid fork version length".to_string());
        }
        let arr: [u8; 4] = bytes.try_into().expect("slice with incorrect length");
        Ok(ForkVersion::new(arr))
    }
}

impl From<ForkVersion> for [u8; 4] {
    fn from(val: ForkVersion) -> Self {
        val.0
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ForkData {
    #[serde(with = "serde_utils::bytes_4_hex")]
    pub current_version: [u8; 4],
    pub genesis_validators_root: Hash256,
}

impl ForkData {
    pub fn compute_builder_domain(&self) -> Domain {
        let fork_data_root = self.tree_hash_root();
        let mut domain = Domain::default();
        domain[..4].copy_from_slice(BuilderDomain::default().as_ref());
        domain[4..].copy_from_slice(&fork_data_root.as_bytes()[..28]);
        domain
    }

    pub fn compute_proposer_domain(&self) -> Domain {
        let fork_data_root = self.tree_hash_root();
        let mut domain = Domain::default();
        domain[..4].copy_from_slice(&ProposerDomain::default().0);
        domain[4..].copy_from_slice(&fork_data_root.as_bytes()[..28]);
        domain
    }
}

#[derive(Debug, Clone)]
pub struct ForkDatas {
    builder: ForkData,
    proposer: ForkData,
}

impl ForkDatas {
    pub fn from_genesis_and_current_version(
        genesis_fork_version: [u8; 4],
        current_version: [u8; 4],
        genesis_validators_root: Hash256,
    ) -> Self {
        let builder = ForkData {
            current_version: genesis_fork_version,
            genesis_validators_root: Hash256::default(),
        };
        let proposer = ForkData {
            current_version,
            genesis_validators_root,
        };
        Self { builder, proposer }
    }

    pub fn new(builder: ForkData, proposer: ForkData) -> Self {
        Self { builder, proposer }
    }

    pub fn compute_builder_domain(&self) -> Domain {
        self.builder.compute_builder_domain()
    }

    pub fn compute_proposer_domain(&self) -> Domain {
        self.proposer.compute_proposer_domain()
    }
}

impl Default for ForkDatas {
    fn default() -> Self {
        let genesis_validators_root =
            Hash256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let current_version = ForkVersion::from_str("0x20000093").unwrap().into();
        Self {
            builder: ForkData::default(),
            proposer: ForkData {
                current_version,
                genesis_validators_root,
            },
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SigningData {
    pub object_root: Hash256,
    pub domain: Hash256,
}

pub trait SignedRoot: TreeHash {
    fn signing_root(&self, domain: Hash256) -> Hash256 {
        SigningData {
            object_root: self.tree_hash_root(),
            domain,
        }
        .tree_hash_root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(TreeHash)]
    struct TestMsg {
        value: u64,
    }

    impl SignedRoot for TestMsg {}

    #[test]
    fn test_builder_domain_starts_with_0001() {
        let fd = ForkData::default();
        let domain = fd.compute_builder_domain();
        assert_eq!(domain[..4], [0, 0, 0, 1]);
    }

    #[test]
    fn test_proposer_domain_starts_with_0000() {
        let fd = ForkData::default();
        let domain = fd.compute_proposer_domain();
        assert_eq!(domain[..4], [0, 0, 0, 0]);
    }

    #[test]
    fn test_builder_and_proposer_domains_differ() {
        let fd = ForkData::default();
        let builder = fd.compute_builder_domain();
        let proposer = fd.compute_proposer_domain();
        assert_ne!(builder, proposer);
    }

    #[test]
    fn test_fork_datas_default_gives_different_domains() {
        let fds = ForkDatas::default();
        assert_ne!(fds.compute_builder_domain(), fds.compute_proposer_domain());
    }

    #[test]
    fn test_fork_version_from_str_ok() {
        let fv = ForkVersion::from_str("0x20000093").unwrap();
        assert_eq!(fv.0, [0x20, 0x00, 0x00, 0x93]);
    }

    #[test]
    fn test_fork_version_from_str_no_prefix() {
        let fv = ForkVersion::from_str("20000093").unwrap();
        assert_eq!(fv.0, [0x20, 0x00, 0x00, 0x93]);
    }

    #[test]
    fn test_fork_version_from_str_invalid() {
        let result = ForkVersion::from_str("xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_fork_version_from_str_invalid_length() {
        let result = ForkVersion::from_str("0x123456789");
        assert!(result.is_err());
    }

    #[test]
    fn test_try_from_chain_mainnet() {
        let fv = ForkVersion::try_from_chain(Chain::Mainnet).unwrap();
        let bytes: [u8; 4] = fv.into();
        assert_eq!(bytes, [0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_try_from_chain_holesky() {
        let fv = ForkVersion::try_from_chain(Chain::Holesky).unwrap();
        let bytes: [u8; 4] = fv.into();
        assert_eq!(bytes, [0x01, 0x01, 0x70, 0x00]);
    }

    #[test]
    fn test_signing_root_differs_by_domain() {
        let msg = TestMsg { value: 42 };
        let fd = ForkData::default();
        let builder_domain = fd.compute_builder_domain();
        let proposer_domain = fd.compute_proposer_domain();
        assert_ne!(
            msg.signing_root(builder_domain.into()),
            msg.signing_root(proposer_domain.into())
        );
    }

    #[test]
    fn test_from_fork_version_roundtrip() {
        let chain = Chain::Mainnet;
        let fv = ForkVersion::try_from_chain(chain).unwrap();
        let bytes: [u8; 4] = fv.into();
        let chain_back = Chain::from_fork_version(bytes);
        assert_eq!(chain_back, Some(Chain::Mainnet));
    }
}
