//! Stable, validated serialized envelopes for Astraeus calculations.

use std::collections::BTreeMap;

use astraeus_core::{
    CalculationError, CalculationProvenance, CalculationRequest, CalculationResult,
    CelestialObject, HouseCusps, Position,
};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct CalculationArtifact {
    request: CalculationRequest,
    result: CalculationResult,
}

#[derive(Serialize)]
struct ArtifactRef<'a> {
    schema_version: u32,
    request: &'a CalculationRequest,
    result: &'a CalculationResult,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    schema_version: u32,
    request: CalculationRequest,
    result: ResultWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultWire {
    positions: BTreeMap<CelestialObject, Position>,
    houses: HouseCusps,
    provenance: CalculationProvenance,
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("invalid artifact JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported calculation artifact schema version {0}")]
    UnsupportedSchema(u32),
    #[error(transparent)]
    InvalidResult(#[from] CalculationError),
}

impl CalculationArtifact {
    pub fn new(
        request: CalculationRequest,
        result: CalculationResult,
    ) -> Result<Self, ArtifactError> {
        CalculationResult::new(
            &request,
            result.positions().clone(),
            result.houses().clone(),
            result.provenance().clone(),
        )?;
        Ok(Self { request, result })
    }

    pub fn from_json(input: &str) -> Result<Self, ArtifactError> {
        let wire: ArtifactWire = serde_json::from_str(input)?;
        Self::from_wire(wire)
    }

    fn from_wire(wire: ArtifactWire) -> Result<Self, ArtifactError> {
        if wire.schema_version != SCHEMA_VERSION {
            return Err(ArtifactError::UnsupportedSchema(wire.schema_version));
        }
        let result = CalculationResult::new(
            &wire.request,
            wire.result.positions,
            wire.result.houses,
            wire.result.provenance,
        )?;
        Ok(Self {
            request: wire.request,
            result,
        })
    }

    pub fn request(&self) -> &CalculationRequest {
        &self.request
    }
    pub fn result(&self) -> &CalculationResult {
        &self.result
    }

    /// Canonical compact JSON used for content addressing.
    pub fn to_json(&self) -> Result<String, ArtifactError> {
        Ok(serde_json::to_string(self)?)
    }

    /// Human-readable JSON; never use this representation as digest input.
    pub fn to_pretty_json(&self) -> Result<String, ArtifactError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn content_sha256(&self) -> Result<String, ArtifactError> {
        let bytes = serde_json::to_vec(self)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn content_id(&self) -> Result<String, ArtifactError> {
        Ok(format!("sha256:{}", self.content_sha256()?))
    }
}

impl<'de> Deserialize<'de> for CalculationArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ArtifactWire::deserialize(deserializer)?;
        Self::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

impl Serialize for CalculationArtifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ArtifactRef {
            schema_version: SCHEMA_VERSION,
            request: &self.request,
            result: &self.result,
        }
        .serialize(serializer)
    }
}
