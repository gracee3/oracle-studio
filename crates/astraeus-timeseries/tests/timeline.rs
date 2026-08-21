use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinition, AspectKind, CalculationProvenance, CalculationRequest,
    CelestialObject, ChartAngles, ChartPointId, DeterministicMock, EphemerisAdapter,
    EphemerisSource, GeographicLocation, HouseCusps, HouseSystem, Position, UtcInstant, Zodiac,
};
use astraeus_events::{
    EventCoordinateFrame, EventError, EventPositionProvider, EventPositionRequest,
    EventPositionSample,
};
use astraeus_timeseries::{
    ANGULAR_TOLERANCE_DEGREES, AspectTimelineArtifact, AspectTimelineRequest,
    AspectTimelineSubject, MAX_SCAN_STEP_SECONDS, TimelineError, calculate_aspect_timeline,
};
use chrono::Duration;

const DAY: u64 = 86_400;

#[derive(Clone, Copy)]
enum Curve {
    Linear { base: f64, speed: f64 },
    Quadratic { target: f64, center: f64 },
    OrbTangent { target: f64, orb: f64, center: f64 },
    Cubic { target: f64 },
}

impl Curve {
    fn position(self, days: f64) -> (f64, f64) {
        match self {
            Self::Linear { base, speed } => (base + speed * days, speed),
            Self::Quadratic { target, center } => {
                let offset = days - center;
                (target + offset * offset, 2.0 * offset)
            }
            Self::OrbTangent {
                target,
                orb,
                center,
            } => {
                let offset = days - center;
                (target + orb + offset * offset, 2.0 * offset)
            }
            Self::Cubic { target } => {
                let value = (days - 1.0) * (days - 2.0) * (days - 3.0);
                let speed = 3.0 * days * days - 12.0 * days + 11.0;
                (target + value, speed)
            }
        }
    }
}

struct SyntheticProvider {
    epoch: UtcInstant,
    curve: Curve,
    change_provenance: bool,
}

impl SyntheticProvider {
    fn new(epoch: UtcInstant, curve: Curve) -> Self {
        Self {
            epoch,
            curve,
            change_provenance: false,
        }
    }
}

impl EventPositionProvider for SyntheticProvider {
    fn sample_event_positions(
        &self,
        request: &EventPositionRequest,
    ) -> Result<EventPositionSample, EventError> {
        let duration = request.instant().as_datetime() - self.epoch.as_datetime();
        let seconds = duration.num_nanoseconds().unwrap() as f64 / 1_000_000_000.0;
        let days = seconds / DAY as f64;
        let mut positions = BTreeMap::new();
        for object in request.objects() {
            let (longitude, speed) = if *object == CelestialObject::Sun {
                (0.0, 0.0)
            } else {
                self.curve.position(days)
            };
            positions.insert(
                *object,
                AngularPosition::new(longitude.rem_euclid(360.0), speed).unwrap(),
            );
        }
        let version = if self.change_provenance && seconds > 0.0 {
            "changed"
        } else {
            "1"
        };
        EventPositionSample::new(
            request.clone(),
            positions,
            CalculationProvenance::new(
                "synthetic timeline",
                version,
                EphemerisSource::Synthetic,
                None,
            )
            .unwrap(),
        )
    }
}

fn instant(value: &str) -> UtcInstant {
    UtcInstant::parse_rfc3339(value).unwrap()
}

fn after(start: UtcInstant, days: i64) -> UtcInstant {
    UtcInstant::from_datetime(start.as_datetime() + Duration::days(days))
}

fn moving_request(
    start: UtcInstant,
    days: i64,
    kind: AspectKind,
    orb: f64,
    cadence: u64,
) -> AspectTimelineRequest {
    AspectTimelineRequest::new(
        AspectTimelineSubject::moving_moving(
            CelestialObject::Sun,
            CelestialObject::Moon,
            EventCoordinateFrame::TropicalOfDate,
        )
        .unwrap(),
        AspectDefinition::new(kind, orb).unwrap(),
        start,
        after(start, days),
        cadence,
    )
    .unwrap()
}

fn fixed_chart(longitude: f64) -> CalculationArtifact {
    let chart_instant = instant("2000-01-01T12:00:00Z");
    let request = CalculationRequest::new(
        chart_instant,
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        HouseSystem::WholeSign,
    )
    .unwrap();
    let result = DeterministicMock::new(
        BTreeMap::from([(
            CelestialObject::Sun,
            Position::new(longitude, 0.0, 1.0, 1.0).unwrap(),
        )]),
        HouseCusps::new(
            (0..12).map(|value| f64::from(value) * 30.0).collect(),
            ChartAngles::new(
                AngularPosition::new(0.0, 360.0).unwrap(),
                AngularPosition::new(270.0, 360.0).unwrap(),
                AngularPosition::new(180.0, 360.0).unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .calculate(&request)
    .unwrap();
    CalculationArtifact::new(request, result).unwrap()
}

#[test]
fn both_subject_modes_produce_waveform_samples() {
    let start = instant("2026-01-01T00:00:00Z");
    let provider = SyntheticProvider::new(
        start,
        Curve::Linear {
            base: 88.0,
            speed: 1.0,
        },
    );
    let moving = calculate_aspect_timeline(
        &provider,
        moving_request(start, 4, AspectKind::Square, 2.0, DAY),
    )
    .unwrap();
    assert_eq!(moving.samples().len(), 5);
    assert_eq!(moving.exact_passes().len(), 1);
    assert_eq!(moving.windows().len(), 1);
    assert_eq!(moving.windows()[0].start().instant(), after(start, 0));
    assert_eq!(moving.windows()[0].end().instant(), after(start, 4));
    assert!(!moving.windows()[0].start_truncated());
    assert!(!moving.windows()[0].end_truncated());

    let fixed_subject = AspectTimelineSubject::moving_fixed(
        fixed_chart(0.0),
        ChartPointId::Sun,
        CelestialObject::Moon,
    )
    .unwrap();
    let fixed = calculate_aspect_timeline(
        &provider,
        AspectTimelineRequest::new(
            fixed_subject,
            AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
            start,
            after(start, 4),
            DAY,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fixed.exact_passes().len(), 1);
    assert!(
        fixed
            .samples()
            .iter()
            .all(|sample| sample.first_position().longitude_speed_degrees_per_day() == 0.0)
    );
}

#[test]
fn all_five_aspects_refine_exact_passes_in_order() {
    let start = instant("2026-01-01T00:00:00Z");
    for kind in [
        AspectKind::Conjunction,
        AspectKind::Sextile,
        AspectKind::Square,
        AspectKind::Trine,
        AspectKind::Opposition,
    ] {
        let provider = SyntheticProvider::new(
            start,
            Curve::Linear {
                base: kind.angle_degrees() - 1.3,
                speed: 1.0,
            },
        );
        let artifact =
            calculate_aspect_timeline(&provider, moving_request(start, 3, kind, 1.0, DAY)).unwrap();
        assert_eq!(artifact.exact_passes().len(), 1, "{kind:?}");
        let pass = &artifact.exact_passes()[0];
        assert!(pass.bracket_seconds() <= 1.0);
        assert!(pass.sample().angular_error_degrees() <= ANGULAR_TOLERANCE_DEGREES);
    }
}

#[test]
fn wraparound_endpoint_roots_and_inclusive_boundaries_are_deterministic() {
    let start = instant("2026-01-01T00:00:00Z");
    let wrapping = calculate_aspect_timeline(
        &SyntheticProvider::new(
            start,
            Curve::Linear {
                base: 359.0,
                speed: 1.0,
            },
        ),
        moving_request(start, 2, AspectKind::Conjunction, 1.0, DAY),
    )
    .unwrap();
    assert_eq!(
        wrapping.exact_passes()[0].sample().instant(),
        after(start, 1)
    );
    assert!(wrapping.samples()[0].within_orb());
    assert_eq!(wrapping.samples()[0].proximity(), 0.0);

    let endpoint = calculate_aspect_timeline(
        &SyntheticProvider::new(
            start,
            Curve::Linear {
                base: 90.0,
                speed: 1.0,
            },
        ),
        moving_request(start, 1, AspectKind::Square, 1.0, DAY),
    )
    .unwrap();
    assert_eq!(endpoint.exact_passes().len(), 1);
    assert_eq!(endpoint.exact_passes()[0].sample().instant(), start);

    let zero_duration = calculate_aspect_timeline(
        &SyntheticProvider::new(
            start,
            Curve::Linear {
                base: 89.0,
                speed: -1.0,
            },
        ),
        moving_request(start, 0, AspectKind::Square, 1.0, DAY),
    )
    .unwrap();
    assert_eq!(zero_duration.samples().len(), 1);
    assert_eq!(zero_duration.windows().len(), 1);
    assert_eq!(
        zero_duration.windows()[0].start().instant(),
        zero_duration.windows()[0].end().instant()
    );
}

#[test]
fn truncated_and_zero_duration_tangent_windows_are_explicit() {
    let start = instant("2026-01-01T00:00:00Z");
    let truncated = calculate_aspect_timeline(
        &SyntheticProvider::new(
            start,
            Curve::Linear {
                base: 89.0,
                speed: 0.0,
            },
        ),
        moving_request(start, 2, AspectKind::Square, 2.0, DAY),
    )
    .unwrap();
    assert_eq!(truncated.windows().len(), 1);
    assert!(truncated.windows()[0].start_truncated());
    assert!(truncated.windows()[0].end_truncated());

    let tangent = calculate_aspect_timeline(
        &SyntheticProvider::new(
            start,
            Curve::OrbTangent {
                target: 90.0,
                orb: 1.0,
                center: 2.1,
            },
        ),
        moving_request(start, 4, AspectKind::Square, 1.0, DAY),
    )
    .unwrap();
    assert_eq!(tangent.windows().len(), 1);
    assert_eq!(
        tangent.windows()[0].start().instant(),
        tangent.windows()[0].end().instant()
    );
    assert!(!tangent.windows()[0].start_truncated());
    assert!(!tangent.windows()[0].end_truncated());
}

#[test]
fn retrograde_triple_passes_and_stationary_exact_tangency_are_found() {
    let start = instant("2026-01-01T00:00:00Z");
    let triple = calculate_aspect_timeline(
        &SyntheticProvider::new(start, Curve::Cubic { target: 90.0 }),
        moving_request(start, 4, AspectKind::Square, 0.5, DAY),
    )
    .unwrap();
    assert_eq!(triple.exact_passes().len(), 3);
    assert!(
        triple
            .exact_passes()
            .windows(2)
            .all(|pair| pair[0].sample().instant() < pair[1].sample().instant())
    );

    let tangent = calculate_aspect_timeline(
        &SyntheticProvider::new(
            start,
            Curve::Quadratic {
                target: 90.0,
                center: 2.1,
            },
        ),
        moving_request(start, 4, AspectKind::Square, 1.0, DAY),
    )
    .unwrap();
    assert_eq!(tangent.exact_passes().len(), 1);
    assert!(
        tangent.exact_passes()[0].sample().angular_error_degrees() <= ANGULAR_TOLERANCE_DEGREES
    );
}

#[test]
fn artifact_round_trip_is_strict_and_content_addressed() {
    let start = instant("2026-01-01T00:00:00Z");
    let request = moving_request(start, 3, AspectKind::Trine, 1.0, DAY);
    let provider = SyntheticProvider::new(
        start,
        Curve::Linear {
            base: 118.7,
            speed: 1.0,
        },
    );
    let first = calculate_aspect_timeline(&provider, request.clone()).unwrap();
    let second = calculate_aspect_timeline(&provider, request).unwrap();
    assert_eq!(first.to_json().unwrap(), second.to_json().unwrap());
    assert_eq!(first.content_id().unwrap(), second.content_id().unwrap());
    let json = first.to_json().unwrap();
    let decoded = AspectTimelineArtifact::from_json(&json).unwrap();
    assert_eq!(decoded.to_json().unwrap(), json);
    assert_eq!(decoded.content_id().unwrap(), first.content_id().unwrap());
    assert!(
        AspectTimelineArtifact::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":2",
            1
        ))
        .is_err()
    );
    assert!(
        AspectTimelineArtifact::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"extra\":true",
            1
        ))
        .is_err()
    );
    let proximity = format!(
        "\"proximity\":{}",
        serde_json::to_string(&first.samples()[0].proximity()).unwrap()
    );
    assert!(
        AspectTimelineArtifact::from_json(&json.replacen(&proximity, "\"proximity\":0.123", 1))
            .is_err()
    );
}

#[test]
fn invalid_requests_caps_and_provider_provenance_are_rejected() {
    let start = instant("2026-01-01T00:00:00Z");
    let subject = AspectTimelineSubject::moving_moving(
        CelestialObject::Sun,
        CelestialObject::Moon,
        EventCoordinateFrame::TropicalOfDate,
    )
    .unwrap();
    assert!(matches!(
        AspectTimelineRequest::new(
            subject.clone(),
            AspectDefinition::new(AspectKind::Square, 1.0).unwrap(),
            after(start, 1),
            start,
            DAY,
        ),
        Err(TimelineError::InvalidRange)
    ));
    assert!(matches!(
        AspectTimelineRequest::new(
            subject.clone(),
            AspectDefinition::new(AspectKind::Square, 1.0).unwrap(),
            start,
            after(start, 1),
            0,
        ),
        Err(TimelineError::InvalidCadence)
    ));
    assert!(matches!(
        AspectTimelineRequest::new(
            subject,
            AspectDefinition::new(AspectKind::Square, 0.0).unwrap(),
            start,
            after(start, 1),
            DAY,
        ),
        Err(TimelineError::NonPositiveOrb)
    ));
    assert!(matches!(
        AspectTimelineSubject::moving_moving(
            CelestialObject::Moon,
            CelestialObject::Moon,
            EventCoordinateFrame::TropicalOfDate,
        ),
        Err(TimelineError::DuplicateMovingObjects)
    ));
    assert!(matches!(
        AspectTimelineSubject::moving_fixed(
            fixed_chart(0.0),
            ChartPointId::Mercury,
            CelestialObject::Moon,
        ),
        Err(TimelineError::FixedPointUnavailable(ChartPointId::Mercury))
    ));

    let too_many_outputs_end =
        UtcInstant::from_datetime(start.as_datetime() + Duration::seconds(100_000));
    assert!(matches!(
        AspectTimelineRequest::new(
            AspectTimelineSubject::moving_moving(
                CelestialObject::Sun,
                CelestialObject::Moon,
                EventCoordinateFrame::TropicalOfDate,
            )
            .unwrap(),
            AspectDefinition::new(AspectKind::Square, 1.0).unwrap(),
            start,
            too_many_outputs_end,
            1,
        ),
        Err(TimelineError::TooManyOutputSamples)
    ));

    let scan_seconds = MAX_SCAN_STEP_SECONDS * 100_001;
    let too_many_scans_end =
        UtcInstant::from_datetime(start.as_datetime() + Duration::seconds(scan_seconds as i64));
    assert!(matches!(
        AspectTimelineRequest::new(
            AspectTimelineSubject::moving_moving(
                CelestialObject::Sun,
                CelestialObject::Moon,
                EventCoordinateFrame::TropicalOfDate,
            )
            .unwrap(),
            AspectDefinition::new(AspectKind::Square, 1.0).unwrap(),
            start,
            too_many_scans_end,
            scan_seconds,
        ),
        Err(TimelineError::TooManyScanIntervals)
    ));

    let inconsistent = SyntheticProvider {
        epoch: start,
        curve: Curve::Linear {
            base: 89.0,
            speed: 1.0,
        },
        change_provenance: true,
    };
    assert!(matches!(
        calculate_aspect_timeline(
            &inconsistent,
            moving_request(start, 1, AspectKind::Square, 1.0, DAY),
        ),
        Err(TimelineError::ProviderProvenanceMismatch)
    ));
}
