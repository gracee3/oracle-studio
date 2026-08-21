//! Exact-time astronomical event solving and ordinary event-chart artifacts.

use std::collections::{BTreeMap, BTreeSet};

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, Ayanamsa, CalculationProvenance, CelestialObject, EphemerisAdapter,
    GeographicLocation, UtcInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use chrono::{Duration, SecondsFormat};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TIME_TOLERANCE_SECONDS: f64 = 1.0;
pub const DEFAULT_ANGULAR_TOLERANCE_DEGREES: f64 = 1e-5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSelection {
    Previous,
    Nearest,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnFrame {
    ConfiguredZodiac,
    BirthEpochEclipticPrecessionCorrected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LunationKind {
    NewMoon,
    FullMoon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonalPoint {
    MarchEquinox,
    JuneSolstice,
    SeptemberEquinox,
    DecemberSolstice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventCoordinateFrame {
    Configured {
        zodiac: Zodiac,
        ayanamsa: Option<Ayanamsa>,
    },
    TropicalOfDate,
    BirthEpochEcliptic {
        epoch: UtcInstant,
    },
}

impl EventCoordinateFrame {
    pub fn configured(zodiac: Zodiac, ayanamsa: Option<Ayanamsa>) -> Result<Self, EventError> {
        match (zodiac, ayanamsa) {
            (Zodiac::Tropical, None) | (Zodiac::Sidereal, Some(_)) => {
                Ok(Self::Configured { zodiac, ayanamsa })
            }
            _ => Err(EventError::InvalidCoordinateFrame),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventPositionRequest {
    instant: UtcInstant,
    objects: Vec<CelestialObject>,
    frame: EventCoordinateFrame,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventPositionRequestWire {
    instant: UtcInstant,
    objects: Vec<CelestialObject>,
    frame: EventCoordinateFrame,
}

impl EventPositionRequest {
    pub fn new(
        instant: UtcInstant,
        objects: Vec<CelestialObject>,
        frame: EventCoordinateFrame,
    ) -> Result<Self, EventError> {
        if objects.is_empty() {
            return Err(EventError::EmptyObjectSet);
        }
        let mut unique = BTreeSet::new();
        if objects.iter().any(|object| !unique.insert(*object)) {
            return Err(EventError::DuplicateObject);
        }
        if let EventCoordinateFrame::Configured { zodiac, ayanamsa } = frame {
            EventCoordinateFrame::configured(zodiac, ayanamsa)?;
        }
        Ok(Self {
            instant,
            objects,
            frame,
        })
    }
    pub fn instant(&self) -> UtcInstant {
        self.instant
    }
    pub fn objects(&self) -> &[CelestialObject] {
        &self.objects
    }
    pub fn frame(&self) -> EventCoordinateFrame {
        self.frame
    }
}

impl<'de> Deserialize<'de> for EventPositionRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = EventPositionRequestWire::deserialize(deserializer)?;
        Self::new(wire.instant, wire.objects, wire.frame).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EventPositionSample {
    request: EventPositionRequest,
    positions: BTreeMap<CelestialObject, AngularPosition>,
    provenance: CalculationProvenance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventPositionSampleWire {
    request: EventPositionRequest,
    positions: BTreeMap<CelestialObject, AngularPosition>,
    provenance: CalculationProvenance,
}

impl EventPositionSample {
    pub fn new(
        request: EventPositionRequest,
        positions: BTreeMap<CelestialObject, AngularPosition>,
        provenance: CalculationProvenance,
    ) -> Result<Self, EventError> {
        EventPositionRequest::new(request.instant, request.objects.clone(), request.frame)?;
        if positions.len() != request.objects.len()
            || request
                .objects
                .iter()
                .any(|object| !positions.contains_key(object))
        {
            return Err(EventError::SampleObjectMismatch);
        }
        Ok(Self {
            request,
            positions,
            provenance,
        })
    }
    pub fn request(&self) -> &EventPositionRequest {
        &self.request
    }
    pub fn positions(&self) -> &BTreeMap<CelestialObject, AngularPosition> {
        &self.positions
    }
    pub fn provenance(&self) -> &CalculationProvenance {
        &self.provenance
    }
}
impl<'de> Deserialize<'de> for EventPositionSample {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = EventPositionSampleWire::deserialize(d)?;
        Self::new(w.request, w.positions, w.provenance).map_err(serde::de::Error::custom)
    }
}

pub trait EventPositionProvider {
    fn sample_event_positions(
        &self,
        request: &EventPositionRequest,
    ) -> Result<EventPositionSample, EventError>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventDefinition {
    Return {
        object: CelestialObject,
        frame: ReturnFrame,
        natal: Box<DerivedChartArtifact>,
    },
    Lunation {
        lunation: LunationKind,
    },
    Ingress {
        object: CelestialObject,
        target_longitude_degrees: f64,
    },
    Seasonal {
        point: SeasonalPoint,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSearch {
    pub reference: UtcInstant,
    pub selection: EventSelection,
    pub window_days: f64,
    pub scan_step_hours: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverMetadata {
    algorithm: String,
    time_tolerance_seconds: f64,
    angular_tolerance_degrees: f64,
    iterations: u32,
    residual_degrees: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventChartArtifact {
    definition: EventDefinition,
    search: EventSearch,
    target_longitude_degrees: f64,
    target_sample: Option<EventPositionSample>,
    exact_sample: EventPositionSample,
    solver: SolverMetadata,
    chart: DerivedChartArtifact,
}

#[derive(Serialize)]
struct ArtifactRef<'a> {
    schema_version: u32,
    definition: &'a EventDefinition,
    search: EventSearch,
    target_longitude_degrees: f64,
    target_sample: &'a Option<EventPositionSample>,
    exact_sample: &'a EventPositionSample,
    solver: &'a SolverMetadata,
    chart: &'a DerivedChartArtifact,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    schema_version: u32,
    definition: EventDefinition,
    search: EventSearch,
    target_longitude_degrees: f64,
    target_sample: Option<EventPositionSample>,
    exact_sample: EventPositionSample,
    solver: SolverMetadata,
    chart: DerivedChartArtifact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EclipseKind {
    Solar,
    Lunar,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EclipseClassification {
    Central,
    Noncentral,
    Total,
    Annular,
    Partial,
    Hybrid,
    Penumbral,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EclipseSearchDirection {
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct GlobalEclipseSearch {
    pub kind: EclipseKind,
    pub reference: UtcInstant,
    pub selection: EventSelection,
    pub window_days: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalEclipseSearchWire {
    kind: EclipseKind,
    reference: UtcInstant,
    selection: EventSelection,
    window_days: f64,
}

impl GlobalEclipseSearch {
    pub fn new(
        kind: EclipseKind,
        reference: UtcInstant,
        selection: EventSelection,
        window_days: f64,
    ) -> Result<Self, EventError> {
        if !window_days.is_finite() || window_days <= 0.0 {
            return Err(EventError::InvalidSearch);
        }
        Ok(Self {
            kind,
            reference,
            selection,
            window_days,
        })
    }
}
impl<'de> Deserialize<'de> for GlobalEclipseSearch {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = GlobalEclipseSearchWire::deserialize(d)?;
        Self::new(w.kind, w.reference, w.selection, w.window_days).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GlobalEclipseMaximum {
    kind: EclipseKind,
    exact_instant: UtcInstant,
    classifications: Vec<EclipseClassification>,
    native_flags: i32,
    time_conversion_residual_seconds: f64,
    provenance: CalculationProvenance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalEclipseMaximumWire {
    kind: EclipseKind,
    exact_instant: UtcInstant,
    classifications: Vec<EclipseClassification>,
    native_flags: i32,
    time_conversion_residual_seconds: f64,
    provenance: CalculationProvenance,
}

impl GlobalEclipseMaximum {
    pub fn new(
        kind: EclipseKind,
        exact_instant: UtcInstant,
        mut classifications: Vec<EclipseClassification>,
        native_flags: i32,
        time_conversion_residual_seconds: f64,
        provenance: CalculationProvenance,
    ) -> Result<Self, EventError> {
        classifications.sort();
        classifications.dedup();
        if native_flags <= 0
            || classifications != classifications_from_native_flags(native_flags)
            || !time_conversion_residual_seconds.is_finite()
            || !(0.0..=DEFAULT_TIME_TOLERANCE_SECONDS).contains(&time_conversion_residual_seconds)
        {
            return Err(EventError::InvalidEclipseMaximum);
        }
        Ok(Self {
            kind,
            exact_instant,
            classifications,
            native_flags,
            time_conversion_residual_seconds,
            provenance,
        })
    }
    pub fn kind(&self) -> EclipseKind {
        self.kind
    }
    pub fn exact_instant(&self) -> UtcInstant {
        self.exact_instant
    }
    pub fn classifications(&self) -> &[EclipseClassification] {
        &self.classifications
    }
    pub fn provenance(&self) -> &CalculationProvenance {
        &self.provenance
    }
}

fn classifications_from_native_flags(flags: i32) -> Vec<EclipseClassification> {
    let mut values = Vec::new();
    for (flag, classification) in [
        (1, EclipseClassification::Central),
        (2, EclipseClassification::Noncentral),
        (4, EclipseClassification::Total),
        (8, EclipseClassification::Annular),
        (16, EclipseClassification::Partial),
        (32, EclipseClassification::Hybrid),
        (64, EclipseClassification::Penumbral),
    ] {
        if flags & flag != 0 {
            values.push(classification);
        }
    }
    values
}
impl<'de> Deserialize<'de> for GlobalEclipseMaximum {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = GlobalEclipseMaximumWire::deserialize(d)?;
        Self::new(
            w.kind,
            w.exact_instant,
            w.classifications,
            w.native_flags,
            w.time_conversion_residual_seconds,
            w.provenance,
        )
        .map_err(serde::de::Error::custom)
    }
}

pub trait GlobalEclipseProvider {
    fn find_global_eclipse(
        &self,
        kind: EclipseKind,
        reference: UtcInstant,
        direction: EclipseSearchDirection,
    ) -> Result<GlobalEclipseMaximum, EventError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlobalEclipseChartArtifact {
    search: GlobalEclipseSearch,
    maximum: GlobalEclipseMaximum,
    chart: DerivedChartArtifact,
}

#[derive(Serialize)]
struct EclipseArtifactRef<'a> {
    schema_version: u32,
    search: GlobalEclipseSearch,
    maximum: &'a GlobalEclipseMaximum,
    chart: &'a DerivedChartArtifact,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EclipseArtifactWire {
    schema_version: u32,
    search: GlobalEclipseSearch,
    maximum: GlobalEclipseMaximum,
    chart: DerivedChartArtifact,
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("invalid event artifact JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported event artifact schema version {0}")]
    UnsupportedSchema(u32),
    #[error("search window and scan step must be finite and positive")]
    InvalidSearch,
    #[error("event longitude must be finite and in [0, 360)")]
    InvalidLongitude,
    #[error("invalid event coordinate frame")]
    InvalidCoordinateFrame,
    #[error("event position request must contain at least one object")]
    EmptyObjectSet,
    #[error("event position request contains duplicate objects")]
    DuplicateObject,
    #[error("event position sample object set does not match its request")]
    SampleObjectMismatch,
    #[error("chart specification does not request required object {0:?}")]
    MissingObject(CelestialObject),
    #[error("configured return chart and natal chart must use the same coordinate frame")]
    ReturnFrameMismatch,
    #[error("no event root found in the requested window")]
    NoRoot,
    #[error("event solver did not meet the residual target")]
    ResidualTooLarge,
    #[error("serialized event values do not match the event policy and exact sample")]
    DerivedValueMismatch,
    #[error("event provider failed: {0}")]
    Provider(String),
    #[error("calculation failed: {0}")]
    Calculation(String),
    #[error("invalid event time: {0}")]
    Time(String),
    #[error("invalid global eclipse maximum")]
    InvalidEclipseMaximum,
    #[error("global eclipse maximum is outside the requested window or direction")]
    EclipseOutsideSearch,
}

pub fn solve_return<P: EventPositionProvider + EphemerisAdapter>(
    provider: &P,
    specification: &ChartSpecification,
    location: GeographicLocation,
    natal: &DerivedChartArtifact,
    object: CelestialObject,
    frame: ReturnFrame,
    search: EventSearch,
) -> Result<EventChartArtifact, EventError> {
    solve_event(
        provider,
        specification,
        location,
        EventDefinition::Return {
            object,
            frame,
            natal: Box::new(natal.clone()),
        },
        search,
    )
}

pub fn solve_global_eclipse<P: GlobalEclipseProvider + EphemerisAdapter>(
    provider: &P,
    specification: &ChartSpecification,
    location: GeographicLocation,
    search: GlobalEclipseSearch,
) -> Result<GlobalEclipseChartArtifact, EventError> {
    GlobalEclipseSearch::new(
        search.kind,
        search.reference,
        search.selection,
        search.window_days,
    )?;
    for object in [CelestialObject::Sun, CelestialObject::Moon] {
        if !specification.calculation().objects().contains(&object) {
            return Err(EventError::MissingObject(object));
        }
    }
    let maximum = match search.selection {
        EventSelection::Previous => provider.find_global_eclipse(
            search.kind,
            search.reference,
            EclipseSearchDirection::Backward,
        )?,
        EventSelection::Next => provider.find_global_eclipse(
            search.kind,
            search.reference,
            EclipseSearchDirection::Forward,
        )?,
        EventSelection::Nearest => {
            let previous = provider.find_global_eclipse(
                search.kind,
                search.reference,
                EclipseSearchDirection::Backward,
            )?;
            let next = provider.find_global_eclipse(
                search.kind,
                search.reference,
                EclipseSearchDirection::Forward,
            )?;
            let before = (search.reference.as_datetime() - previous.exact_instant().as_datetime())
                .num_milliseconds()
                .abs();
            let after = (next.exact_instant().as_datetime() - search.reference.as_datetime())
                .num_milliseconds()
                .abs();
            if before <= after { previous } else { next }
        }
    };
    validate_eclipse_selection(search, &maximum)?;
    let request = specification.request(maximum.exact_instant(), location);
    let result = provider.calculate(&request).map_err(calc)?;
    let calculation = CalculationArtifact::new(request, result).map_err(calc)?;
    let chart = DerivedChartArtifact::new(calculation, specification.clone()).map_err(calc)?;
    GlobalEclipseChartArtifact::build(search, maximum, chart)
}

impl GlobalEclipseChartArtifact {
    fn build(
        search: GlobalEclipseSearch,
        maximum: GlobalEclipseMaximum,
        chart: DerivedChartArtifact,
    ) -> Result<Self, EventError> {
        GlobalEclipseSearch::new(
            search.kind,
            search.reference,
            search.selection,
            search.window_days,
        )?;
        validate_eclipse_selection(search, &maximum)?;
        if chart.calculation().request().instant() != maximum.exact_instant() {
            return Err(EventError::DerivedValueMismatch);
        }
        for object in [CelestialObject::Sun, CelestialObject::Moon] {
            if !chart.calculation().request().objects().contains(&object) {
                return Err(EventError::MissingObject(object));
            }
        }
        Ok(Self {
            search,
            maximum,
            chart,
        })
    }
    fn from_wire(w: EclipseArtifactWire) -> Result<Self, EventError> {
        if w.schema_version != SCHEMA_VERSION {
            return Err(EventError::UnsupportedSchema(w.schema_version));
        }
        Self::build(w.search, w.maximum, w.chart)
    }
    pub fn maximum(&self) -> &GlobalEclipseMaximum {
        &self.maximum
    }
    pub fn chart(&self) -> &DerivedChartArtifact {
        &self.chart
    }
    pub fn to_json(&self) -> Result<String, EventError> {
        Ok(serde_json::to_string(self)?)
    }
    pub fn from_json(input: &str) -> Result<Self, EventError> {
        Ok(serde_json::from_str(input)?)
    }
    pub fn content_id(&self) -> Result<String, EventError> {
        Ok(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(self)?)
        ))
    }
}
impl Serialize for GlobalEclipseChartArtifact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        EclipseArtifactRef {
            schema_version: SCHEMA_VERSION,
            search: self.search,
            maximum: &self.maximum,
            chart: &self.chart,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for GlobalEclipseChartArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_wire(EclipseArtifactWire::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

fn validate_eclipse_selection(
    search: GlobalEclipseSearch,
    maximum: &GlobalEclipseMaximum,
) -> Result<(), EventError> {
    if maximum.kind() != search.kind {
        return Err(EventError::InvalidEclipseMaximum);
    }
    let delta = maximum.exact_instant().as_datetime() - search.reference.as_datetime();
    let direction_ok = match search.selection {
        EventSelection::Previous => delta <= Duration::zero(),
        EventSelection::Next => delta >= Duration::zero(),
        EventSelection::Nearest => true,
    };
    if !direction_ok || delta.num_milliseconds().abs() as f64 > search.window_days * 86_400_000.0 {
        return Err(EventError::EclipseOutsideSearch);
    }
    Ok(())
}

pub fn solve_event<P: EventPositionProvider + EphemerisAdapter>(
    provider: &P,
    specification: &ChartSpecification,
    location: GeographicLocation,
    definition: EventDefinition,
    search: EventSearch,
) -> Result<EventChartArtifact, EventError> {
    validate_search(search)?;
    for object in required_objects(&definition) {
        if !specification.calculation().objects().contains(&object) {
            return Err(EventError::MissingObject(object));
        }
    }
    let frame = event_frame(&definition, specification)?;
    let (target, target_sample) = target_longitude(provider, &definition, frame)?;
    validate_longitude(target)?;
    let center = search.reference.as_datetime();
    let window_ms = (search.window_days * 86_400_000.0).round() as i64;
    let step_ms = (search.scan_step_hours * 3_600_000.0).round() as i64;
    let start = center - Duration::milliseconds(window_ms);
    let end = center + Duration::milliseconds(window_ms);
    let mut roots = Vec::new();
    let mut left = start;
    let mut left_value = residual(provider, &definition, frame, target, instant(left)?)?;
    while left < end {
        let right = (left + Duration::milliseconds(step_ms)).min(end);
        let right_value = residual(provider, &definition, frame, target, instant(right)?)?;
        if left_value.abs() <= DEFAULT_ANGULAR_TOLERANCE_DEGREES {
            roots.push((left, 0, left_value));
        }
        if left_value.signum() != right_value.signum() && (left_value - right_value).abs() < 180.0 {
            roots.push(bisect(
                provider,
                &definition,
                frame,
                target,
                left,
                right,
                left_value,
            )?);
        }
        left = right;
        left_value = right_value;
    }
    roots.sort_by_key(|root| root.0);
    roots.dedup_by(|a, b| (a.0 - b.0).num_seconds().abs() <= 1);
    let chosen = choose(&roots, center, search.selection).ok_or(EventError::NoRoot)?;
    if chosen.2.abs() > DEFAULT_ANGULAR_TOLERANCE_DEGREES {
        return Err(EventError::ResidualTooLarge);
    }
    let exact = instant(chosen.0)?;
    let sample = provider.sample_event_positions(&EventPositionRequest::new(
        exact,
        required_objects(&definition),
        frame,
    )?)?;
    let request = specification.request(exact, location);
    let result = provider.calculate(&request).map_err(calc)?;
    let calculation = CalculationArtifact::new(request, result).map_err(calc)?;
    let chart = DerivedChartArtifact::new(calculation, specification.clone()).map_err(calc)?;
    EventChartArtifact::build(
        definition,
        search,
        target,
        target_sample,
        sample,
        SolverMetadata {
            algorithm: "scan_bracket_bisection_v2".into(),
            time_tolerance_seconds: DEFAULT_TIME_TOLERANCE_SECONDS,
            angular_tolerance_degrees: DEFAULT_ANGULAR_TOLERANCE_DEGREES,
            iterations: chosen.1,
            residual_degrees: chosen.2.abs(),
        },
        chart,
    )
}

impl EventChartArtifact {
    fn build(
        definition: EventDefinition,
        search: EventSearch,
        target: f64,
        target_sample: Option<EventPositionSample>,
        sample: EventPositionSample,
        solver: SolverMetadata,
        chart: DerivedChartArtifact,
    ) -> Result<Self, EventError> {
        validate_search(search)?;
        validate_longitude(target)?;
        if chart.calculation().request().instant() != sample.request().instant() {
            return Err(EventError::DerivedValueMismatch);
        }
        let expected_objects = required_objects(&definition);
        if sample.request().objects() != expected_objects
            || sample.request().frame() != event_frame(&definition, chart.specification())?
        {
            return Err(EventError::DerivedValueMismatch);
        }
        validate_target_sample(
            &definition,
            sample.request().frame(),
            target,
            target_sample.as_ref(),
        )?;
        let expected_residual =
            residual_from_positions(&definition, target, sample.positions())?.abs();
        if solver.algorithm != "scan_bracket_bisection_v2"
            || solver.time_tolerance_seconds != DEFAULT_TIME_TOLERANCE_SECONDS
            || solver.angular_tolerance_degrees != DEFAULT_ANGULAR_TOLERANCE_DEGREES
            || solver.residual_degrees > DEFAULT_ANGULAR_TOLERANCE_DEGREES
            || (solver.residual_degrees - expected_residual).abs() > 1e-9
        {
            return Err(EventError::DerivedValueMismatch);
        }
        Ok(Self {
            definition,
            search,
            target_longitude_degrees: target,
            target_sample,
            exact_sample: sample,
            solver,
            chart,
        })
    }
    fn from_wire(w: ArtifactWire) -> Result<Self, EventError> {
        if w.schema_version != SCHEMA_VERSION {
            return Err(EventError::UnsupportedSchema(w.schema_version));
        }
        Self::build(
            w.definition,
            w.search,
            w.target_longitude_degrees,
            w.target_sample,
            w.exact_sample,
            w.solver,
            w.chart,
        )
    }
    pub fn exact_instant(&self) -> UtcInstant {
        self.exact_sample.request().instant()
    }
    pub fn residual_degrees(&self) -> f64 {
        self.solver.residual_degrees
    }
    pub fn chart(&self) -> &DerivedChartArtifact {
        &self.chart
    }
    pub fn exact_sample(&self) -> &EventPositionSample {
        &self.exact_sample
    }
    pub fn to_json(&self) -> Result<String, EventError> {
        Ok(serde_json::to_string(self)?)
    }
    pub fn from_json(input: &str) -> Result<Self, EventError> {
        Ok(serde_json::from_str(input)?)
    }
    pub fn content_id(&self) -> Result<String, EventError> {
        Ok(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(self)?)
        ))
    }
}
impl Serialize for EventChartArtifact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ArtifactRef {
            schema_version: SCHEMA_VERSION,
            definition: &self.definition,
            search: self.search,
            target_longitude_degrees: self.target_longitude_degrees,
            target_sample: &self.target_sample,
            exact_sample: &self.exact_sample,
            solver: &self.solver,
            chart: &self.chart,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for EventChartArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_wire(ArtifactWire::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

fn validate_search(search: EventSearch) -> Result<(), EventError> {
    if !search.window_days.is_finite()
        || search.window_days <= 0.0
        || !search.scan_step_hours.is_finite()
        || search.scan_step_hours <= 0.0
    {
        Err(EventError::InvalidSearch)
    } else {
        Ok(())
    }
}
fn validate_longitude(value: f64) -> Result<(), EventError> {
    if value.is_finite() && (0.0..360.0).contains(&value) {
        Ok(())
    } else {
        Err(EventError::InvalidLongitude)
    }
}
fn required_objects(definition: &EventDefinition) -> Vec<CelestialObject> {
    match definition {
        EventDefinition::Return { object, .. } | EventDefinition::Ingress { object, .. } => {
            vec![*object]
        }
        EventDefinition::Lunation { .. } => vec![CelestialObject::Sun, CelestialObject::Moon],
        EventDefinition::Seasonal { .. } => vec![CelestialObject::Sun],
    }
}
fn configured_frame(spec: &ChartSpecification) -> Result<EventCoordinateFrame, EventError> {
    EventCoordinateFrame::configured(spec.calculation().zodiac(), spec.calculation().ayanamsa())
}
fn event_frame(
    definition: &EventDefinition,
    spec: &ChartSpecification,
) -> Result<EventCoordinateFrame, EventError> {
    match definition {
        EventDefinition::Return {
            frame: ReturnFrame::ConfiguredZodiac,
            natal,
            ..
        } => {
            let frame = configured_frame(spec)?;
            if frame != configured_frame(natal.specification())? {
                return Err(EventError::ReturnFrameMismatch);
            }
            Ok(frame)
        }
        EventDefinition::Return {
            frame: ReturnFrame::BirthEpochEclipticPrecessionCorrected,
            natal,
            ..
        } => Ok(EventCoordinateFrame::BirthEpochEcliptic {
            epoch: natal.calculation().request().instant(),
        }),
        EventDefinition::Ingress { .. } => configured_frame(spec),
        EventDefinition::Lunation { .. } | EventDefinition::Seasonal { .. } => {
            Ok(EventCoordinateFrame::TropicalOfDate)
        }
    }
}
fn target_longitude(
    provider: &impl EventPositionProvider,
    definition: &EventDefinition,
    frame: EventCoordinateFrame,
) -> Result<(f64, Option<EventPositionSample>), EventError> {
    match definition {
        EventDefinition::Return { object, natal, .. } => {
            let sample = provider.sample_event_positions(&EventPositionRequest::new(
                natal.calculation().request().instant(),
                vec![*object],
                frame,
            )?)?;
            Ok((sample.positions()[object].longitude_degrees(), Some(sample)))
        }
        EventDefinition::Ingress {
            target_longitude_degrees,
            ..
        } => Ok((*target_longitude_degrees, None)),
        EventDefinition::Lunation {
            lunation: LunationKind::NewMoon,
        } => Ok((0.0, None)),
        EventDefinition::Lunation {
            lunation: LunationKind::FullMoon,
        } => Ok((180.0, None)),
        EventDefinition::Seasonal { point } => Ok((
            match point {
                SeasonalPoint::MarchEquinox => 0.0,
                SeasonalPoint::JuneSolstice => 90.0,
                SeasonalPoint::SeptemberEquinox => 180.0,
                SeasonalPoint::DecemberSolstice => 270.0,
            },
            None,
        )),
    }
}
fn validate_target_sample(
    definition: &EventDefinition,
    frame: EventCoordinateFrame,
    target: f64,
    sample: Option<&EventPositionSample>,
) -> Result<(), EventError> {
    match definition {
        EventDefinition::Return {
            object,
            natal,
            frame: ReturnFrame::ConfiguredZodiac,
        } => {
            if frame != configured_frame(natal.specification())? {
                return Err(EventError::DerivedValueMismatch);
            }
            validate_return_target_sample(natal, *object, frame, target, sample)
        }
        EventDefinition::Return {
            object,
            natal,
            frame: ReturnFrame::BirthEpochEclipticPrecessionCorrected,
        } => validate_return_target_sample(natal, *object, frame, target, sample),
        EventDefinition::Ingress {
            target_longitude_degrees,
            ..
        } => validate_fixed_target(*target_longitude_degrees, target, sample),
        EventDefinition::Lunation {
            lunation: LunationKind::NewMoon,
        } => validate_fixed_target(0.0, target, sample),
        EventDefinition::Lunation {
            lunation: LunationKind::FullMoon,
        } => validate_fixed_target(180.0, target, sample),
        EventDefinition::Seasonal { point } => validate_fixed_target(
            match point {
                SeasonalPoint::MarchEquinox => 0.0,
                SeasonalPoint::JuneSolstice => 90.0,
                SeasonalPoint::SeptemberEquinox => 180.0,
                SeasonalPoint::DecemberSolstice => 270.0,
            },
            target,
            sample,
        ),
    }
}

fn validate_return_target_sample(
    natal: &DerivedChartArtifact,
    object: CelestialObject,
    frame: EventCoordinateFrame,
    target: f64,
    sample: Option<&EventPositionSample>,
) -> Result<(), EventError> {
    let sample = sample.ok_or(EventError::DerivedValueMismatch)?;
    if sample.request().instant() != natal.calculation().request().instant()
        || sample.request().objects() != [object]
        || sample.request().frame() != frame
        || angular_difference(sample.positions()[&object].longitude_degrees(), target) > 1e-9
    {
        return Err(EventError::DerivedValueMismatch);
    }
    Ok(())
}

fn validate_fixed_target(
    expected: f64,
    target: f64,
    sample: Option<&EventPositionSample>,
) -> Result<(), EventError> {
    if sample.is_some() || angular_difference(expected, target) > 1e-9 {
        Err(EventError::DerivedValueMismatch)
    } else {
        Ok(())
    }
}
fn residual(
    provider: &impl EventPositionProvider,
    definition: &EventDefinition,
    frame: EventCoordinateFrame,
    target: f64,
    at: UtcInstant,
) -> Result<f64, EventError> {
    let sample = provider.sample_event_positions(&EventPositionRequest::new(
        at,
        required_objects(definition),
        frame,
    )?)?;
    residual_from_positions(definition, target, sample.positions())
}
fn residual_from_positions(
    definition: &EventDefinition,
    target: f64,
    positions: &BTreeMap<CelestialObject, AngularPosition>,
) -> Result<f64, EventError> {
    let longitude = match definition {
        EventDefinition::Lunation { .. } => (positions[&CelestialObject::Moon].longitude_degrees()
            - positions[&CelestialObject::Sun].longitude_degrees())
        .rem_euclid(360.0),
        EventDefinition::Return { object, .. } | EventDefinition::Ingress { object, .. } => {
            positions[object].longitude_degrees()
        }
        EventDefinition::Seasonal { .. } => positions[&CelestialObject::Sun].longitude_degrees(),
    };
    Ok(signed(longitude - target))
}
fn bisect(
    provider: &impl EventPositionProvider,
    definition: &EventDefinition,
    frame: EventCoordinateFrame,
    target: f64,
    mut left: chrono::DateTime<chrono::Utc>,
    mut right: chrono::DateTime<chrono::Utc>,
    mut left_value: f64,
) -> Result<(chrono::DateTime<chrono::Utc>, u32, f64), EventError> {
    let mut iterations = 0;
    while (right - left).num_milliseconds() as f64 > DEFAULT_TIME_TOLERANCE_SECONDS * 1000.0
        && iterations < 80
    {
        let middle = left + Duration::milliseconds((right - left).num_milliseconds() / 2);
        let value = residual(provider, definition, frame, target, instant(middle)?)?;
        if value.abs() <= DEFAULT_ANGULAR_TOLERANCE_DEGREES {
            return Ok((middle, iterations + 1, value));
        }
        if left_value.signum() == value.signum() {
            left = middle;
            left_value = value;
        } else {
            right = middle;
        }
        iterations += 1;
    }
    let middle = left + Duration::milliseconds((right - left).num_milliseconds() / 2);
    let value = residual(provider, definition, frame, target, instant(middle)?)?;
    Ok((middle, iterations, value))
}
fn choose(
    roots: &[(chrono::DateTime<chrono::Utc>, u32, f64)],
    reference: chrono::DateTime<chrono::Utc>,
    selection: EventSelection,
) -> Option<(chrono::DateTime<chrono::Utc>, u32, f64)> {
    let eligible = roots.iter().copied().filter(|root| match selection {
        EventSelection::Previous => root.0 <= reference,
        EventSelection::Next => root.0 >= reference,
        EventSelection::Nearest => true,
    });
    match selection {
        EventSelection::Previous => eligible.max_by_key(|root| root.0),
        EventSelection::Next => eligible.min_by_key(|root| root.0),
        EventSelection::Nearest => eligible
            .min_by_key(|root| ((root.0 - reference).num_milliseconds().abs() / 1000, root.0)),
    }
}
fn angular_difference(a: f64, b: f64) -> f64 {
    signed(a - b).abs()
}
fn signed(value: f64) -> f64 {
    let value = value.rem_euclid(360.0);
    if value > 180.0 { value - 360.0 } else { value }
}
fn instant(value: chrono::DateTime<chrono::Utc>) -> Result<UtcInstant, EventError> {
    UtcInstant::parse_rfc3339(&value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .map_err(|error| EventError::Time(error.to_string()))
}
fn calc(error: impl ToString) -> EventError {
    EventError::Calculation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_nearest_tie_chooses_earlier() {
        let reference = chrono::DateTime::parse_from_rfc3339("2000-01-02T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let earlier = reference - Duration::days(1);
        let later = reference + Duration::days(1);
        assert_eq!(
            choose(
                &[(later, 1, 0.0), (earlier, 1, 0.0)],
                reference,
                EventSelection::Nearest
            )
            .unwrap()
            .0,
            earlier
        );
    }
}
