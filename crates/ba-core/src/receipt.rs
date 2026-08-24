use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{EntityId, Receipt, Timestamp};

/// Hashable receipt data. The resulting hash is stored in [`Receipt::hash`].
#[derive(Debug, Serialize)]
pub struct ReceiptDraft<'a> {
    pub id: &'a EntityId,
    pub project_id: &'a EntityId,
    pub task_id: &'a Option<EntityId>,
    pub run_id: &'a Option<EntityId>,
    pub kind: &'a str,
    pub payload: &'a Value,
    pub previous_hash: &'a Option<String>,
    pub created_at: &'a Timestamp,
}

impl<'a> From<&'a Receipt> for ReceiptDraft<'a> {
    fn from(receipt: &'a Receipt) -> Self {
        Self {
            id: &receipt.id,
            project_id: &receipt.project_id,
            task_id: &receipt.task_id,
            run_id: &receipt.run_id,
            kind: &receipt.kind,
            payload: &receipt.payload,
            previous_hash: &receipt.previous_hash,
            created_at: &receipt.created_at,
        }
    }
}

/// Serialize JSON with recursively sorted object keys and no insignificant whitespace.
///
/// # Errors
///
/// Returns [`ReceiptChainError::Serialization`] when the value cannot be represented as JSON.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, ReceiptChainError> {
    let value = serde_json::to_value(value)?;
    serde_json::to_string(&sort_json(value)).map_err(Into::into)
}

/// Compute the lowercase SHA-256 hash of canonical receipt data.
///
/// # Errors
///
/// Returns [`ReceiptChainError::Serialization`] when canonical serialization fails.
pub fn compute_receipt_hash(draft: &ReceiptDraft<'_>) -> Result<String, ReceiptChainError> {
    let canonical = canonical_json(draft)?;
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            let sorted = entries
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<Map<_, _>>();
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json).collect()),
        scalar => scalar,
    }
}

#[derive(Debug, Error)]
pub enum ReceiptChainError {
    #[error("receipt serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("receipt {id} expected previous hash {expected:?}, found {actual:?}")]
    PreviousHash {
        id: EntityId,
        expected: Option<String>,
        actual: Option<String>,
    },
    #[error("receipt {id} hash mismatch: expected {expected}, found {actual}")]
    HashMismatch {
        id: EntityId,
        expected: String,
        actual: String,
    },
    #[error("receipt {id} expected chain sequence {expected}, found {actual}")]
    Sequence {
        id: EntityId,
        expected: u64,
        actual: u64,
    },
    #[error("receipt {id} has inconsistent persisted field {field}")]
    StoredFieldMismatch { id: EntityId, field: &'static str },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::canonical_json;

    #[test]
    fn canonical_json_sorts_nested_objects() -> Result<(), Box<dyn std::error::Error>> {
        let value = json!({"z": 1, "a": {"d": 4, "b": 2}, "list": [{"y": 2, "x": 1}]});
        assert_eq!(
            canonical_json(&value)?,
            r#"{"a":{"b":2,"d":4},"list":[{"x":1,"y":2}],"z":1}"#
        );
        Ok(())
    }
}
