use std::collections::BTreeMap;

use astraeus_core::{
    CalculationProvenance, CalculationRequest, CalculationResult, CelestialObject, EphemerisSource,
    HouseCusps, Position,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{FixtureComparisonError, compare::compare_results};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSource {
    pub tool: String,
    pub version: String,
    pub repository: String,
    pub revision: String,
    pub archive_sha256: String,
    pub engine: String,
    pub command: Vec<String>,
    pub raw_output_file: String,
    pub raw_output_sha256: String,
    pub license: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureTolerances {
    pub angle_degrees: f64,
    pub distance_au: f64,
    pub speed_degrees_per_day: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoldenFixture {
    id: String,
    source: FixtureSource,
    request: CalculationRequest,
    expected: CalculationResult,
    tolerances: FixtureTolerances,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenFixtureWire {
    schema_version: u32,
    id: String,
    source: FixtureSource,
    request: CalculationRequest,
    expected: ExpectedWire,
    tolerances: FixtureTolerances,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedWire {
    positions: BTreeMap<CelestialObject, Position>,
    houses: HouseCusps,
}

#[derive(Debug, Error)]
pub enum FixtureError {
    #[error("invalid fixture JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported fixture schema version {0}")]
    UnsupportedSchema(u32),
    #[error("fixture field {0} must not be empty")]
    EmptyField(&'static str),
    #[error("fixture field {field} must be a lowercase 64-character SHA-256 digest")]
    InvalidSha256 { field: &'static str },
    #[error("fixture tolerance {0} must be finite and non-negative")]
    InvalidTolerance(&'static str),
    #[error("unsupported fixture engine {0}")]
    UnsupportedEngine(String),
    #[error(transparent)]
    InvalidExpected(#[from] astraeus_core::CalculationError),
    #[error("raw output SHA-256 mismatch: expected {expected}, got {actual}")]
    RawOutputHash { expected: String, actual: String },
}

impl GoldenFixture {
    pub fn from_json(input: &str) -> Result<Self, FixtureError> {
        let wire: GoldenFixtureWire = serde_json::from_str(input)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(FixtureError::UnsupportedSchema(wire.schema_version));
        }
        validate_nonempty("id", &wire.id)?;
        wire.source.validate()?;
        wire.tolerances.validate()?;
        let ephemeris_source = match wire.source.engine.as_str() {
            "moshier" => EphemerisSource::Moshier,
            "swiss_files" => EphemerisSource::SwissFiles,
            engine => return Err(FixtureError::UnsupportedEngine(engine.to_owned())),
        };
        let provenance = CalculationProvenance::new(
            wire.source.tool.clone(),
            wire.source.version.clone(),
            ephemeris_source,
            Some(wire.source.revision.clone()),
        )
        .map_err(astraeus_core::CalculationError::from)?;
        let expected = CalculationResult::new(
            &wire.request,
            wire.expected.positions,
            wire.expected.houses,
            provenance,
        )?;
        Ok(Self {
            id: wire.id,
            source: wire.source,
            request: wire.request,
            expected,
            tolerances: wire.tolerances,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn source(&self) -> &FixtureSource {
        &self.source
    }
    pub fn request(&self) -> &CalculationRequest {
        &self.request
    }
    pub fn expected(&self) -> &CalculationResult {
        &self.expected
    }
    pub fn tolerances(&self) -> FixtureTolerances {
        self.tolerances
    }

    pub fn compare(&self, actual: &CalculationResult) -> Result<(), FixtureComparisonError> {
        compare_results(&self.expected, actual, self.tolerances)
    }

    pub fn verify_raw_output(&self, raw_output: &str) -> Result<(), FixtureError> {
        let actual = format!("{:x}", Sha256::digest(raw_output.as_bytes()));
        if actual == self.source.raw_output_sha256 {
            Ok(())
        } else {
            Err(FixtureError::RawOutputHash {
                expected: self.source.raw_output_sha256.clone(),
                actual,
            })
        }
    }
}

impl FixtureSource {
    fn validate(&self) -> Result<(), FixtureError> {
        validate_nonempty("source.tool", &self.tool)?;
        validate_nonempty("source.version", &self.version)?;
        validate_nonempty("source.repository", &self.repository)?;
        validate_nonempty("source.revision", &self.revision)?;
        validate_sha("source.archive_sha256", &self.archive_sha256)?;
        validate_nonempty("source.engine", &self.engine)?;
        if self.command.is_empty() || self.command.iter().any(|part| part.is_empty()) {
            return Err(FixtureError::EmptyField("source.command"));
        }
        validate_nonempty("source.raw_output_file", &self.raw_output_file)?;
        validate_sha("source.raw_output_sha256", &self.raw_output_sha256)?;
        validate_nonempty("source.license", &self.license)
    }
}

impl FixtureTolerances {
    fn validate(self) -> Result<(), FixtureError> {
        for (name, value) in [
            ("angle_degrees", self.angle_degrees),
            ("distance_au", self.distance_au),
            ("speed_degrees_per_day", self.speed_degrees_per_day),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(FixtureError::InvalidTolerance(name));
            }
        }
        Ok(())
    }
}

fn validate_nonempty(field: &'static str, value: &str) -> Result<(), FixtureError> {
    if value.trim().is_empty() {
        Err(FixtureError::EmptyField(field))
    } else {
        Ok(())
    }
}

fn validate_sha(field: &'static str, value: &str) -> Result<(), FixtureError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(FixtureError::InvalidSha256 { field })
    }
}
