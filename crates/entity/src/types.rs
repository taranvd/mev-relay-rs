use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use tree_hash::{PackedEncoding, TreeHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct B256(pub alloy_primitives::B256);

impl FromStr for B256 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        alloy_primitives::B256::from_str(s).map(Self).map_err(|e| format!("{e:?}"))
    }
}

impl TreeHash for B256 {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Vector
    }
    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        self.0.as_slice().into()
    }
    fn tree_hash_packing_factor() -> usize {
        1
    }
    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        tree_hash::Hash256::from_slice(self.0.as_slice())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(pub alloy_primitives::Address);

impl FromStr for Address {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        alloy_primitives::Address::from_str(s).map(Self).map_err(|e| format!("{e:?}"))
    }
}

impl TreeHash for Address {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Vector
    }
    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        let mut buf = [0u8; 32];
        buf[..20].copy_from_slice(self.0.as_slice());
        buf.as_slice().into()
    }
    fn tree_hash_packing_factor() -> usize {
        1
    }
    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        let mut buf = [0u8; 32];
        buf[..20].copy_from_slice(self.0.as_slice());
        tree_hash::Hash256::from_slice(&buf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct U256(pub alloy_primitives::U256);

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for U256 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        alloy_primitives::U256::from_str(s)
            .map(Self)
            .map_err(|e| format!("{e:?}"))
    }
}

impl TreeHash for U256 {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Basic
    }
    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        self.0.to_le_bytes::<32>().as_slice().into()
    }
    fn tree_hash_packing_factor() -> usize {
        1
    }
    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        tree_hash::Hash256::from_slice(&self.0.to_le_bytes::<32>())
    }
}
