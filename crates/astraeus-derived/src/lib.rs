//! Validated, content-addressed derived chart artifacts.

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    Aspect, PointPlacement, ValidationError, calculate_aspects, calculate_placements,
    chart_point_positions,
};
use astraeus_specifications::ChartSpecification;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedChartArtifact {
    calculation: CalculationArtifact,
    specification: ChartSpecification,
    placements: Vec<PointPlacement>,
    aspects: Vec<Aspect>,
}

#[derive(Serialize)]
struct DerivedRef<'a> {
    schema_version: u32,
    calculation: &'a CalculationArtifact,
    specification: &'a ChartSpecification,
    placements: &'a [PointPlacement],
    aspects: &'a [Aspect],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DerivedWire {
    schema_version: u32,
    calculation: CalculationArtifact,
    specification: ChartSpecification,
    placements: Vec<PointPlacement>,
    aspects: Vec<Aspect>,
}

#[derive(Debug, Error)]
pub enum DerivedArtifactError {
    #[error("invalid derived chart artifact JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported derived chart artifact schema version {0}")]
    UnsupportedSchema(u32),
    #[error("chart specification calculation options do not match the calculation request")]
    CalculationPolicyMismatch,
    #[error("serialized aspects do not match aspects derived from the calculation")]
    AspectMismatch,
    #[error("serialized placements do not match placements derived from the calculation")]
    PlacementMismatch,
    #[error(transparent)]
    InvalidDerivedValue(#[from] ValidationError),
}

impl DerivedChartArtifact {
    pub fn new(
        calculation: CalculationArtifact,
        specification: ChartSpecification,
    ) -> Result<Self, DerivedArtifactError> {
        if calculation.request().options() != *specification.calculation() {
            return Err(DerivedArtifactError::CalculationPolicyMismatch);
        }
        let all_points = chart_point_positions(calculation.result())?;
        let aspect_points = specification
            .aspect_points()
            .as_slice()
            .iter()
            .map(|point| (*point, all_points[point]))
            .collect();
        let placements = calculate_placements(calculation.result())?;
        let aspects = calculate_aspects(&aspect_points, specification.aspects());
        Ok(Self {
            calculation,
            specification,
            placements,
            aspects,
        })
    }

    pub fn from_json(input: &str) -> Result<Self, DerivedArtifactError> {
        let wire: DerivedWire = serde_json::from_str(input)?;
        Self::from_wire(wire)
    }

    fn from_wire(wire: DerivedWire) -> Result<Self, DerivedArtifactError> {
        if wire.schema_version != SCHEMA_VERSION {
            return Err(DerivedArtifactError::UnsupportedSchema(wire.schema_version));
        }
        let derived = Self::new(wire.calculation, wire.specification)?;
        if !placements_match(&derived.placements, &wire.placements) {
            return Err(DerivedArtifactError::PlacementMismatch);
        }
        if !aspects_match(&derived.aspects, &wire.aspects) {
            return Err(DerivedArtifactError::AspectMismatch);
        }
        Ok(derived)
    }

    pub fn calculation(&self) -> &CalculationArtifact {
        &self.calculation
    }

    pub fn specification(&self) -> &ChartSpecification {
        &self.specification
    }

    pub fn aspects(&self) -> &[Aspect] {
        &self.aspects
    }

    pub fn placements(&self) -> &[PointPlacement] {
        &self.placements
    }

    pub fn to_json(&self) -> Result<String, DerivedArtifactError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String, DerivedArtifactError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn content_sha256(&self) -> Result<String, DerivedArtifactError> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }

    pub fn content_id(&self) -> Result<String, DerivedArtifactError> {
        Ok(format!("sha256:{}", self.content_sha256()?))
    }
}

fn placements_match(first: &[PointPlacement], second: &[PointPlacement]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(first, second)| {
            first.point() == second.point()
                && first.house() == second.house()
                && first.sign().sign() == second.sign().sign()
                && (first.sign().degrees_within_sign() - second.sign().degrees_within_sign()).abs()
                    <= 1e-12
        })
}

fn aspects_match(first: &[Aspect], second: &[Aspect]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(first, second)| {
            first.first() == second.first()
                && first.second() == second.second()
                && first.kind() == second.kind()
                && first.phase() == second.phase()
                && (first.separation_degrees() - second.separation_degrees()).abs() <= 1e-12
                && (first.signed_separation_degrees() - second.signed_separation_degrees()).abs()
                    <= 1e-12
                && (first.orb_degrees() - second.orb_degrees()).abs() <= 1e-12
                && (first.relative_speed_degrees_per_day()
                    - second.relative_speed_degrees_per_day())
                .abs()
                    <= 1e-12
        })
}

impl<'de> Deserialize<'de> for DerivedChartArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = DerivedWire::deserialize(deserializer)?;
        Self::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

impl Serialize for DerivedChartArtifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        DerivedRef {
            schema_version: SCHEMA_VERSION,
            calculation: &self.calculation,
            specification: &self.specification,
            placements: &self.placements,
            aspects: &self.aspects,
        }
        .serialize(serializer)
    }
}
