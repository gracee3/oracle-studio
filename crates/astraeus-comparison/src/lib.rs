//! Typed, versioned aspects between two independently calculated charts.

use std::collections::BTreeMap;

use astraeus_core::{
    ASPECT_EXACT_TOLERANCE_DEGREES, ASPECT_STATION_TOLERANCE_DEGREES_PER_DAY, AspectDefinitions,
    AspectKind, AspectPhase, ChartPointId, ChartPointSelection, PhaseAwareAspectDefinitions,
    chart_point_positions,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_techniques::{ProgressedChartArtifact, SyntheticChartArtifact};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "layer_kind", content = "artifact", rename_all = "snake_case")]
pub enum ChartLayerArtifact {
    Physical(DerivedChartArtifact),
    Progressed(ProgressedChartArtifact),
    Synthetic(SyntheticChartArtifact),
}

#[derive(Clone, Copy)]
struct LayerPoint {
    longitude_degrees: f64,
    motion_degrees_per_day: Option<f64>,
}

impl From<DerivedChartArtifact> for ChartLayerArtifact {
    fn from(value: DerivedChartArtifact) -> Self {
        Self::Physical(value)
    }
}
impl From<ProgressedChartArtifact> for ChartLayerArtifact {
    fn from(value: ProgressedChartArtifact) -> Self {
        Self::Progressed(value)
    }
}
impl From<SyntheticChartArtifact> for ChartLayerArtifact {
    fn from(value: SyntheticChartArtifact) -> Self {
        Self::Synthetic(value)
    }
}

impl ChartLayerArtifact {
    fn frame(&self) -> (astraeus_core::Zodiac, Option<astraeus_core::Ayanamsa>) {
        match self {
            Self::Physical(chart) => {
                let request = chart.calculation().request();
                (request.zodiac(), request.ayanamsa())
            }
            Self::Progressed(chart) => (chart.zodiac(), chart.ayanamsa()),
            Self::Synthetic(chart) => (chart.zodiac(), chart.ayanamsa()),
        }
    }
    fn points(&self) -> Result<BTreeMap<ChartPointId, LayerPoint>, ComparisonArtifactError> {
        match self {
            Self::Physical(chart) => Ok(chart_point_positions(chart.calculation().result())
                .map_err(|error| ComparisonArtifactError::InvalidPointData(error.to_string()))?
                .into_iter()
                .map(|(id, p)| {
                    (
                        id,
                        LayerPoint {
                            longitude_degrees: p.longitude_degrees(),
                            motion_degrees_per_day: Some(p.longitude_speed_degrees_per_day()),
                        },
                    )
                })
                .collect()),
            Self::Progressed(chart) => Ok(chart
                .points()
                .iter()
                .map(|(id, p)| {
                    (
                        *id,
                        LayerPoint {
                            longitude_degrees: p.longitude_degrees(),
                            motion_degrees_per_day: p.motion_degrees_per_target_day(),
                        },
                    )
                })
                .collect()),
            Self::Synthetic(chart) => Ok(chart
                .points()
                .iter()
                .map(|(id, p)| {
                    (
                        *id,
                        LayerPoint {
                            longitude_degrees: p.longitude_degrees(),
                            motion_degrees_per_day: p.motion_degrees_per_target_day(),
                        },
                    )
                })
                .collect()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonKind {
    Generic,
    Synastry,
    TransitToNatal,
    EventToNatal,
    ReturnToNatal,
    ProgressedToNatal,
    ProgressedSynastry,
    TransitToTransit,
    ProgressedToProgressed,
    HarmonicToNatal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMotionPolicy {
    None,
    SecondMovesAgainstFirstFixed,
    BothInstantaneous,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonSpecification {
    kind: ComparisonKind,
    aspects: AspectDefinitions,
    first_points: ChartPointSelection,
    second_points: ChartPointSelection,
    motion: ComparisonMotionPolicy,
}

impl ComparisonSpecification {
    pub fn new(
        kind: ComparisonKind,
        aspects: AspectDefinitions,
        first_points: ChartPointSelection,
        second_points: ChartPointSelection,
        motion: ComparisonMotionPolicy,
    ) -> Result<Self, ComparisonArtifactError> {
        if first_points.as_slice().is_empty() || second_points.as_slice().is_empty() {
            return Err(ComparisonArtifactError::EmptyPointSelection);
        }
        Ok(Self {
            kind,
            aspects,
            first_points,
            second_points,
            motion,
        })
    }

    pub fn synastry(
        aspects: AspectDefinitions,
        first_points: ChartPointSelection,
        second_points: ChartPointSelection,
    ) -> Result<Self, ComparisonArtifactError> {
        Self::new(
            ComparisonKind::Synastry,
            aspects,
            first_points,
            second_points,
            ComparisonMotionPolicy::None,
        )
    }

    pub fn moving_second(
        kind: ComparisonKind,
        aspects: AspectDefinitions,
        first_points: ChartPointSelection,
        second_points: ChartPointSelection,
    ) -> Result<Self, ComparisonArtifactError> {
        Self::new(
            kind,
            aspects,
            first_points,
            second_points,
            ComparisonMotionPolicy::SecondMovesAgainstFirstFixed,
        )
    }

    pub const fn kind(&self) -> ComparisonKind {
        self.kind
    }

    pub const fn aspects(&self) -> &AspectDefinitions {
        &self.aspects
    }

    /// Returns the first layer's points in their declared selection order.
    pub const fn first_points(&self) -> &ChartPointSelection {
        &self.first_points
    }

    /// Returns the second layer's points in their declared selection order.
    pub const fn second_points(&self) -> &ChartPointSelection {
        &self.second_points
    }

    pub const fn motion(&self) -> ComparisonMotionPolicy {
        self.motion
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterChartAspect {
    first: ChartPointId,
    second: ChartPointId,
    kind: AspectKind,
    separation_degrees: f64,
    signed_separation_degrees: f64,
    orb_degrees: f64,
    relative_speed_degrees_per_day: Option<f64>,
    phase: Option<AspectPhase>,
}

impl InterChartAspect {
    pub const fn first(self) -> ChartPointId {
        self.first
    }

    pub const fn second(self) -> ChartPointId {
        self.second
    }

    pub const fn kind(self) -> AspectKind {
        self.kind
    }

    pub fn orb_degrees(self) -> f64 {
        self.orb_degrees
    }

    pub fn relative_speed_degrees_per_day(self) -> Option<f64> {
        self.relative_speed_degrees_per_day
    }

    pub const fn phase(self) -> Option<AspectPhase> {
        self.phase
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonArtifact {
    first: ChartLayerArtifact,
    second: ChartLayerArtifact,
    specification: ComparisonSpecification,
    aspects: Vec<InterChartAspect>,
}

#[derive(Serialize)]
struct ArtifactRef<'a> {
    schema_version: u32,
    first: &'a ChartLayerArtifact,
    second: &'a ChartLayerArtifact,
    specification: &'a ComparisonSpecification,
    aspects: &'a [InterChartAspect],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    schema_version: u32,
    first: ChartLayerArtifact,
    second: ChartLayerArtifact,
    specification: ComparisonSpecification,
    aspects: Vec<InterChartAspect>,
}

#[derive(Debug, Error)]
pub enum ComparisonArtifactError {
    #[error("invalid comparison artifact JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported comparison artifact schema version {0}")]
    UnsupportedSchema(u32),
    #[error("comparison point selections must not be empty")]
    EmptyPointSelection,
    #[error("comparison charts must use the same zodiac and ayanamsa")]
    CoordinateFrameMismatch,
    #[error("chart {side} does not contain selected point {point:?}")]
    MissingPoint {
        side: &'static str,
        point: ChartPointId,
    },
    #[error("serialized inter-chart aspects do not match the charts and policy")]
    AspectMismatch,
    #[error("comparison motion policy requires motion for {side} point {point:?}")]
    MissingMotion {
        side: &'static str,
        point: ChartPointId,
    },
    #[error("invalid chart point data: {0}")]
    InvalidPointData(String),
}

impl ComparisonArtifact {
    pub fn new(
        first: impl Into<ChartLayerArtifact>,
        second: impl Into<ChartLayerArtifact>,
        specification: ComparisonSpecification,
    ) -> Result<Self, ComparisonArtifactError> {
        let first = first.into();
        let second = second.into();
        if specification.first_points.as_slice().is_empty()
            || specification.second_points.as_slice().is_empty()
        {
            return Err(ComparisonArtifactError::EmptyPointSelection);
        }
        if first.frame() != second.frame() {
            return Err(ComparisonArtifactError::CoordinateFrameMismatch);
        }
        let first_all = first.points()?;
        let second_all = second.points()?;
        let first_points = select_points(&first_all, &specification.first_points, "first")?;
        let second_points = select_points(&second_all, &specification.second_points, "second")?;
        let aspects = calculate_inter_chart_aspects(
            &first_points,
            &second_points,
            &specification.aspects,
            specification.motion,
        )?;
        Ok(Self {
            first,
            second,
            specification,
            aspects,
        })
    }

    pub fn from_json(input: &str) -> Result<Self, ComparisonArtifactError> {
        let wire: ArtifactWire = serde_json::from_str(input)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(ComparisonArtifactError::UnsupportedSchema(
                wire.schema_version,
            ));
        }
        let artifact = Self::new(wire.first, wire.second, wire.specification)?;
        if !inter_aspects_match(&artifact.aspects, &wire.aspects) {
            return Err(ComparisonArtifactError::AspectMismatch);
        }
        Ok(artifact)
    }

    pub fn first(&self) -> &ChartLayerArtifact {
        &self.first
    }

    pub fn second(&self) -> &ChartLayerArtifact {
        &self.second
    }

    pub fn specification(&self) -> &ComparisonSpecification {
        &self.specification
    }

    pub fn aspects(&self) -> &[InterChartAspect] {
        &self.aspects
    }

    pub fn to_json(&self) -> Result<String, ComparisonArtifactError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn content_sha256(&self) -> Result<String, ComparisonArtifactError> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }

    pub fn content_id(&self) -> Result<String, ComparisonArtifactError> {
        Ok(format!("sha256:{}", self.content_sha256()?))
    }
}

impl Serialize for ComparisonArtifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ArtifactRef {
            schema_version: SCHEMA_VERSION,
            first: &self.first,
            second: &self.second,
            specification: &self.specification,
            aspects: &self.aspects,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ComparisonArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ArtifactWire::deserialize(deserializer)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(serde::de::Error::custom(format!(
                "unsupported comparison artifact schema version {}",
                wire.schema_version
            )));
        }
        let serialized_aspects = wire.aspects;
        let artifact = Self::new(wire.first, wire.second, wire.specification)
            .map_err(serde::de::Error::custom)?;
        if !inter_aspects_match(&artifact.aspects, &serialized_aspects) {
            return Err(serde::de::Error::custom(
                "serialized inter-chart aspects do not match the charts and policy",
            ));
        }
        Ok(artifact)
    }
}

fn select_points(
    all: &BTreeMap<ChartPointId, LayerPoint>,
    selection: &ChartPointSelection,
    side: &'static str,
) -> Result<BTreeMap<ChartPointId, LayerPoint>, ComparisonArtifactError> {
    selection
        .as_slice()
        .iter()
        .map(|point| {
            all.get(point)
                .copied()
                .map(|position| (*point, position))
                .ok_or(ComparisonArtifactError::MissingPoint {
                    side,
                    point: *point,
                })
        })
        .collect()
}

fn calculate_inter_chart_aspects(
    first: &BTreeMap<ChartPointId, LayerPoint>,
    second: &BTreeMap<ChartPointId, LayerPoint>,
    definitions: &AspectDefinitions,
    motion: ComparisonMotionPolicy,
) -> Result<Vec<InterChartAspect>, ComparisonArtifactError> {
    let definitions = PhaseAwareAspectDefinitions::uniform(definitions)
        .map_err(|error| ComparisonArtifactError::InvalidPointData(error.to_string()))?;
    calculate_inter_chart_aspects_with_rules(first, second, &definitions, motion)
}

/// Calculate inter-chart aspects with phase/category-aware orb rules without
/// changing the schema-v1 [`ComparisonArtifact`] contract.
pub fn calculate_phase_aware_inter_chart_aspects(
    first: &ChartLayerArtifact,
    second: &ChartLayerArtifact,
    definitions: &PhaseAwareAspectDefinitions,
    first_points: &ChartPointSelection,
    second_points: &ChartPointSelection,
    motion: ComparisonMotionPolicy,
) -> Result<Vec<InterChartAspect>, ComparisonArtifactError> {
    if first.frame() != second.frame() {
        return Err(ComparisonArtifactError::CoordinateFrameMismatch);
    }
    let first_all = first.points()?;
    let second_all = second.points()?;
    let first = select_points(&first_all, first_points, "first")?;
    let second = select_points(&second_all, second_points, "second")?;
    calculate_inter_chart_aspects_with_rules(&first, &second, definitions, motion)
}

fn calculate_inter_chart_aspects_with_rules(
    first: &BTreeMap<ChartPointId, LayerPoint>,
    second: &BTreeMap<ChartPointId, LayerPoint>,
    definitions: &PhaseAwareAspectDefinitions,
    motion: ComparisonMotionPolicy,
) -> Result<Vec<InterChartAspect>, ComparisonArtifactError> {
    let mut aspects = Vec::new();
    for (first_id, first_position) in first {
        for (second_id, second_position) in second {
            let signed_separation = signed_separation(
                first_position.longitude_degrees,
                second_position.longitude_degrees,
            );
            let separation = signed_separation.abs();
            let potential = definitions
                .as_slice()
                .iter()
                .map(|definition| {
                    let orb = (separation - definition.kind().angle_degrees()).abs();
                    (definition, orb)
                })
                .filter(|(definition, orb)| {
                    *orb <= definition
                        .orbs()
                        .for_pair_phase(*first_id, *second_id, None)
                })
                .collect::<Vec<_>>();
            if potential.is_empty() {
                continue;
            }
            let relative_speed = comparison_relative_speed(
                first_position,
                second_position,
                *first_id,
                *second_id,
                motion,
            )?;
            let best = potential
                .into_iter()
                .filter(|(definition, orb)| {
                    let definition_phase = relative_speed
                        .map(|speed| classify_phase(signed_separation, definition.kind(), speed));
                    *orb <= definition.orbs().for_pair_phase(
                        *first_id,
                        *second_id,
                        definition_phase,
                    )
                })
                .min_by(|(left, left_orb), (right, right_orb)| {
                    left_orb
                        .total_cmp(right_orb)
                        .then_with(|| left.kind().cmp(&right.kind()))
                });
            if let Some((definition, orb)) = best {
                let phase = relative_speed
                    .map(|speed| classify_phase(signed_separation, definition.kind(), speed));
                aspects.push(InterChartAspect {
                    first: *first_id,
                    second: *second_id,
                    kind: definition.kind(),
                    separation_degrees: separation,
                    signed_separation_degrees: signed_separation,
                    orb_degrees: orb,
                    relative_speed_degrees_per_day: relative_speed,
                    phase,
                });
            }
        }
    }
    Ok(aspects)
}

fn comparison_relative_speed(
    first: &LayerPoint,
    second: &LayerPoint,
    first_id: ChartPointId,
    second_id: ChartPointId,
    motion: ComparisonMotionPolicy,
) -> Result<Option<f64>, ComparisonArtifactError> {
    match motion {
        ComparisonMotionPolicy::None => Ok(None),
        ComparisonMotionPolicy::SecondMovesAgainstFirstFixed => {
            Ok(Some(second.motion_degrees_per_day.ok_or(
                ComparisonArtifactError::MissingMotion {
                    side: "second",
                    point: second_id,
                },
            )?))
        }
        ComparisonMotionPolicy::BothInstantaneous => Ok(Some(
            second
                .motion_degrees_per_day
                .ok_or(ComparisonArtifactError::MissingMotion {
                    side: "second",
                    point: second_id,
                })?
                - first
                    .motion_degrees_per_day
                    .ok_or(ComparisonArtifactError::MissingMotion {
                        side: "first",
                        point: first_id,
                    })?,
        )),
    }
}

fn signed_separation(first_longitude: f64, second_longitude: f64) -> f64 {
    let separation = (second_longitude - first_longitude).rem_euclid(360.0);
    if separation > 180.0 {
        separation - 360.0
    } else {
        separation
    }
}

fn classify_phase(signed_separation: f64, kind: AspectKind, relative_speed: f64) -> AspectPhase {
    let angle = kind.angle_degrees();
    let signed_target = if signed_separation < 0.0 {
        -angle
    } else {
        angle
    };
    let deviation = signed_separation - signed_target;
    if deviation.abs() <= ASPECT_EXACT_TOLERANCE_DEGREES {
        AspectPhase::Exact
    } else if relative_speed.abs() <= ASPECT_STATION_TOLERANCE_DEGREES_PER_DAY {
        AspectPhase::Stationary
    } else if deviation * relative_speed < 0.0 {
        AspectPhase::Applying
    } else {
        AspectPhase::Separating
    }
}

fn inter_aspects_match(first: &[InterChartAspect], second: &[InterChartAspect]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(first, second)| {
            first.first == second.first
                && first.second == second.second
                && first.kind == second.kind
                && first.phase == second.phase
                && (first.separation_degrees - second.separation_degrees).abs() <= 1e-12
                && (first.signed_separation_degrees - second.signed_separation_degrees).abs()
                    <= 1e-12
                && (first.orb_degrees - second.orb_degrees).abs() <= 1e-12
                && match (
                    first.relative_speed_degrees_per_day,
                    second.relative_speed_degrees_per_day,
                ) {
                    (None, None) => true,
                    (Some(first), Some(second)) => (first - second).abs() <= 1e-12,
                    _ => false,
                }
        })
}
