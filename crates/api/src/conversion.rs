use relay_crypto::{BlsPublicKey, BlsSignature};
use relay_entity::{
    bid_submission::BidSubmission,
    bid_trace::BidTrace,
    execution_payload::{BlobsBundle, ExecutionPayload, Withdrawal},
    types::{Address, B256, U256},
};
use ssz_types::{FixedVector, VariableList};
use std::str::FromStr;
use std::sync::Arc;
use typenum::U1073741824;

use crate::proto;

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("invalid hash: {0}")]
    Hash(String),
    #[error("invalid address: {0}")]
    Address(String),
    #[error("invalid BLS public key: {0}")]
    BlsPublicKey(String),
    #[error("invalid BLS signature: {0}")]
    BlsSignature(String),
    #[error("invalid U256: {0}")]
    U256(String),
    #[error("invalid fixed vector: {0}")]
    FixedVector(String),
    #[error("invalid variable list: {0}")]
    VariableList(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid bytes length for Bytes48: {0}")]
    Bytes48(String),
    #[error("invalid bytes length for Blob: {0}")]
    Blob(String),
}

impl TryFrom<proto::BidRequest> for BidSubmission {
    type Error = ConversionError;

    fn try_from(req: proto::BidRequest) -> Result<Self, Self::Error> {
        let trace = req
            .bid_trace
            .ok_or(ConversionError::MissingField("bid_trace"))?;
        let payload = req
            .execution_payload
            .ok_or(ConversionError::MissingField("execution_payload"))?;
        let blobs = req.blobs_bundle.unwrap_or_default();

        Ok(BidSubmission::new(
            BidTrace::try_from(trace)?,
            Arc::new(ExecutionPayload::try_from(payload)?),
            Arc::new(BlobsBundle::try_from(blobs)?),
            BlsSignature::deserialize(&req.signature).map_err(ConversionError::BlsSignature)?,
        ))
    }
}

impl TryFrom<proto::BidTrace> for BidTrace {
    type Error = ConversionError;

    fn try_from(t: proto::BidTrace) -> Result<Self, Self::Error> {
        Ok(Self {
            slot: t.slot,
            parent_hash: bytes_to_b256(&t.parent_hash)?,
            block_hash: bytes_to_b256(&t.block_hash)?,
            builder_pubkey: BlsPublicKey::deserialize(&t.builder_pubkey)
                .map_err(ConversionError::BlsPublicKey)?,
            proposer_pubkey: BlsPublicKey::deserialize(&t.proposer_pubkey)
                .map_err(ConversionError::BlsPublicKey)?,
            proposer_fee_recipient: bytes_to_address(&t.proposer_fee_recipient)?,
            gas_limit: t.gas_limit,
            gas_used: t.gas_used,
            value: U256::from_str(&t.value).map_err(|e| ConversionError::U256(e.to_string()))?,
        })
    }
}

impl TryFrom<proto::ExecutionPayload> for ExecutionPayload {
    type Error = ConversionError;

    fn try_from(p: proto::ExecutionPayload) -> Result<Self, Self::Error> {
        Ok(Self {
            parent_hash: bytes_to_b256(&p.parent_hash)?,
            fee_recipient: bytes_to_address(&p.fee_recipient)?,
            state_root: bytes_to_b256(&p.state_root)?,
            receipts_root: bytes_to_b256(&p.receipts_root)?,
            logs_bloom: FixedVector::new(p.logs_bloom)
                .map_err(|e| ConversionError::FixedVector(format!("{e:?}")))?,
            prev_randao: bytes_to_b256(&p.prev_randao)?,
            block_number: p.block_number,
            gas_limit: p.gas_limit,
            gas_used: p.gas_used,
            timestamp: p.timestamp,
            extra_data: VariableList::new(p.extra_data)
                .map_err(|e| ConversionError::VariableList(format!("{e:?}")))?,
            base_fee_per_gas: U256(alloy_primitives::U256::from_be_slice(&p.base_fee_per_gas)),
            block_hash: bytes_to_b256(&p.block_hash)?,
            transactions: {
                let inner: Vec<VariableList<u8, U1073741824>> = p
                    .transactions
                    .into_iter()
                    .map(|tx| {
                        VariableList::new(tx)
                            .map_err(|e| ConversionError::VariableList(format!("{e:?}")))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                VariableList::new(inner)
                    .map_err(|e| ConversionError::VariableList(format!("{e:?}")))?
            },
            withdrawals: {
                let inner: Vec<Withdrawal> = p
                    .withdrawals
                    .into_iter()
                    .map(Withdrawal::try_from)
                    .collect::<Result<Vec<_>, _>>()?;
                VariableList::new(inner)
                    .map_err(|e| ConversionError::VariableList(format!("{e:?}")))?
            },
            blob_gas_used: p.blob_gas_used,
            excess_blob_gas: p.excess_blob_gas,
        })
    }
}

impl TryFrom<proto::Withdrawal> for Withdrawal {
    type Error = ConversionError;

    fn try_from(w: proto::Withdrawal) -> Result<Self, Self::Error> {
        Ok(Self {
            index: w.index,
            validator_index: w.validator_index,
            address: bytes_to_address(&w.address)?,
            amount: w.amount,
        })
    }
}

impl TryFrom<proto::BlobsBundle> for BlobsBundle {
    type Error = ConversionError;

    fn try_from(b: proto::BlobsBundle) -> Result<Self, Self::Error> {
        Ok(Self {
            commitments: b
                .commitments
                .into_iter()
                .map(bytes_to_bytes48)
                .collect::<Result<Vec<_>, _>>()?,
            proofs: b
                .proofs
                .into_iter()
                .map(bytes_to_bytes48)
                .collect::<Result<Vec<_>, _>>()?,
            blobs: b
                .blobs
                .into_iter()
                .map(bytes_to_blob)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<proto::SignedValidatorRegistration> for relay_entity::SignedValidatorRegistration {
    type Error = ConversionError;

    fn try_from(s: proto::SignedValidatorRegistration) -> Result<Self, Self::Error> {
        let msg = s.message.ok_or(ConversionError::MissingField("message"))?;
        Ok(Self {
            message: relay_entity::ValidatorRegistration {
                fee_recipient: bytes_to_address(&msg.fee_recipient)?,
                gas_limit: msg.gas_limit,
                timestamp: msg.timestamp,
                pubkey: BlsPublicKey::deserialize(&msg.pubkey)
                    .map_err(ConversionError::BlsPublicKey)?,
            },
            signature: BlsSignature::deserialize(&s.signature)
                .map_err(ConversionError::BlsSignature)?,
        })
    }
}

fn bytes_to_b256(bytes: &[u8]) -> Result<B256, ConversionError> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ConversionError::Hash(format!("expected 32 bytes, got {}", bytes.len())))?;
    Ok(B256(alloy_primitives::B256::from(arr)))
}

fn bytes_to_address(bytes: &[u8]) -> Result<Address, ConversionError> {
    let arr: [u8; 20] = bytes
        .try_into()
        .map_err(|_| ConversionError::Address(format!("expected 20 bytes, got {}", bytes.len())))?;
    Ok(Address(alloy_primitives::Address::from(arr)))
}

fn bytes_to_bytes48(bytes: Vec<u8>) -> Result<alloy_primitives::FixedBytes<48>, ConversionError> {
    if bytes.len() != 48 {
        return Err(ConversionError::Bytes48(format!(
            "expected 48 bytes, got {}",
            bytes.len()
        )));
    }
    let arr: [u8; 48] = bytes.try_into().unwrap();
    Ok(alloy_primitives::FixedBytes::from(arr))
}

fn bytes_to_blob(bytes: Vec<u8>) -> Result<alloy_primitives::FixedBytes<131072>, ConversionError> {
    if bytes.len() != 131072 {
        return Err(ConversionError::Blob(format!(
            "expected 131072 bytes, got {}",
            bytes.len()
        )));
    }
    let arr: [u8; 131072] = bytes.try_into().unwrap();
    Ok(alloy_primitives::FixedBytes::from(arr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_crypto::BlsSecretKey;

    fn valid_pubkey_bytes() -> Vec<u8> {
        BlsSecretKey::random().public_key().serialize().to_vec()
    }

    fn valid_signature_bytes() -> Vec<u8> {
        let sk = BlsSecretKey::random();
        sk.sign(b"test").serialize().to_vec()
    }

    fn make_test_bid_trace() -> proto::BidTrace {
        let pk = valid_pubkey_bytes();
        proto::BidTrace {
            slot: 1,
            parent_hash: vec![0u8; 32],
            block_hash: vec![1u8; 32],
            builder_pubkey: pk.clone(),
            proposer_pubkey: pk,
            proposer_fee_recipient: vec![4u8; 20],
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            value: "1000000000000000000".to_string(),
        }
    }

    fn make_test_execution_payload() -> proto::ExecutionPayload {
        proto::ExecutionPayload {
            parent_hash: vec![0u8; 32],
            state_root: vec![1u8; 32],
            receipts_root: vec![2u8; 32],
            logs_bloom: vec![0u8; 256],
            prev_randao: vec![3u8; 32],
            extra_data: vec![],
            base_fee_per_gas: vec![0u8; 32],
            fee_recipient: vec![5u8; 20],
            block_hash: vec![6u8; 32],
            transactions: vec![],
            withdrawals: vec![],
            block_number: 42,
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            timestamp: 1234567890,
            blob_gas_used: 0,
            excess_blob_gas: 0,
        }
    }

    fn make_test_blobs_bundle() -> proto::BlobsBundle {
        proto::BlobsBundle {
            commitments: vec![],
            proofs: vec![],
            blobs: vec![],
        }
    }

    #[test]
    fn test_bid_trace_conversion() {
        let proto = make_test_bid_trace();
        let entity = BidTrace::try_from(proto).unwrap();
        assert_eq!(entity.slot, 1);
        assert_eq!(
            entity.parent_hash,
            B256(alloy_primitives::B256::from([0u8; 32]))
        );
        assert_eq!(entity.value.to_string(), "1000000000000000000");
    }

    #[test]
    fn test_bid_trace_invalid_hash_length() {
        let mut proto = make_test_bid_trace();
        proto.parent_hash = vec![0u8; 16];
        let result = BidTrace::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_bid_trace_invalid_pubkey_length() {
        let mut proto = make_test_bid_trace();
        proto.builder_pubkey = vec![0u8; 16];
        let result = BidTrace::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_bid_trace_invalid_address_length() {
        let mut proto = make_test_bid_trace();
        proto.proposer_fee_recipient = vec![0u8; 16];
        let result = BidTrace::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_bid_trace_invalid_value() {
        let mut proto = make_test_bid_trace();
        proto.value = "not-a-number".to_string();
        let result = BidTrace::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_payload_conversion() {
        let proto = make_test_execution_payload();
        let entity = ExecutionPayload::try_from(proto).unwrap();
        assert_eq!(entity.block_number, 42);
        assert_eq!(
            entity.parent_hash,
            B256(alloy_primitives::B256::from([0u8; 32]))
        );
    }

    #[test]
    fn test_execution_payload_invalid_logs_bloom() {
        let mut proto = make_test_execution_payload();
        proto.logs_bloom = vec![0u8; 128];
        let result = ExecutionPayload::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdrawal_conversion() {
        let proto = proto::Withdrawal {
            index: 1,
            validator_index: 2,
            address: vec![3u8; 20],
            amount: 4,
        };
        let entity = Withdrawal::try_from(proto).unwrap();
        assert_eq!(entity.index, 1);
        assert_eq!(entity.validator_index, 2);
        assert_eq!(entity.amount, 4);
    }

    #[test]
    fn test_withdrawal_invalid_address() {
        let proto = proto::Withdrawal {
            index: 1,
            validator_index: 2,
            address: vec![3u8; 19],
            amount: 4,
        };
        let result = Withdrawal::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_blobs_bundle_conversion() {
        let proto = make_test_blobs_bundle();
        let entity = BlobsBundle::try_from(proto).unwrap();
        assert!(entity.commitments.is_empty());
        assert!(entity.proofs.is_empty());
        assert!(entity.blobs.is_empty());
    }

    #[test]
    fn test_bid_request_conversion() {
        let proto = proto::BidRequest {
            bid_trace: Some(make_test_bid_trace()),
            execution_payload: Some(make_test_execution_payload()),
            signature: valid_signature_bytes(),
            blobs_bundle: Some(make_test_blobs_bundle()),
        };
        let entity = BidSubmission::try_from(proto).unwrap();
        assert_eq!(entity.message.slot, 1);
    }

    #[test]
    fn test_bid_request_missing_fields() {
        let proto = proto::BidRequest {
            bid_trace: None,
            execution_payload: None,
            signature: vec![],
            blobs_bundle: None,
        };
        let result = BidSubmission::try_from(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_bid_request_invalid_signature() {
        let proto = proto::BidRequest {
            bid_trace: Some(make_test_bid_trace()),
            execution_payload: Some(make_test_execution_payload()),
            signature: vec![0u8; 10],
            blobs_bundle: Some(make_test_blobs_bundle()),
        };
        let result = BidSubmission::try_from(proto);
        assert!(result.is_err());
    }
}
