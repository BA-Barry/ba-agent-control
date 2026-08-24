use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;
use ulid::Ulid;

/// Stable, time-sortable identifier shared by all domain entities.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EntityId(String);

impl EntityId {
    /// Generate a fresh ULID.
    #[must_use]
    pub fn new() -> Self {
        Self(Ulid::new().to_string())
    }

    /// Access the canonical 26-character representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for EntityId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let ulid = Ulid::from_string(value).map_err(|_| IdError::Invalid(value.to_owned()))?;
        Ok(Self(ulid.to_string()))
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(de::Error::custom)
    }
}

/// Identifier parsing failure.
#[derive(Debug, Error)]
pub enum IdError {
    #[error("invalid ULID: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::EntityId;

    #[test]
    fn generated_id_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let id = EntityId::new();
        assert_eq!(EntityId::from_str(id.as_str())?, id);
        Ok(())
    }

    #[test]
    fn rejects_non_ulid() {
        assert!(EntityId::from_str("not-an-id").is_err());
    }

    #[test]
    fn deserialization_preserves_the_ulid_invariant() {
        assert!(serde_json::from_str::<EntityId>(r#""not-an-id""#).is_err());
    }
}
