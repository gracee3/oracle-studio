//! Canonical waveform-ready aspect timelines and exact-pass solving.

use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinition, AspectKind, AspectMeasurement, AspectPhase,
    CalculationProvenance, CelestialObject, ChartPointId, UtcInstant, chart_point_positions,
    measure_aspect,
};
use astraeus_events::{
    EventCoordinateFrame, EventPositionProvider, EventPositionRequest, EventPositionSample,
};
use chrono::Duration;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_OUTPUT_SAMPLES: usize = 100_000;
pub const MAX_SCAN_INTERVALS: usize = 100_000;
pub const MAX_SCAN_STEP_SECONDS: u64 = 21_600;
pub const TIME_TOLERANCE_SECONDS: f64 = 1.0;
pub const ANGULAR_TOLERANCE_DEGREES: f64 = 1e-5;
pub const MAX_REFINEMENT_ITERATIONS: u32 = 80;

const DERIVED_TOLERANCE: f64 = 1e-12;
const ALGORITHM: &str = "continuous_relative_longitude_bisection_v1";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AspectTimelineSubject {
    MovingFixed {
        chart: Box<CalculationArtifact>,
        fixed_point: ChartPointId,
        moving_object: CelestialObject,
    },
    MovingMoving {
        first_object: CelestialObject,
        second_object: CelestialObject,
        frame: EventCoordinateFrame,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SubjectWire {
    MovingFixed {
        chart: Box<CalculationArtifact>,
        fixed_point: ChartPointId,
        moving_object: CelestialObject,
    },
    MovingMoving {
        first_object: CelestialObject,
        second_object: CelestialObject,
        frame: EventCoordinateFrame,
    },
}

impl AspectTimelineSubject {
    pub fn moving_fixed(
        chart: CalculationArtifact,
        fixed_point: ChartPointId,
        moving_object: CelestialObject,
    ) -> Result<Self, TimelineError> {
        let subject = Self::MovingFixed {
            chart: Box::new(chart),
            fixed_point,
            moving_object,
        };
        subject.fixed_position()?;
        Ok(subject)
    }

    pub fn moving_moving(
        first_object: CelestialObject,
        second_object: CelestialObject,
        frame: EventCoordinateFrame,
    ) -> Result<Self, TimelineError> {
        if first_object == second_object {
            return Err(TimelineError::DuplicateMovingObjects);
        }
        validate_frame(frame)?;
        Ok(Self::MovingMoving {
            first_object,
            second_object,
            frame,
        })
    }

    fn from_wire(wire: SubjectWire) -> Result<Self, TimelineError> {
        match wire {
            SubjectWire::MovingFixed {
                chart,
                fixed_point,
                moving_object,
            } => {
                let subject = Self::MovingFixed {
                    chart,
                    fixed_point,
                    moving_object,
                };
                subject.fixed_position()?;
                Ok(subject)
            }
            SubjectWire::MovingMoving {
                first_object,
                second_object,
                frame,
            } => Self::moving_moving(first_object, second_object, frame),
        }
    }

    fn fixed_position(&self) -> Result<Option<AngularPosition>, TimelineError> {
        match self {
            Self::MovingFixed {
                chart, fixed_point, ..
            } => {
                let points = chart_point_positions(chart.result())
                    .map_err(|_| TimelineError::FixedPointUnavailable(*fixed_point))?;
                let position = points
                    .get(fixed_point)
                    .ok_or(TimelineError::FixedPointUnavailable(*fixed_point))?;
                Ok(Some(AngularPosition::new(
                    position.longitude_degrees(),
                    0.0,
                )?))
            }
            Self::MovingMoving { .. } => Ok(None),
        }
    }

    fn frame(&self) -> Result<EventCoordinateFrame, TimelineError> {
        match self {
            Self::MovingFixed { chart, .. } => Ok(EventCoordinateFrame::configured(
                chart.request().zodiac(),
                chart.request().ayanamsa(),
            )?),
            Self::MovingMoving { frame, .. } => Ok(*frame),
        }
    }

    fn objects(&self) -> Vec<CelestialObject> {
        match self {
            Self::MovingFixed { moving_object, .. } => vec![*moving_object],
            Self::MovingMoving {
                first_object,
                second_object,
                ..
            } => vec![*first_object, *second_object],
        }
    }
}

impl<'de> Deserialize<'de> for AspectTimelineSubject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_wire(SubjectWire::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AspectTimelineRequest {
    subject: AspectTimelineSubject,
    aspect: AspectDefinition,
    start: UtcInstant,
    end: UtcInstant,
    cadence_seconds: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestWire {
    subject: AspectTimelineSubject,
    aspect: AspectDefinition,
    start: UtcInstant,
    end: UtcInstant,
    cadence_seconds: u64,
}

impl AspectTimelineRequest {
    pub fn new(
        subject: AspectTimelineSubject,
        aspect: AspectDefinition,
        start: UtcInstant,
        end: UtcInstant,
        cadence_seconds: u64,
    ) -> Result<Self, TimelineError> {
        if end < start {
            return Err(TimelineError::InvalidRange);
        }
        if cadence_seconds == 0 {
            return Err(TimelineError::InvalidCadence);
        }
        if aspect.orb_degrees() <= 0.0 {
            return Err(TimelineError::NonPositiveOrb);
        }
        subject.fixed_position()?;
        validate_frame(subject.frame()?)?;
        let request = Self {
            subject,
            aspect,
            start,
            end,
            cadence_seconds,
        };
        output_times(&request)?;
        scan_times(&request)?;
        Ok(request)
    }

    pub fn subject(&self) -> &AspectTimelineSubject {
        &self.subject
    }
    pub fn aspect(&self) -> AspectDefinition {
        self.aspect
    }
    pub fn start(&self) -> UtcInstant {
        self.start
    }
    pub fn end(&self) -> UtcInstant {
        self.end
    }
    pub fn cadence_seconds(&self) -> u64 {
        self.cadence_seconds
    }
}

impl<'de> Deserialize<'de> for AspectTimelineRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = RequestWire::deserialize(deserializer)?;
        Self::new(
            wire.subject,
            wire.aspect,
            wire.start,
            wire.end,
            wire.cadence_seconds,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AspectTimelineSample {
    instant: UtcInstant,
    first_position: AngularPosition,
    second_position: AngularPosition,
    signed_separation_degrees: f64,
    separation_degrees: f64,
    signed_aspect_error_degrees: f64,
    angular_error_degrees: f64,
    relative_speed_degrees_per_day: f64,
    phase: AspectPhase,
    within_orb: bool,
    proximity: f64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleWire {
    instant: UtcInstant,
    first_position: AngularPosition,
    second_position: AngularPosition,
    signed_separation_degrees: f64,
    separation_degrees: f64,
    signed_aspect_error_degrees: f64,
    angular_error_degrees: f64,
    relative_speed_degrees_per_day: f64,
    phase: AspectPhase,
    within_orb: bool,
    proximity: f64,
}

impl AspectTimelineSample {
    fn from_positions(
        instant: UtcInstant,
        first_position: AngularPosition,
        second_position: AngularPosition,
        aspect: AspectDefinition,
    ) -> Self {
        let measurement = measure_aspect(first_position, second_position, aspect.kind());
        Self::from_measurement(
            instant,
            first_position,
            second_position,
            measurement,
            aspect,
        )
    }

    fn from_measurement(
        instant: UtcInstant,
        first_position: AngularPosition,
        second_position: AngularPosition,
        measurement: AspectMeasurement,
        aspect: AspectDefinition,
    ) -> Self {
        let error = measurement.angular_error_degrees();
        Self {
            instant,
            first_position,
            second_position,
            signed_separation_degrees: measurement.signed_separation_degrees(),
            separation_degrees: measurement.separation_degrees(),
            signed_aspect_error_degrees: measurement.signed_aspect_error_degrees(),
            angular_error_degrees: error,
            relative_speed_degrees_per_day: measurement.relative_speed_degrees_per_day(),
            phase: measurement.phase(),
            within_orb: error <= aspect.orb_degrees(),
            proximity: (1.0 - error / aspect.orb_degrees()).clamp(0.0, 1.0),
        }
    }

    fn from_wire(wire: SampleWire, aspect: AspectDefinition) -> Result<Self, TimelineError> {
        let expected = Self::from_positions(
            wire.instant,
            wire.first_position,
            wire.second_position,
            aspect,
        );
        let actual = Self {
            instant: wire.instant,
            first_position: wire.first_position,
            second_position: wire.second_position,
            signed_separation_degrees: wire.signed_separation_degrees,
            separation_degrees: wire.separation_degrees,
            signed_aspect_error_degrees: wire.signed_aspect_error_degrees,
            angular_error_degrees: wire.angular_error_degrees,
            relative_speed_degrees_per_day: wire.relative_speed_degrees_per_day,
            phase: wire.phase,
            within_orb: wire.within_orb,
            proximity: wire.proximity,
        };
        if !samples_match(&actual, &expected) {
            return Err(TimelineError::DerivedValueMismatch);
        }
        Ok(actual)
    }

    pub fn instant(&self) -> UtcInstant {
        self.instant
    }
    pub fn first_position(&self) -> AngularPosition {
        self.first_position
    }
    pub fn second_position(&self) -> AngularPosition {
        self.second_position
    }
    pub fn signed_separation_degrees(&self) -> f64 {
        self.signed_separation_degrees
    }
    pub fn separation_degrees(&self) -> f64 {
        self.separation_degrees
    }
    pub fn signed_aspect_error_degrees(&self) -> f64 {
        self.signed_aspect_error_degrees
    }
    pub fn angular_error_degrees(&self) -> f64 {
        self.angular_error_degrees
    }
    pub fn relative_speed_degrees_per_day(&self) -> f64 {
        self.relative_speed_degrees_per_day
    }
    pub fn phase(&self) -> AspectPhase {
        self.phase
    }
    pub fn within_orb(&self) -> bool {
        self.within_orb
    }
    pub fn proximity(&self) -> f64 {
        self.proximity
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExactAspectPass {
    sample: AspectTimelineSample,
    target_relative_longitude_degrees: f64,
    bracket_seconds: f64,
    iterations: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactPassWire {
    sample: SampleWire,
    target_relative_longitude_degrees: f64,
    bracket_seconds: f64,
    iterations: u32,
}

impl ExactAspectPass {
    pub fn sample(&self) -> &AspectTimelineSample {
        &self.sample
    }
    pub fn target_relative_longitude_degrees(&self) -> f64 {
        self.target_relative_longitude_degrees
    }
    pub fn bracket_seconds(&self) -> f64 {
        self.bracket_seconds
    }
    pub fn iterations(&self) -> u32 {
        self.iterations
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AspectWindow {
    start: AspectTimelineSample,
    end: AspectTimelineSample,
    start_truncated: bool,
    end_truncated: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowWire {
    start: SampleWire,
    end: SampleWire,
    start_truncated: bool,
    end_truncated: bool,
}

impl AspectWindow {
    pub fn start(&self) -> &AspectTimelineSample {
        &self.start
    }
    pub fn end(&self) -> &AspectTimelineSample {
        &self.end
    }
    pub fn start_truncated(&self) -> bool {
        self.start_truncated
    }
    pub fn end_truncated(&self) -> bool {
        self.end_truncated
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineSolverMetadata {
    algorithm: String,
    scan_step_seconds: u64,
    time_tolerance_seconds: f64,
    angular_tolerance_degrees: f64,
    max_iterations: u32,
    output_sample_count: u32,
    scan_interval_count: u32,
}

impl TimelineSolverMetadata {
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }
    pub fn scan_step_seconds(&self) -> u64 {
        self.scan_step_seconds
    }
    pub fn time_tolerance_seconds(&self) -> f64 {
        self.time_tolerance_seconds
    }
    pub fn angular_tolerance_degrees(&self) -> f64 {
        self.angular_tolerance_degrees
    }
    pub fn max_iterations(&self) -> u32 {
        self.max_iterations
    }
    pub fn output_sample_count(&self) -> u32 {
        self.output_sample_count
    }
    pub fn scan_interval_count(&self) -> u32 {
        self.scan_interval_count
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AspectTimelineArtifact {
    request: AspectTimelineRequest,
    provider_provenance: CalculationProvenance,
    samples: Vec<AspectTimelineSample>,
    exact_passes: Vec<ExactAspectPass>,
    windows: Vec<AspectWindow>,
    solver: TimelineSolverMetadata,
}

#[derive(Serialize)]
struct ArtifactRef<'a> {
    schema_version: u32,
    request: &'a AspectTimelineRequest,
    provider_provenance: &'a CalculationProvenance,
    samples: &'a [AspectTimelineSample],
    exact_passes: &'a [ExactAspectPass],
    windows: &'a [AspectWindow],
    solver: &'a TimelineSolverMetadata,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    schema_version: u32,
    request: AspectTimelineRequest,
    provider_provenance: CalculationProvenance,
    samples: Vec<SampleWire>,
    exact_passes: Vec<ExactPassWire>,
    windows: Vec<WindowWire>,
    solver: TimelineSolverMetadata,
}

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("invalid aspect timeline JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported aspect timeline schema version {0}")]
    UnsupportedSchema(u32),
    #[error("timeline end must not precede its start")]
    InvalidRange,
    #[error("timeline cadence must be a positive whole number of seconds")]
    InvalidCadence,
    #[error("timeline aspect orb must be greater than zero")]
    NonPositiveOrb,
    #[error("moving/moving timeline objects must be distinct")]
    DuplicateMovingObjects,
    #[error("fixed chart point {0:?} is unavailable in the embedded chart")]
    FixedPointUnavailable(ChartPointId),
    #[error("timeline coordinate frame is invalid")]
    InvalidCoordinateFrame,
    #[error("timeline exceeds the {MAX_OUTPUT_SAMPLES} output-sample limit")]
    TooManyOutputSamples,
    #[error("timeline exceeds the {MAX_SCAN_INTERVALS} internal scan-interval limit")]
    TooManyScanIntervals,
    #[error("timeline provider failed: {0}")]
    Provider(String),
    #[error("timeline provider returned inconsistent provenance")]
    ProviderProvenanceMismatch,
    #[error("timeline provider returned an inconsistent request or object set")]
    ProviderSampleMismatch,
    #[error("timeline instant is outside the supported range")]
    TimeRange,
    #[error("timeline solver did not meet its time and angular tolerances")]
    SolverDidNotConverge,
    #[error("serialized timeline derived values are inconsistent")]
    DerivedValueMismatch,
    #[error("serialized timeline structure is invalid")]
    InvalidArtifact,
    #[error(transparent)]
    Core(#[from] astraeus_core::ValidationError),
    #[error("event provider request failed: {0}")]
    Event(#[from] astraeus_events::EventError),
}

#[derive(Clone)]
struct Evaluation {
    first: AngularPosition,
    second: AngularPosition,
    measurement: AspectMeasurement,
}

struct TimelineSampler<'a, P> {
    provider: &'a P,
    request: &'a AspectTimelineRequest,
    fixed: Option<AngularPosition>,
    frame: EventCoordinateFrame,
    objects: Vec<CelestialObject>,
    expected_provenance: Option<CalculationProvenance>,
    cache: BTreeMap<UtcInstant, Evaluation>,
}

impl<'a, P: EventPositionProvider> TimelineSampler<'a, P> {
    fn new(provider: &'a P, request: &'a AspectTimelineRequest) -> Result<Self, TimelineError> {
        Ok(Self {
            provider,
            request,
            fixed: request.subject.fixed_position()?,
            frame: request.subject.frame()?,
            objects: request.subject.objects(),
            expected_provenance: None,
            cache: BTreeMap::new(),
        })
    }

    fn evaluate(&mut self, instant: UtcInstant) -> Result<Evaluation, TimelineError> {
        if let Some(value) = self.cache.get(&instant) {
            return Ok(value.clone());
        }
        let provider_request =
            EventPositionRequest::new(instant, self.objects.clone(), self.frame)?;
        let sample = self
            .provider
            .sample_event_positions(&provider_request)
            .map_err(|error| TimelineError::Provider(error.to_string()))?;
        if sample.request() != &provider_request {
            return Err(TimelineError::ProviderSampleMismatch);
        }
        match &self.expected_provenance {
            Some(expected) if expected != sample.provenance() => {
                return Err(TimelineError::ProviderProvenanceMismatch);
            }
            None => self.expected_provenance = Some(sample.provenance().clone()),
            _ => {}
        }
        let (first, second) = self.positions(&sample)?;
        let value = Evaluation {
            first,
            second,
            measurement: measure_aspect(first, second, self.request.aspect.kind()),
        };
        self.cache.insert(instant, value.clone());
        Ok(value)
    }

    fn positions(
        &self,
        sample: &EventPositionSample,
    ) -> Result<(AngularPosition, AngularPosition), TimelineError> {
        match &self.request.subject {
            AspectTimelineSubject::MovingFixed { moving_object, .. } => Ok((
                self.fixed.ok_or(TimelineError::InvalidArtifact)?,
                *sample
                    .positions()
                    .get(moving_object)
                    .ok_or(TimelineError::ProviderSampleMismatch)?,
            )),
            AspectTimelineSubject::MovingMoving {
                first_object,
                second_object,
                ..
            } => Ok((
                *sample
                    .positions()
                    .get(first_object)
                    .ok_or(TimelineError::ProviderSampleMismatch)?,
                *sample
                    .positions()
                    .get(second_object)
                    .ok_or(TimelineError::ProviderSampleMismatch)?,
            )),
        }
    }
}

#[derive(Clone)]
struct ContinuousPoint {
    instant: UtcInstant,
    evaluation: Evaluation,
    raw_relative: f64,
    unwrapped_relative: f64,
}

#[derive(Clone)]
struct RefinedPoint {
    point: ContinuousPoint,
    bracket_seconds: f64,
    iterations: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BoundaryKind {
    Enter,
    Exit,
    Tangent,
}

#[derive(Clone)]
struct BoundaryEvent {
    sample: AspectTimelineSample,
    kind: BoundaryKind,
}

/// Calculate regular samples, exact passes, and inclusive orb windows.
pub fn calculate_aspect_timeline<P: EventPositionProvider>(
    provider: &P,
    request: AspectTimelineRequest,
) -> Result<AspectTimelineArtifact, TimelineError> {
    let request = AspectTimelineRequest::new(
        request.subject,
        request.aspect,
        request.start,
        request.end,
        request.cadence_seconds,
    )?;
    let output_instants = output_times(&request)?;
    let scan_instants = scan_times(&request)?;
    let mut sampler = TimelineSampler::new(provider, &request)?;

    let mut samples = Vec::with_capacity(output_instants.len());
    for instant in output_instants {
        let evaluation = sampler.evaluate(instant)?;
        samples.push(sample_from_evaluation(instant, &evaluation, request.aspect));
    }

    let mut scan_points = Vec::with_capacity(scan_instants.len());
    let mut previous_raw = None;
    let mut previous_unwrapped = 0.0;
    for instant in scan_instants {
        let evaluation = sampler.evaluate(instant)?;
        let raw = raw_relative(&evaluation);
        let unwrapped = if let Some(previous_raw) = previous_raw {
            previous_unwrapped + signed_delta(previous_raw, raw)
        } else {
            raw
        };
        scan_points.push(ContinuousPoint {
            instant,
            evaluation,
            raw_relative: raw,
            unwrapped_relative: unwrapped,
        });
        previous_raw = Some(raw);
        previous_unwrapped = unwrapped;
    }

    let continuous = add_stationary_points(&mut sampler, &scan_points)?;
    let mut exact_passes = find_exact_passes(&mut sampler, &continuous, request.aspect)?;
    deduplicate_passes(&mut exact_passes);
    let boundary_events = find_orb_boundaries(&mut sampler, &continuous, request.aspect)?;
    let windows = build_windows(&continuous, boundary_events, request.aspect)?;

    let provider_provenance = sampler
        .expected_provenance
        .clone()
        .ok_or(TimelineError::ProviderSampleMismatch)?;
    let scan_interval_count = scan_points.len().saturating_sub(1);
    let solver = TimelineSolverMetadata {
        algorithm: ALGORITHM.into(),
        scan_step_seconds: request.cadence_seconds.min(MAX_SCAN_STEP_SECONDS),
        time_tolerance_seconds: TIME_TOLERANCE_SECONDS,
        angular_tolerance_degrees: ANGULAR_TOLERANCE_DEGREES,
        max_iterations: MAX_REFINEMENT_ITERATIONS,
        output_sample_count: samples
            .len()
            .try_into()
            .map_err(|_| TimelineError::TooManyOutputSamples)?,
        scan_interval_count: scan_interval_count
            .try_into()
            .map_err(|_| TimelineError::TooManyScanIntervals)?,
    };
    AspectTimelineArtifact::build(
        request,
        provider_provenance,
        samples,
        exact_passes,
        windows,
        solver,
    )
}

fn add_stationary_points<P: EventPositionProvider>(
    sampler: &mut TimelineSampler<'_, P>,
    scan_points: &[ContinuousPoint],
) -> Result<Vec<ContinuousPoint>, TimelineError> {
    if scan_points.len() < 2 {
        return Ok(scan_points.to_vec());
    }
    let mut points = vec![scan_points[0].clone()];
    for pair in scan_points.windows(2) {
        let left_speed = pair[0]
            .evaluation
            .measurement
            .relative_speed_degrees_per_day();
        let right_speed = pair[1]
            .evaluation
            .measurement
            .relative_speed_degrees_per_day();
        if left_speed * right_speed < 0.0 {
            let stationary = refine_stationary(sampler, &pair[0], &pair[1])?;
            if stationary.point.instant > pair[0].instant
                && stationary.point.instant < pair[1].instant
            {
                points.push(stationary.point);
            }
        }
        points.push(pair[1].clone());
    }
    Ok(points)
}

fn find_exact_passes<P: EventPositionProvider>(
    sampler: &mut TimelineSampler<'_, P>,
    points: &[ContinuousPoint],
    aspect: AspectDefinition,
) -> Result<Vec<ExactAspectPass>, TimelineError> {
    let mut passes = Vec::new();
    for point in points {
        if point.evaluation.measurement.angular_error_degrees() <= ANGULAR_TOLERANCE_DEGREES {
            passes.push(pass_from_point(
                point.clone(),
                nearest_target(point.unwrapped_relative, aspect.kind()),
                0.0,
                0,
                aspect,
            ));
        }
    }
    for pair in points.windows(2) {
        for target in targets_between(
            pair[0].unwrapped_relative,
            pair[1].unwrapped_relative,
            aspect.kind(),
        ) {
            let left_error = pair[0].unwrapped_relative - target;
            let right_error = pair[1].unwrapped_relative - target;
            if left_error.abs() <= ANGULAR_TOLERANCE_DEGREES
                || right_error.abs() <= ANGULAR_TOLERANCE_DEGREES
                || left_error * right_error >= 0.0
            {
                continue;
            }
            let refined = refine_value(
                sampler,
                &pair[0],
                &pair[1],
                |point| point.unwrapped_relative - target,
                ANGULAR_TOLERANCE_DEGREES,
                false,
            )?;
            passes.push(pass_from_point(
                refined.point,
                target,
                refined.bracket_seconds,
                refined.iterations,
                aspect,
            ));
        }
    }
    Ok(passes)
}

fn find_orb_boundaries<P: EventPositionProvider>(
    sampler: &mut TimelineSampler<'_, P>,
    points: &[ContinuousPoint],
    aspect: AspectDefinition,
) -> Result<Vec<BoundaryEvent>, TimelineError> {
    let mut events = Vec::new();
    let orb_error = |point: &ContinuousPoint| {
        point.evaluation.measurement.angular_error_degrees() - aspect.orb_degrees()
    };
    for pair in points.windows(2) {
        let left = orb_error(&pair[0]);
        let right = orb_error(&pair[1]);
        if left.abs() > ANGULAR_TOLERANCE_DEGREES
            && right.abs() > ANGULAR_TOLERANCE_DEGREES
            && left * right < 0.0
        {
            let refined = refine_value(
                sampler,
                &pair[0],
                &pair[1],
                |point| point.evaluation.measurement.angular_error_degrees() - aspect.orb_degrees(),
                ANGULAR_TOLERANCE_DEGREES,
                true,
            )?;
            events.push(BoundaryEvent {
                sample: sample_from_evaluation(
                    refined.point.instant,
                    &refined.point.evaluation,
                    aspect,
                ),
                kind: if left > 0.0 {
                    BoundaryKind::Enter
                } else {
                    BoundaryKind::Exit
                },
            });
        }
    }
    for (index, point) in points.iter().enumerate() {
        if orb_error(point).abs() > ANGULAR_TOLERANCE_DEGREES {
            continue;
        }
        let before = index.checked_sub(1).map(|value| orb_error(&points[value]));
        let after = points.get(index + 1).map(orb_error);
        let kind = match (before.map(is_inside), after.map(is_inside)) {
            (None, Some(true)) | (Some(false), Some(true)) => Some(BoundaryKind::Enter),
            (Some(true), None) | (Some(true), Some(false)) => Some(BoundaryKind::Exit),
            (None, Some(false)) | (Some(false), None) | (Some(false), Some(false)) => {
                Some(BoundaryKind::Tangent)
            }
            (None, None) => Some(BoundaryKind::Tangent),
            _ => None,
        };
        if let Some(kind) = kind {
            events.push(BoundaryEvent {
                sample: sample_from_evaluation(point.instant, &point.evaluation, aspect),
                kind,
            });
        }
    }
    events.sort_by_key(|event| event.sample.instant);
    events.dedup_by(|right, left| {
        if seconds_between(left.sample.instant, right.sample.instant) <= TIME_TOLERANCE_SECONDS {
            if left.kind == BoundaryKind::Tangent && right.kind != BoundaryKind::Tangent {
                *left = right.clone();
            }
            true
        } else {
            false
        }
    });
    Ok(events)
}

fn build_windows(
    points: &[ContinuousPoint],
    events: Vec<BoundaryEvent>,
    aspect: AspectDefinition,
) -> Result<Vec<AspectWindow>, TimelineError> {
    let first = points.first().ok_or(TimelineError::InvalidArtifact)?;
    let last = points.last().ok_or(TimelineError::InvalidArtifact)?;
    let first_sample = sample_from_evaluation(first.instant, &first.evaluation, aspect);
    let last_sample = sample_from_evaluation(last.instant, &last.evaluation, aspect);
    let first_error = first.evaluation.measurement.angular_error_degrees() - aspect.orb_degrees();
    let mut current = if first_error < -ANGULAR_TOLERANCE_DEGREES {
        Some((first_sample.clone(), true))
    } else {
        None
    };
    let mut windows = Vec::new();
    for event in events {
        match event.kind {
            BoundaryKind::Enter => {
                if current.is_none() {
                    current = Some((event.sample, false));
                }
            }
            BoundaryKind::Exit => {
                if let Some((start, start_truncated)) = current.take() {
                    windows.push(AspectWindow {
                        start,
                        end: event.sample,
                        start_truncated,
                        end_truncated: false,
                    });
                } else {
                    windows.push(AspectWindow {
                        start: event.sample.clone(),
                        end: event.sample,
                        start_truncated: false,
                        end_truncated: false,
                    });
                }
            }
            BoundaryKind::Tangent => {
                if current.is_none() {
                    windows.push(AspectWindow {
                        start: event.sample.clone(),
                        end: event.sample,
                        start_truncated: false,
                        end_truncated: false,
                    });
                }
            }
        }
    }
    if let Some((start, start_truncated)) = current {
        let at_boundary = (last_sample.angular_error_degrees - aspect.orb_degrees()).abs()
            <= ANGULAR_TOLERANCE_DEGREES;
        windows.push(AspectWindow {
            start,
            end: last_sample,
            start_truncated,
            end_truncated: !at_boundary,
        });
    }
    windows.sort_by_key(|window| window.start.instant);
    Ok(windows)
}

fn refine_stationary<P: EventPositionProvider>(
    sampler: &mut TimelineSampler<'_, P>,
    left: &ContinuousPoint,
    right: &ContinuousPoint,
) -> Result<RefinedPoint, TimelineError> {
    refine_value(
        sampler,
        left,
        right,
        |point| {
            point
                .evaluation
                .measurement
                .relative_speed_degrees_per_day()
        },
        1e-8,
        false,
    )
}

fn refine_value<P, F>(
    sampler: &mut TimelineSampler<'_, P>,
    left: &ContinuousPoint,
    right: &ContinuousPoint,
    value: F,
    value_tolerance: f64,
    prefer_nonpositive: bool,
) -> Result<RefinedPoint, TimelineError>
where
    P: EventPositionProvider,
    F: Fn(&ContinuousPoint) -> f64,
{
    let mut low = left.clone();
    let mut high = right.clone();
    let mut low_value = value(&low);
    let high_value = value(&high);
    let mut best = if low_value.abs() <= high_value.abs() {
        low.clone()
    } else {
        high.clone()
    };
    let mut best_value = value(&best).abs();
    let mut best_nonpositive = [left, right]
        .into_iter()
        .filter_map(|point| {
            let point_value = value(point);
            (point_value <= 0.0).then_some((point.clone(), point_value.abs()))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1));
    let mut iterations = 0;
    while iterations < MAX_REFINEMENT_ITERATIONS {
        let width = seconds_between(low.instant, high.instant);
        let selected_value = if prefer_nonpositive {
            best_nonpositive
                .as_ref()
                .map_or(f64::INFINITY, |candidate| candidate.1)
        } else {
            best_value
        };
        if width <= TIME_TOLERANCE_SECONDS && selected_value <= value_tolerance {
            return Ok(RefinedPoint {
                point: if prefer_nonpositive {
                    best_nonpositive
                        .expect("a bracketed crossing has a nonpositive endpoint")
                        .0
                } else {
                    best
                },
                bracket_seconds: width,
                iterations,
            });
        }
        let midpoint = midpoint(low.instant, high.instant)?;
        if midpoint == low.instant || midpoint == high.instant {
            break;
        }
        let evaluation = sampler.evaluate(midpoint)?;
        let point = continuous_from_left(&low, midpoint, evaluation);
        let point_value = value(&point);
        if point_value.abs() < best_value {
            best_value = point_value.abs();
            best = point.clone();
        }
        if point_value <= 0.0
            && best_nonpositive
                .as_ref()
                .is_none_or(|candidate| point_value.abs() < candidate.1)
        {
            best_nonpositive = Some((point.clone(), point_value.abs()));
        }
        if low_value * point_value <= 0.0 {
            high = point;
        } else {
            low = point;
            low_value = point_value;
        }
        iterations += 1;
    }
    let width = seconds_between(low.instant, high.instant);
    let selected_value = if prefer_nonpositive {
        best_nonpositive
            .as_ref()
            .map_or(f64::INFINITY, |candidate| candidate.1)
    } else {
        best_value
    };
    if width <= TIME_TOLERANCE_SECONDS && selected_value <= value_tolerance {
        Ok(RefinedPoint {
            point: if prefer_nonpositive {
                best_nonpositive
                    .expect("a bracketed crossing has a nonpositive endpoint")
                    .0
            } else {
                best
            },
            bracket_seconds: width,
            iterations,
        })
    } else {
        Err(TimelineError::SolverDidNotConverge)
    }
}

fn pass_from_point(
    point: ContinuousPoint,
    target: f64,
    bracket_seconds: f64,
    iterations: u32,
    aspect: AspectDefinition,
) -> ExactAspectPass {
    ExactAspectPass {
        sample: sample_from_evaluation(point.instant, &point.evaluation, aspect),
        target_relative_longitude_degrees: target,
        bracket_seconds,
        iterations,
    }
}

fn deduplicate_passes(passes: &mut Vec<ExactAspectPass>) {
    passes.sort_by(|left, right| {
        left.sample
            .instant
            .cmp(&right.sample.instant)
            .then_with(|| {
                left.target_relative_longitude_degrees
                    .total_cmp(&right.target_relative_longitude_degrees)
            })
    });
    passes.dedup_by(|right, left| {
        if seconds_between(left.sample.instant, right.sample.instant) <= TIME_TOLERANCE_SECONDS {
            if right
                .sample
                .angular_error_degrees
                .total_cmp(&left.sample.angular_error_degrees)
                .is_lt()
            {
                *left = right.clone();
            }
            true
        } else {
            false
        }
    });
}

fn sample_from_evaluation(
    instant: UtcInstant,
    evaluation: &Evaluation,
    aspect: AspectDefinition,
) -> AspectTimelineSample {
    AspectTimelineSample::from_positions(instant, evaluation.first, evaluation.second, aspect)
}

fn continuous_from_left(
    left: &ContinuousPoint,
    instant: UtcInstant,
    evaluation: Evaluation,
) -> ContinuousPoint {
    let raw = raw_relative(&evaluation);
    ContinuousPoint {
        instant,
        unwrapped_relative: left.unwrapped_relative + signed_delta(left.raw_relative, raw),
        raw_relative: raw,
        evaluation,
    }
}

fn raw_relative(evaluation: &Evaluation) -> f64 {
    (evaluation.second.longitude_degrees() - evaluation.first.longitude_degrees()).rem_euclid(360.0)
}

fn signed_delta(first: f64, second: f64) -> f64 {
    let delta = (second - first).rem_euclid(360.0);
    if delta > 180.0 { delta - 360.0 } else { delta }
}

fn targets_between(first: f64, second: f64, kind: AspectKind) -> Vec<f64> {
    let minimum = first.min(second) - ANGULAR_TOLERANCE_DEGREES;
    let maximum = first.max(second) + ANGULAR_TOLERANCE_DEGREES;
    let angle = kind.angle_degrees();
    let bases: &[f64] = if angle == 0.0 {
        &[0.0]
    } else if angle == 180.0 {
        &[180.0]
    } else {
        &[angle, -angle]
    };
    let mut targets = Vec::new();
    for base in bases {
        let first_cycle = ((minimum - base) / 360.0).floor() as i64 - 1;
        let last_cycle = ((maximum - base) / 360.0).ceil() as i64 + 1;
        for cycle in first_cycle..=last_cycle {
            let target = base + cycle as f64 * 360.0;
            if (minimum..=maximum).contains(&target) {
                targets.push(target);
            }
        }
    }
    targets.sort_by(f64::total_cmp);
    targets.dedup_by(|left, right| same(*left, *right));
    targets
}

fn nearest_target(value: f64, kind: AspectKind) -> f64 {
    let mut candidates = targets_between(value - 180.0, value + 180.0, kind);
    candidates.sort_by(|left, right| {
        (value - *left)
            .abs()
            .total_cmp(&(value - *right).abs())
            .then_with(|| left.total_cmp(right))
    });
    candidates[0]
}

fn output_times(request: &AspectTimelineRequest) -> Result<Vec<UtcInstant>, TimelineError> {
    generate_times(
        request.start,
        request.end,
        request.cadence_seconds,
        MAX_OUTPUT_SAMPLES,
        TimelineError::TooManyOutputSamples,
    )
}

fn scan_times(request: &AspectTimelineRequest) -> Result<Vec<UtcInstant>, TimelineError> {
    generate_times(
        request.start,
        request.end,
        request.cadence_seconds.min(MAX_SCAN_STEP_SECONDS),
        MAX_SCAN_INTERVALS + 1,
        TimelineError::TooManyScanIntervals,
    )
}

fn generate_times(
    start: UtcInstant,
    end: UtcInstant,
    step_seconds: u64,
    maximum_points: usize,
    limit_error: TimelineError,
) -> Result<Vec<UtcInstant>, TimelineError> {
    let step: i64 = step_seconds
        .try_into()
        .map_err(|_| TimelineError::TimeRange)?;
    let mut values = vec![start];
    let mut current = start;
    while current < end {
        let next_datetime = current
            .as_datetime()
            .checked_add_signed(Duration::seconds(step))
            .ok_or(TimelineError::TimeRange)?;
        let next = UtcInstant::from_datetime(next_datetime.min(end.as_datetime()));
        if next == current {
            return Err(TimelineError::TimeRange);
        }
        values.push(next);
        if values.len() > maximum_points {
            return Err(limit_error);
        }
        current = next;
    }
    Ok(values)
}

fn midpoint(first: UtcInstant, second: UtcInstant) -> Result<UtcInstant, TimelineError> {
    let duration = second.as_datetime() - first.as_datetime();
    let nanoseconds = duration.num_nanoseconds().ok_or(TimelineError::TimeRange)?;
    let value = first
        .as_datetime()
        .checked_add_signed(Duration::nanoseconds(nanoseconds / 2))
        .ok_or(TimelineError::TimeRange)?;
    Ok(UtcInstant::from_datetime(value))
}

fn seconds_between(first: UtcInstant, second: UtcInstant) -> f64 {
    let duration = second.as_datetime() - first.as_datetime();
    duration
        .num_nanoseconds()
        .map_or_else(
            || duration.num_milliseconds() as f64 / 1_000.0,
            |value| value as f64 / 1_000_000_000.0,
        )
        .abs()
}

fn validate_frame(frame: EventCoordinateFrame) -> Result<(), TimelineError> {
    if let EventCoordinateFrame::Configured { zodiac, ayanamsa } = frame {
        EventCoordinateFrame::configured(zodiac, ayanamsa)
            .map_err(|_| TimelineError::InvalidCoordinateFrame)?;
    }
    Ok(())
}

fn is_inside(value: f64) -> bool {
    value < -ANGULAR_TOLERANCE_DEGREES
}

fn same(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= DERIVED_TOLERANCE
}

fn positions_match(left: AngularPosition, right: AngularPosition) -> bool {
    same(left.longitude_degrees(), right.longitude_degrees())
        && same(
            left.longitude_speed_degrees_per_day(),
            right.longitude_speed_degrees_per_day(),
        )
}

fn samples_match(left: &AspectTimelineSample, right: &AspectTimelineSample) -> bool {
    left.instant == right.instant
        && positions_match(left.first_position, right.first_position)
        && positions_match(left.second_position, right.second_position)
        && same(
            left.signed_separation_degrees,
            right.signed_separation_degrees,
        )
        && same(left.separation_degrees, right.separation_degrees)
        && same(
            left.signed_aspect_error_degrees,
            right.signed_aspect_error_degrees,
        )
        && same(left.angular_error_degrees, right.angular_error_degrees)
        && same(
            left.relative_speed_degrees_per_day,
            right.relative_speed_degrees_per_day,
        )
        && left.phase == right.phase
        && left.within_orb == right.within_orb
        && same(left.proximity, right.proximity)
}

impl AspectTimelineArtifact {
    fn build(
        request: AspectTimelineRequest,
        provider_provenance: CalculationProvenance,
        samples: Vec<AspectTimelineSample>,
        exact_passes: Vec<ExactAspectPass>,
        windows: Vec<AspectWindow>,
        solver: TimelineSolverMetadata,
    ) -> Result<Self, TimelineError> {
        let expected_times = output_times(&request)?;
        if samples.len() != expected_times.len()
            || samples
                .iter()
                .zip(expected_times)
                .any(|(sample, instant)| sample.instant != instant)
        {
            return Err(TimelineError::InvalidArtifact);
        }
        let fixed = request.subject.fixed_position()?;
        for sample in &samples {
            validate_sample(sample, request.aspect, fixed)?;
        }
        let mut previous_pass = None;
        for pass in &exact_passes {
            validate_sample(&pass.sample, request.aspect, fixed)?;
            if pass.sample.instant < request.start
                || pass.sample.instant > request.end
                || pass.sample.angular_error_degrees > ANGULAR_TOLERANCE_DEGREES
                || !pass.target_relative_longitude_degrees.is_finite()
                || !valid_target(
                    pass.target_relative_longitude_degrees,
                    request.aspect.kind(),
                )
                || !pass.bracket_seconds.is_finite()
                || !(0.0..=TIME_TOLERANCE_SECONDS).contains(&pass.bracket_seconds)
                || pass.iterations > MAX_REFINEMENT_ITERATIONS
            {
                return Err(TimelineError::InvalidArtifact);
            }
            let target_modulo = pass.target_relative_longitude_degrees.rem_euclid(360.0);
            let raw = (pass.sample.second_position.longitude_degrees()
                - pass.sample.first_position.longitude_degrees())
            .rem_euclid(360.0);
            if signed_delta(target_modulo, raw).abs() > ANGULAR_TOLERANCE_DEGREES {
                return Err(TimelineError::DerivedValueMismatch);
            }
            if let Some(previous) = previous_pass
                && (pass.sample.instant < previous
                    || seconds_between(previous, pass.sample.instant) <= TIME_TOLERANCE_SECONDS)
            {
                return Err(TimelineError::InvalidArtifact);
            }
            previous_pass = Some(pass.sample.instant);
        }
        let mut previous_end = None;
        for window in &windows {
            validate_sample(&window.start, request.aspect, fixed)?;
            validate_sample(&window.end, request.aspect, fixed)?;
            if window.start.instant < request.start
                || window.end.instant > request.end
                || window.end.instant < window.start.instant
                || !window.start.within_orb
                || !window.end.within_orb
                || (!window.start_truncated
                    && (window.start.angular_error_degrees - request.aspect.orb_degrees()).abs()
                        > ANGULAR_TOLERANCE_DEGREES)
                || (!window.end_truncated
                    && (window.end.angular_error_degrees - request.aspect.orb_degrees()).abs()
                        > ANGULAR_TOLERANCE_DEGREES)
            {
                return Err(TimelineError::InvalidArtifact);
            }
            if let Some(end) = previous_end
                && window.start.instant < end
            {
                return Err(TimelineError::InvalidArtifact);
            }
            previous_end = Some(window.end.instant);
        }
        let scan_count = scan_times(&request)?.len().saturating_sub(1);
        if solver.algorithm != ALGORITHM
            || solver.scan_step_seconds != request.cadence_seconds.min(MAX_SCAN_STEP_SECONDS)
            || !same(solver.time_tolerance_seconds, TIME_TOLERANCE_SECONDS)
            || !same(solver.angular_tolerance_degrees, ANGULAR_TOLERANCE_DEGREES)
            || solver.max_iterations != MAX_REFINEMENT_ITERATIONS
            || solver.output_sample_count as usize != samples.len()
            || solver.scan_interval_count as usize != scan_count
        {
            return Err(TimelineError::InvalidArtifact);
        }
        Ok(Self {
            request,
            provider_provenance,
            samples,
            exact_passes,
            windows,
            solver,
        })
    }

    fn from_wire(wire: ArtifactWire) -> Result<Self, TimelineError> {
        if wire.schema_version != SCHEMA_VERSION {
            return Err(TimelineError::UnsupportedSchema(wire.schema_version));
        }
        let aspect = wire.request.aspect;
        let samples = wire
            .samples
            .into_iter()
            .map(|sample| AspectTimelineSample::from_wire(sample, aspect))
            .collect::<Result<Vec<_>, _>>()?;
        let exact_passes = wire
            .exact_passes
            .into_iter()
            .map(|pass| {
                Ok(ExactAspectPass {
                    sample: AspectTimelineSample::from_wire(pass.sample, aspect)?,
                    target_relative_longitude_degrees: pass.target_relative_longitude_degrees,
                    bracket_seconds: pass.bracket_seconds,
                    iterations: pass.iterations,
                })
            })
            .collect::<Result<Vec<_>, TimelineError>>()?;
        let windows = wire
            .windows
            .into_iter()
            .map(|window| {
                Ok(AspectWindow {
                    start: AspectTimelineSample::from_wire(window.start, aspect)?,
                    end: AspectTimelineSample::from_wire(window.end, aspect)?,
                    start_truncated: window.start_truncated,
                    end_truncated: window.end_truncated,
                })
            })
            .collect::<Result<Vec<_>, TimelineError>>()?;
        Self::build(
            wire.request,
            wire.provider_provenance,
            samples,
            exact_passes,
            windows,
            wire.solver,
        )
    }

    pub fn request(&self) -> &AspectTimelineRequest {
        &self.request
    }
    pub fn provider_provenance(&self) -> &CalculationProvenance {
        &self.provider_provenance
    }
    pub fn samples(&self) -> &[AspectTimelineSample] {
        &self.samples
    }
    pub fn exact_passes(&self) -> &[ExactAspectPass] {
        &self.exact_passes
    }
    pub fn windows(&self) -> &[AspectWindow] {
        &self.windows
    }
    pub fn solver(&self) -> &TimelineSolverMetadata {
        &self.solver
    }
    pub fn to_json(&self) -> Result<String, TimelineError> {
        Ok(serde_json::to_string(self)?)
    }
    pub fn to_pretty_json(&self) -> Result<String, TimelineError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
    pub fn from_json(input: &str) -> Result<Self, TimelineError> {
        Ok(serde_json::from_str(input)?)
    }
    pub fn content_sha256(&self) -> Result<String, TimelineError> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
    pub fn content_id(&self) -> Result<String, TimelineError> {
        Ok(format!("sha256:{}", self.content_sha256()?))
    }
}

fn validate_sample(
    sample: &AspectTimelineSample,
    aspect: AspectDefinition,
    fixed: Option<AngularPosition>,
) -> Result<(), TimelineError> {
    let expected = AspectTimelineSample::from_positions(
        sample.instant,
        sample.first_position,
        sample.second_position,
        aspect,
    );
    if !samples_match(sample, &expected)
        || fixed.is_some_and(|position| !positions_match(position, sample.first_position))
    {
        return Err(TimelineError::DerivedValueMismatch);
    }
    Ok(())
}

fn valid_target(target: f64, kind: AspectKind) -> bool {
    let modulo = target.rem_euclid(360.0);
    let angle = kind.angle_degrees();
    signed_delta(modulo, angle).abs() <= DERIVED_TOLERANCE
        || (angle != 0.0
            && angle != 180.0
            && signed_delta(modulo, (-angle).rem_euclid(360.0)).abs() <= DERIVED_TOLERANCE)
}

impl Serialize for AspectTimelineArtifact {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ArtifactRef {
            schema_version: SCHEMA_VERSION,
            request: &self.request,
            provider_provenance: &self.provider_provenance,
            samples: &self.samples,
            exact_passes: &self.exact_passes,
            windows: &self.windows,
            solver: &self.solver,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AspectTimelineArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_wire(ArtifactWire::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
