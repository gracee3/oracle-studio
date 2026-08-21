//! Stable, validated reusable chart specifications.

use astraeus_core::{
    AspectDefinitions, CalculationOptions, CalculationRequest, CelestialObject, ChartPointId,
    ChartPointSelection, GeographicLocation, UtcInstant,
};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;

/// Reusable calculation and aspect policy, independent of a subject or time.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSpecification {
    calculation: CalculationOptions,
    aspects: AspectDefinitions,
    aspect_points: ChartPointSelection,
}

#[derive(Serialize)]
struct SpecificationRef<'a> {
    schema_version: u32,
    calculation: &'a CalculationOptions,
    aspects: &'a AspectDefinitions,
    aspect_points: &'a ChartPointSelection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecificationWire {
    schema_version: u32,
    calculation: CalculationOptions,
    aspects: AspectDefinitions,
    aspect_points: ChartPointSelection,
}

#[derive(Debug, Error)]
pub enum SpecificationError {
    #[error("invalid chart specification JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported chart specification schema version {0}")]
    UnsupportedSchema(u32),
    #[error("chart point {0:?} is unavailable under the calculation object policy")]
    UnavailableChartPoint(ChartPointId),
}

impl ChartSpecification {
    pub fn new(calculation: CalculationOptions, aspects: AspectDefinitions) -> Self {
        let aspect_points = ChartPointSelection::standard(calculation.objects());
        Self {
            calculation,
            aspects,
            aspect_points,
        }
    }

    pub fn with_aspect_points(
        calculation: CalculationOptions,
        aspects: AspectDefinitions,
        aspect_points: ChartPointSelection,
    ) -> Result<Self, SpecificationError> {
        for point in aspect_points.as_slice() {
            if !point_is_available(*point, calculation.objects()) {
                return Err(SpecificationError::UnavailableChartPoint(*point));
            }
        }
        Ok(Self {
            calculation,
            aspects,
            aspect_points,
        })
    }

    pub fn from_json(input: &str) -> Result<Self, SpecificationError> {
        let wire: SpecificationWire = serde_json::from_str(input)?;
        Self::from_wire(wire)
    }

    fn from_wire(wire: SpecificationWire) -> Result<Self, SpecificationError> {
        if wire.schema_version != SCHEMA_VERSION {
            return Err(SpecificationError::UnsupportedSchema(wire.schema_version));
        }
        Self::with_aspect_points(wire.calculation, wire.aspects, wire.aspect_points)
    }

    pub fn to_json(&self) -> Result<String, SpecificationError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String, SpecificationError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn calculation(&self) -> &CalculationOptions {
        &self.calculation
    }

    pub fn aspects(&self) -> &AspectDefinitions {
        &self.aspects
    }

    pub fn aspect_points(&self) -> &ChartPointSelection {
        &self.aspect_points
    }

    pub fn request(&self, instant: UtcInstant, location: GeographicLocation) -> CalculationRequest {
        CalculationRequest::from_options(instant, location, self.calculation.clone())
    }
}

impl<'de> Deserialize<'de> for ChartSpecification {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SpecificationWire::deserialize(deserializer)?;
        Self::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

impl Serialize for ChartSpecification {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SpecificationRef {
            schema_version: SCHEMA_VERSION,
            calculation: &self.calculation,
            aspects: &self.aspects,
            aspect_points: &self.aspect_points,
        }
        .serialize(serializer)
    }
}

fn point_is_available(point: ChartPointId, objects: &[CelestialObject]) -> bool {
    if let Some(object) = point.celestial_object() {
        return objects.contains(&object);
    }
    match point {
        ChartPointId::MeanSouthNode => objects.contains(&CelestialObject::MeanNode),
        ChartPointId::TrueSouthNode => objects.contains(&CelestialObject::TrueNode),
        ChartPointId::Ascendant
        | ChartPointId::Midheaven
        | ChartPointId::Descendant
        | ChartPointId::ImumCoeli
        | ChartPointId::Vertex => true,
        _ => false,
    }
}
