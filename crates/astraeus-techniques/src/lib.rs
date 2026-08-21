//! Versioned Western chart techniques with explicit method and motion policies.

use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    Ayanamsa, CalculationRequest, ChartPointId, EphemerisAdapter, GeographicLocation, UtcInstant,
    ValidationError, Zodiac, chart_point_positions,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
pub const MEAN_TROPICAL_YEAR_DAYS: f64 = 365.2422;
pub const MEAN_SIDEREAL_MONTH_DAYS: f64 = 27.321_582_18;
pub const NAIBOD_DEGREES_PER_YEAR: f64 = 0.985_647_33;
const VALUE_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressionMethod {
    Secondary,
    TertiaryI,
    TertiaryII,
    Minor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnglePolicy {
    NatalFixed,
    RecastAtSymbolicInstant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolarArcMethod {
    Naibod,
    TrueSolarArc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcApplication {
    AllPoints,
    AnglesOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositeFramework {
    PointsOnly,
    MidpointAnglesAndCusps,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TechniquePointPosition {
    longitude_degrees: f64,
    motion_degrees_per_target_day: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TechniquePointPositionWire {
    longitude_degrees: f64,
    motion_degrees_per_target_day: Option<f64>,
}

impl TechniquePointPosition {
    pub fn new(longitude_degrees: f64, motion: Option<f64>) -> Result<Self, TechniqueError> {
        if !longitude_degrees.is_finite() || !(0.0..360.0).contains(&longitude_degrees) {
            return Err(TechniqueError::InvalidPoint);
        }
        if motion.is_some_and(|value| !value.is_finite()) {
            return Err(TechniqueError::InvalidPoint);
        }
        Ok(Self {
            longitude_degrees,
            motion_degrees_per_target_day: motion,
        })
    }
    pub fn longitude_degrees(self) -> f64 {
        self.longitude_degrees
    }
    pub fn motion_degrees_per_target_day(self) -> Option<f64> {
        self.motion_degrees_per_target_day
    }
}

impl<'de> Deserialize<'de> for TechniquePointPosition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = TechniquePointPositionWire::deserialize(deserializer)?;
        Self::new(wire.longitude_degrees, wire.motion_degrees_per_target_day)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SyntheticMethod {
    Harmonic {
        number: u16,
    },
    MidpointComposite {
        framework: CompositeFramework,
    },
    SolarArc {
        method: SolarArcMethod,
        application: ArcApplication,
        target_instant: UtcInstant,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SyntheticSources {
    Single {
        source: Box<DerivedChartArtifact>,
    },
    Composite {
        first: Box<DerivedChartArtifact>,
        second: Box<DerivedChartArtifact>,
    },
    SolarArc {
        natal: Box<DerivedChartArtifact>,
        progressed: Option<Box<ProgressedChartArtifact>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SyntheticChartArtifact {
    method: SyntheticMethod,
    sources: SyntheticSources,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    points: BTreeMap<ChartPointId, TechniquePointPosition>,
    house_cusps_degrees: Option<[f64; 12]>,
}

#[derive(Serialize)]
struct SyntheticRef<'a> {
    schema_version: u32,
    method: &'a SyntheticMethod,
    sources: &'a SyntheticSources,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    points: &'a BTreeMap<ChartPointId, TechniquePointPosition>,
    house_cusps_degrees: &'a Option<[f64; 12]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticWire {
    schema_version: u32,
    method: SyntheticMethod,
    sources: SyntheticSources,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    points: BTreeMap<ChartPointId, TechniquePointPosition>,
    house_cusps_degrees: Option<[f64; 12]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProgressedChartArtifact {
    method: ProgressionMethod,
    angle_policy: AnglePolicy,
    natal: Box<DerivedChartArtifact>,
    target_instant: UtcInstant,
    symbolic_instant: UtcInstant,
    chart: DerivedChartArtifact,
    points: BTreeMap<ChartPointId, TechniquePointPosition>,
}

#[derive(Serialize)]
struct ProgressedRef<'a> {
    schema_version: u32,
    method: ProgressionMethod,
    angle_policy: AnglePolicy,
    natal: &'a DerivedChartArtifact,
    target_instant: UtcInstant,
    symbolic_instant: UtcInstant,
    chart: &'a DerivedChartArtifact,
    points: &'a BTreeMap<ChartPointId, TechniquePointPosition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressedWire {
    schema_version: u32,
    method: ProgressionMethod,
    angle_policy: AnglePolicy,
    natal: DerivedChartArtifact,
    target_instant: UtcInstant,
    symbolic_instant: UtcInstant,
    chart: DerivedChartArtifact,
    points: BTreeMap<ChartPointId, TechniquePointPosition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DavisonChartArtifact {
    first: Box<DerivedChartArtifact>,
    second: Box<DerivedChartArtifact>,
    midpoint_instant: UtcInstant,
    midpoint_location: GeographicLocation,
    chart: DerivedChartArtifact,
}

#[derive(Serialize)]
struct DavisonRef<'a> {
    schema_version: u32,
    first: &'a DerivedChartArtifact,
    second: &'a DerivedChartArtifact,
    midpoint_instant: UtcInstant,
    midpoint_location: GeographicLocation,
    chart: &'a DerivedChartArtifact,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DavisonWire {
    schema_version: u32,
    first: DerivedChartArtifact,
    second: DerivedChartArtifact,
    midpoint_instant: UtcInstant,
    midpoint_location: GeographicLocation,
    chart: DerivedChartArtifact,
}

#[derive(Debug, Error)]
pub enum TechniqueError {
    #[error("invalid technique artifact JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported technique artifact schema version {0}")]
    UnsupportedSchema(u32),
    #[error("serialized technique values do not match their sources and policy")]
    DerivedValueMismatch,
    #[error("invalid technique point")]
    InvalidPoint,
    #[error("target instant precedes the natal instant")]
    TargetPrecedesNatal,
    #[error("harmonic number must be in 2..=360")]
    InvalidHarmonic,
    #[error("source charts must use the same zodiac and ayanamsa")]
    CoordinateFrameMismatch,
    #[error("source charts have no common points")]
    NoCommonPoints,
    #[error("Davison midpoint is undefined for antipodal locations")]
    AntipodalLocations,
    #[error("true solar arc requires a secondary-progressed Sun")]
    InvalidTrueSolarArcSource,
    #[error("invalid technique value: {0}")]
    InvalidValue(String),
    #[error("calculation failed: {0}")]
    Calculation(String),
}

pub fn symbolic_instant(
    natal: UtcInstant,
    target: UtcInstant,
    method: ProgressionMethod,
) -> Result<UtcInstant, TechniqueError> {
    let elapsed = target.as_datetime() - natal.as_datetime();
    if elapsed < Duration::zero() {
        return Err(TechniqueError::TargetPrecedesNatal);
    }
    let real_days = elapsed.num_milliseconds() as f64 / 86_400_000.0;
    let symbolic_days = match method {
        ProgressionMethod::Secondary => real_days / MEAN_TROPICAL_YEAR_DAYS,
        ProgressionMethod::TertiaryI => (real_days / MEAN_SIDEREAL_MONTH_DAYS).floor(),
        ProgressionMethod::TertiaryII => real_days / MEAN_SIDEREAL_MONTH_DAYS,
        ProgressionMethod::Minor => real_days * MEAN_SIDEREAL_MONTH_DAYS / MEAN_TROPICAL_YEAR_DAYS,
    };
    add_days(natal, symbolic_days)
}

pub fn cast_progressed(
    adapter: &impl EphemerisAdapter,
    natal: &DerivedChartArtifact,
    target: UtcInstant,
    method: ProgressionMethod,
    angle_policy: AnglePolicy,
) -> Result<ProgressedChartArtifact, TechniqueError> {
    let symbolic = symbolic_instant(natal.calculation().request().instant(), target, method)?;
    let chart = cast(
        adapter,
        natal.specification(),
        symbolic,
        natal.calculation().request().location(),
    )?;
    ProgressedChartArtifact::build(natal.clone(), target, method, angle_policy, chart)
}

pub fn harmonic(
    source: &DerivedChartArtifact,
    number: u16,
) -> Result<SyntheticChartArtifact, TechniqueError> {
    SyntheticChartArtifact::build(
        SyntheticMethod::Harmonic { number },
        SyntheticSources::Single {
            source: Box::new(source.clone()),
        },
    )
}

pub fn midpoint_composite(
    first: &DerivedChartArtifact,
    second: &DerivedChartArtifact,
    framework: CompositeFramework,
) -> Result<SyntheticChartArtifact, TechniqueError> {
    SyntheticChartArtifact::build(
        SyntheticMethod::MidpointComposite { framework },
        SyntheticSources::Composite {
            first: Box::new(first.clone()),
            second: Box::new(second.clone()),
        },
    )
}

pub fn solar_arc(
    natal: &DerivedChartArtifact,
    progressed: Option<&ProgressedChartArtifact>,
    target: UtcInstant,
    method: SolarArcMethod,
    application: ArcApplication,
) -> Result<SyntheticChartArtifact, TechniqueError> {
    SyntheticChartArtifact::build(
        SyntheticMethod::SolarArc {
            method,
            application,
            target_instant: target,
        },
        SyntheticSources::SolarArc {
            natal: Box::new(natal.clone()),
            progressed: progressed.cloned().map(Box::new),
        },
    )
}

pub fn cast_davison(
    adapter: &impl EphemerisAdapter,
    first: &DerivedChartArtifact,
    second: &DerivedChartArtifact,
    specification: &ChartSpecification,
) -> Result<DavisonChartArtifact, TechniqueError> {
    ensure_frame(first, second)?;
    let instant = midpoint_instant(
        first.calculation().request().instant(),
        second.calculation().request().instant(),
    )?;
    let location = spherical_midpoint(
        first.calculation().request().location(),
        second.calculation().request().location(),
    )?;
    let chart = cast(adapter, specification, instant, location)?;
    DavisonChartArtifact::build(first.clone(), second.clone(), chart)
}

impl ProgressedChartArtifact {
    fn build(
        natal: DerivedChartArtifact,
        target: UtcInstant,
        method: ProgressionMethod,
        angle_policy: AnglePolicy,
        chart: DerivedChartArtifact,
    ) -> Result<Self, TechniqueError> {
        let symbolic = symbolic_instant(natal.calculation().request().instant(), target, method)?;
        let request = chart.calculation().request();
        if request.instant() != symbolic
            || request.location() != natal.calculation().request().location()
            || request.options() != natal.calculation().request().options()
            || chart.specification() != natal.specification()
        {
            return Err(TechniqueError::DerivedValueMismatch);
        }
        let factor = match method {
            ProgressionMethod::Secondary => Some(1.0 / MEAN_TROPICAL_YEAR_DAYS),
            ProgressionMethod::TertiaryI => None,
            ProgressionMethod::TertiaryII => Some(1.0 / MEAN_SIDEREAL_MONTH_DAYS),
            ProgressionMethod::Minor => Some(MEAN_SIDEREAL_MONTH_DAYS / MEAN_TROPICAL_YEAR_DAYS),
        };
        let natal_points = raw_points(&natal)?;
        let mut points = BTreeMap::new();
        for (id, position) in raw_points(&chart)? {
            let fixed = angle_policy == AnglePolicy::NatalFixed && is_angle(id);
            let source = if fixed { natal_points[&id] } else { position };
            let motion = if fixed {
                None
            } else {
                factor.map(|value| source.longitude_speed_degrees_per_day() * value)
            };
            points.insert(
                id,
                TechniquePointPosition::new(source.longitude_degrees(), motion)?,
            );
        }
        Ok(Self {
            method,
            angle_policy,
            natal: Box::new(natal),
            target_instant: target,
            symbolic_instant: symbolic,
            chart,
            points,
        })
    }
    fn from_wire(wire: ProgressedWire) -> Result<Self, TechniqueError> {
        version(wire.schema_version)?;
        let expected = Self::build(
            wire.natal,
            wire.target_instant,
            wire.method,
            wire.angle_policy,
            wire.chart,
        )?;
        if expected.symbolic_instant != wire.symbolic_instant
            || !technique_points_match(&expected.points, &wire.points)
        {
            return Err(TechniqueError::DerivedValueMismatch);
        }
        Ok(expected)
    }
    pub fn symbolic_instant(&self) -> UtcInstant {
        self.symbolic_instant
    }
    pub fn chart(&self) -> &DerivedChartArtifact {
        &self.chart
    }
    pub fn points(&self) -> &BTreeMap<ChartPointId, TechniquePointPosition> {
        &self.points
    }
    pub fn zodiac(&self) -> Zodiac {
        self.chart.calculation().request().zodiac()
    }
    pub fn ayanamsa(&self) -> Option<Ayanamsa> {
        self.chart.calculation().request().ayanamsa()
    }
    pub fn method(&self) -> ProgressionMethod {
        self.method
    }
    pub fn to_json(&self) -> Result<String, TechniqueError> {
        Ok(serde_json::to_string(self)?)
    }
    pub fn from_json(input: &str) -> Result<Self, TechniqueError> {
        Ok(serde_json::from_str(input)?)
    }
    pub fn content_id(&self) -> Result<String, TechniqueError> {
        digest(self)
    }
}

impl SyntheticChartArtifact {
    fn build(method: SyntheticMethod, sources: SyntheticSources) -> Result<Self, TechniqueError> {
        match (&method, &sources) {
            (SyntheticMethod::Harmonic { number }, SyntheticSources::Single { source }) => {
                if !(2..=360).contains(number) {
                    return Err(TechniqueError::InvalidHarmonic);
                }
                let points = raw_points(source)?
                    .into_iter()
                    .map(|(id, p)| {
                        TechniquePointPosition::new(
                            (p.longitude_degrees() * f64::from(*number)).rem_euclid(360.0),
                            None,
                        )
                        .map(|p| (id, p))
                    })
                    .collect::<Result<_, _>>()?;
                let frame = frame(source);
                Ok(Self::new(method, sources, frame, points, None))
            }
            (
                SyntheticMethod::MidpointComposite { framework },
                SyntheticSources::Composite { first, second },
            ) => {
                ensure_frame(first, second)?;
                let a = raw_points(first)?;
                let b = raw_points(second)?;
                let mut points = BTreeMap::new();
                for (id, p) in a {
                    if let Some(q) = b.get(&id) {
                        points.insert(
                            id,
                            TechniquePointPosition::new(
                                forward_midpoint(p.longitude_degrees(), q.longitude_degrees()),
                                None,
                            )?,
                        );
                    }
                }
                if points.is_empty() {
                    return Err(TechniqueError::NoCommonPoints);
                }
                let cusps = if *framework == CompositeFramework::MidpointAnglesAndCusps {
                    let a = first.calculation().result().houses().cusps_degrees();
                    let b = second.calculation().result().houses().cusps_degrees();
                    let mut c = [0.0; 12];
                    for i in 0..12 {
                        c[i] = forward_midpoint(a[i], b[i]);
                    }
                    Some(c)
                } else {
                    None
                };
                let frame = frame(first);
                Ok(Self::new(method, sources, frame, points, cusps))
            }
            (
                SyntheticMethod::SolarArc {
                    method: arc_method,
                    application,
                    target_instant,
                },
                SyntheticSources::SolarArc { natal, progressed },
            ) => {
                let elapsed = target_instant.as_datetime()
                    - natal.calculation().request().instant().as_datetime();
                if elapsed < Duration::zero() {
                    return Err(TechniqueError::TargetPrecedesNatal);
                }
                let (arc, rate) = match arc_method {
                    SolarArcMethod::Naibod => (
                        elapsed.num_milliseconds() as f64 / 86_400_000.0 / MEAN_TROPICAL_YEAR_DAYS
                            * NAIBOD_DEGREES_PER_YEAR,
                        NAIBOD_DEGREES_PER_YEAR / MEAN_TROPICAL_YEAR_DAYS,
                    ),
                    SolarArcMethod::TrueSolarArc => {
                        let p = progressed
                            .as_deref()
                            .ok_or(TechniqueError::InvalidTrueSolarArcSource)?;
                        if p.method() != ProgressionMethod::Secondary
                            || p.target_instant != *target_instant
                        {
                            return Err(TechniqueError::InvalidTrueSolarArcSource);
                        }
                        let n = raw_points(natal)?[&ChartPointId::Sun].longitude_degrees();
                        let s = raw_points(p.chart())?[&ChartPointId::Sun];
                        (
                            (s.longitude_degrees() - n).rem_euclid(360.0),
                            s.longitude_speed_degrees_per_day() / MEAN_TROPICAL_YEAR_DAYS,
                        )
                    }
                };
                let mut points = BTreeMap::new();
                for (id, p) in raw_points(natal)? {
                    let moved = *application == ArcApplication::AllPoints || is_angle(id);
                    points.insert(
                        id,
                        TechniquePointPosition::new(
                            if moved {
                                (p.longitude_degrees() + arc).rem_euclid(360.0)
                            } else {
                                p.longitude_degrees()
                            },
                            moved.then_some(rate),
                        )?,
                    );
                }
                let frame = frame(natal);
                Ok(Self::new(method, sources, frame, points, None))
            }
            _ => Err(TechniqueError::DerivedValueMismatch),
        }
    }
    fn new(
        method: SyntheticMethod,
        sources: SyntheticSources,
        frame: (Zodiac, Option<Ayanamsa>),
        points: BTreeMap<ChartPointId, TechniquePointPosition>,
        cusps: Option<[f64; 12]>,
    ) -> Self {
        Self {
            method,
            sources,
            zodiac: frame.0,
            ayanamsa: frame.1,
            points,
            house_cusps_degrees: cusps,
        }
    }
    fn from_wire(wire: SyntheticWire) -> Result<Self, TechniqueError> {
        version(wire.schema_version)?;
        let expected = Self::build(wire.method, wire.sources)?;
        if expected.zodiac != wire.zodiac
            || expected.ayanamsa != wire.ayanamsa
            || !technique_points_match(&expected.points, &wire.points)
            || !cusps_equal(expected.house_cusps_degrees, wire.house_cusps_degrees)
        {
            return Err(TechniqueError::DerivedValueMismatch);
        }
        Ok(expected)
    }
    pub fn points(&self) -> &BTreeMap<ChartPointId, TechniquePointPosition> {
        &self.points
    }
    pub fn method(&self) -> &SyntheticMethod {
        &self.method
    }
    pub fn zodiac(&self) -> Zodiac {
        self.zodiac
    }
    pub fn ayanamsa(&self) -> Option<Ayanamsa> {
        self.ayanamsa
    }
    pub fn house_cusps_degrees(&self) -> Option<&[f64; 12]> {
        self.house_cusps_degrees.as_ref()
    }
    pub fn to_json(&self) -> Result<String, TechniqueError> {
        Ok(serde_json::to_string(self)?)
    }
    pub fn from_json(input: &str) -> Result<Self, TechniqueError> {
        Ok(serde_json::from_str(input)?)
    }
    pub fn content_id(&self) -> Result<String, TechniqueError> {
        digest(self)
    }
}

impl DavisonChartArtifact {
    fn build(
        first: DerivedChartArtifact,
        second: DerivedChartArtifact,
        chart: DerivedChartArtifact,
    ) -> Result<Self, TechniqueError> {
        ensure_frame(&first, &second)?;
        let instant = midpoint_instant(
            first.calculation().request().instant(),
            second.calculation().request().instant(),
        )?;
        let location = spherical_midpoint(
            first.calculation().request().location(),
            second.calculation().request().location(),
        )?;
        let source_frame = frame(&first);
        if frame(&chart) != source_frame {
            return Err(TechniqueError::CoordinateFrameMismatch);
        }
        if chart.calculation().request().instant() != instant
            || chart.calculation().request().location() != location
        {
            return Err(TechniqueError::DerivedValueMismatch);
        }
        Ok(Self {
            first: Box::new(first),
            second: Box::new(second),
            midpoint_instant: instant,
            midpoint_location: location,
            chart,
        })
    }
    fn from_wire(wire: DavisonWire) -> Result<Self, TechniqueError> {
        version(wire.schema_version)?;
        let expected = Self::build(wire.first, wire.second, wire.chart)?;
        if expected.midpoint_instant != wire.midpoint_instant
            || expected.midpoint_location != wire.midpoint_location
        {
            return Err(TechniqueError::DerivedValueMismatch);
        }
        Ok(expected)
    }
    pub fn midpoint_instant(&self) -> UtcInstant {
        self.midpoint_instant
    }
    pub fn midpoint_location(&self) -> GeographicLocation {
        self.midpoint_location
    }
    pub fn chart(&self) -> &DerivedChartArtifact {
        &self.chart
    }
    pub fn to_json(&self) -> Result<String, TechniqueError> {
        Ok(serde_json::to_string(self)?)
    }
    pub fn from_json(input: &str) -> Result<Self, TechniqueError> {
        Ok(serde_json::from_str(input)?)
    }
    pub fn content_id(&self) -> Result<String, TechniqueError> {
        digest(self)
    }
}

impl Serialize for SyntheticChartArtifact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        SyntheticRef {
            schema_version: SCHEMA_VERSION,
            method: &self.method,
            sources: &self.sources,
            zodiac: self.zodiac,
            ayanamsa: self.ayanamsa,
            points: &self.points,
            house_cusps_degrees: &self.house_cusps_degrees,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for SyntheticChartArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_wire(SyntheticWire::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
impl Serialize for ProgressedChartArtifact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ProgressedRef {
            schema_version: SCHEMA_VERSION,
            method: self.method,
            angle_policy: self.angle_policy,
            natal: &self.natal,
            target_instant: self.target_instant,
            symbolic_instant: self.symbolic_instant,
            chart: &self.chart,
            points: &self.points,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for ProgressedChartArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_wire(ProgressedWire::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
impl Serialize for DavisonChartArtifact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        DavisonRef {
            schema_version: SCHEMA_VERSION,
            first: &self.first,
            second: &self.second,
            midpoint_instant: self.midpoint_instant,
            midpoint_location: self.midpoint_location,
            chart: &self.chart,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for DavisonChartArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_wire(DavisonWire::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

fn cast(
    adapter: &impl EphemerisAdapter,
    specification: &ChartSpecification,
    instant: UtcInstant,
    location: GeographicLocation,
) -> Result<DerivedChartArtifact, TechniqueError> {
    let request: CalculationRequest = specification.request(instant, location);
    let result = adapter.calculate(&request).map_err(calc)?;
    let calculation = CalculationArtifact::new(request, result).map_err(calc)?;
    DerivedChartArtifact::new(calculation, specification.clone()).map_err(calc)
}
fn raw_points(
    chart: &DerivedChartArtifact,
) -> Result<BTreeMap<ChartPointId, astraeus_core::AngularPosition>, TechniqueError> {
    chart_point_positions(chart.calculation().result()).map_err(invalid)
}
fn version(value: u32) -> Result<(), TechniqueError> {
    if value == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(TechniqueError::UnsupportedSchema(value))
    }
}
fn digest(value: &impl Serialize) -> Result<String, TechniqueError> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value)?)
    ))
}
fn calc(error: impl ToString) -> TechniqueError {
    TechniqueError::Calculation(error.to_string())
}
fn invalid(error: impl ToString) -> TechniqueError {
    TechniqueError::InvalidValue(error.to_string())
}
fn ensure_frame(a: &DerivedChartArtifact, b: &DerivedChartArtifact) -> Result<(), TechniqueError> {
    let a = a.calculation().request();
    let b = b.calculation().request();
    if a.zodiac() != b.zodiac() || a.ayanamsa() != b.ayanamsa() {
        Err(TechniqueError::CoordinateFrameMismatch)
    } else {
        Ok(())
    }
}
fn frame(chart: &DerivedChartArtifact) -> (Zodiac, Option<Ayanamsa>) {
    let request = chart.calculation().request();
    (request.zodiac(), request.ayanamsa())
}
fn is_angle(id: ChartPointId) -> bool {
    matches!(
        id,
        ChartPointId::Ascendant
            | ChartPointId::Midheaven
            | ChartPointId::Descendant
            | ChartPointId::ImumCoeli
            | ChartPointId::Vertex
    )
}
fn forward_midpoint(a: f64, b: f64) -> f64 {
    let delta = (b - a).rem_euclid(360.0);
    let signed = if delta > 180.0 { delta - 360.0 } else { delta };
    (a + signed / 2.0).rem_euclid(360.0)
}
fn midpoint_instant(a: UtcInstant, b: UtcInstant) -> Result<UtcInstant, TechniqueError> {
    let delta = b.as_datetime() - a.as_datetime();
    parse_datetime(a.as_datetime() + Duration::milliseconds(delta.num_milliseconds() / 2))
}
fn spherical_midpoint(
    a: GeographicLocation,
    b: GeographicLocation,
) -> Result<GeographicLocation, TechniqueError> {
    let elevation = (a.elevation_meters() + b.elevation_meters()) / 2.0;
    let vector = |p: GeographicLocation| {
        let lat = p.latitude_degrees().to_radians();
        let lon = p.longitude_degrees().to_radians();
        [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
    };
    let a = vector(a);
    let b = vector(b);
    let sum = [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let norm = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    if norm < 1e-12 {
        return Err(TechniqueError::AntipodalLocations);
    }
    GeographicLocation::new(
        (sum[2] / norm).asin().to_degrees(),
        (sum[1] / norm).atan2(sum[0] / norm).to_degrees(),
        elevation,
    )
    .map_err(invalid)
}
fn add_days(instant: UtcInstant, days: f64) -> Result<UtcInstant, TechniqueError> {
    let ms = (days * 86_400_000.0).round();
    if !ms.is_finite() || ms > i64::MAX as f64 || ms < i64::MIN as f64 {
        return Err(invalid("symbolic duration is out of range"));
    }
    parse_datetime(instant.as_datetime() + Duration::milliseconds(ms as i64))
}
fn parse_datetime(value: DateTime<Utc>) -> Result<UtcInstant, TechniqueError> {
    UtcInstant::parse_rfc3339(&value.to_rfc3339_opts(SecondsFormat::Millis, true)).map_err(invalid)
}
fn cusps_equal(a: Option<[f64; 12]>, b: Option<[f64; 12]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a
            .iter()
            .zip(b)
            .all(|(a, b)| (a - b).abs() <= VALUE_TOLERANCE),
        _ => false,
    }
}

fn technique_points_match(
    first: &BTreeMap<ChartPointId, TechniquePointPosition>,
    second: &BTreeMap<ChartPointId, TechniquePointPosition>,
) -> bool {
    first.len() == second.len()
        && first.iter().all(|(id, first)| {
            second.get(id).is_some_and(|second| {
                angular_value_matches(first.longitude_degrees, second.longitude_degrees)
                    && optional_value_matches(
                        first.motion_degrees_per_target_day,
                        second.motion_degrees_per_target_day,
                    )
            })
        })
}

fn angular_value_matches(first: f64, second: f64) -> bool {
    let difference = (first - second).abs();
    difference.min(360.0 - difference) <= VALUE_TOLERANCE
}

fn optional_value_matches(first: Option<f64>, second: Option<f64>) -> bool {
    match (first, second) {
        (None, None) => true,
        (Some(first), Some(second)) => (first - second).abs() <= VALUE_TOLERANCE,
        _ => false,
    }
}

impl From<ValidationError> for TechniqueError {
    fn from(value: ValidationError) -> Self {
        invalid(value)
    }
}
