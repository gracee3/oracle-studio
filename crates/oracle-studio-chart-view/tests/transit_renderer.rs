use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_comparison::{
    ComparisonArtifact, ComparisonKind, ComparisonMotionPolicy, ComparisonSpecification,
};
use astraeus_core::{
    AngularPosition, AspectDefinition, AspectDefinitions, AspectKind, CalculationOptions,
    CalculationRequest, CelestialObject, ChartAngles, ChartPointId, ChartPointSelection,
    DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps, HouseSystem, Position,
    UtcInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use astraeus_techniques::harmonic;
use chrono::{DateTime, Utc};
use oracle_studio_chart_view::{
    ChartScene, RenderOptions, TransitTimeline, TransitTimelineError, WheelOrientation,
    render_biwheel_svg, resolve_circular_collisions,
};
use sha2::{Digest, Sha256};

fn chart(
    timestamp: &str,
    points: &[(CelestialObject, f64, f64)],
    ascendant: f64,
) -> DerivedChartArtifact {
    let objects: Vec<_> = points.iter().map(|(object, _, _)| *object).collect();
    let options =
        CalculationOptions::new(objects, Zodiac::Tropical, None, HouseSystem::WholeSign).unwrap();
    let request = CalculationRequest::from_options(
        UtcInstant::parse_rfc3339(timestamp).unwrap(),
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
        options.clone(),
    );
    let houses = HouseCusps::new(
        (0..12)
            .map(|index| (ascendant + f64::from(index) * 30.0).rem_euclid(360.0))
            .collect(),
        ChartAngles::new(
            AngularPosition::new(ascendant, 0.0).unwrap(),
            AngularPosition::new((ascendant + 270.0).rem_euclid(360.0), 0.0).unwrap(),
            AngularPosition::new((ascendant + 180.0).rem_euclid(360.0), 0.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let positions = points
        .iter()
        .map(|(object, longitude, speed)| {
            (
                *object,
                Position::new(*longitude, 0.0, 1.0, *speed).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let result = DeterministicMock::new(positions, houses)
        .calculate(&request)
        .unwrap();
    DerivedChartArtifact::new(
        CalculationArtifact::new(request, result).unwrap(),
        ChartSpecification::new(options, AspectDefinitions::new(Vec::new()).unwrap()),
    )
    .unwrap()
}

fn selected(points: &[CelestialObject]) -> ChartPointSelection {
    ChartPointSelection::new(points.iter().copied().map(ChartPointId::from).collect()).unwrap()
}

fn definitions() -> AspectDefinitions {
    AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Conjunction, 3.0).unwrap(),
        AspectDefinition::new(AspectKind::Square, 3.0).unwrap(),
        AspectDefinition::new(AspectKind::Trine, 3.0).unwrap(),
        AspectDefinition::new(AspectKind::Opposition, 3.0).unwrap(),
    ])
    .unwrap()
}

fn comparison(
    natal: DerivedChartArtifact,
    transit: DerivedChartArtifact,
    points: &[CelestialObject],
) -> ComparisonArtifact {
    ComparisonArtifact::new(
        natal,
        transit,
        ComparisonSpecification::moving_second(
            ComparisonKind::TransitToNatal,
            definitions(),
            selected(points),
            selected(points),
        )
        .unwrap(),
    )
    .unwrap()
}

fn natal() -> DerivedChartArtifact {
    chart(
        "2000-01-01T00:00:00Z",
        &[
            (CelestialObject::Sun, 0.0, 0.0),
            (CelestialObject::Moon, 120.0, 0.0),
            (CelestialObject::Mercury, 240.0, 0.0),
        ],
        23.0,
    )
}

fn transit(timestamp: &str, sun: f64, mercury: f64) -> DerivedChartArtifact {
    chart(
        timestamp,
        &[
            (CelestialObject::Sun, sun, 1.0),
            (CelestialObject::Moon, 45.0, 0.0),
            (CelestialObject::Mercury, mercury, -1.0),
        ],
        180.0,
    )
}

fn time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn accepts_only_validated_physical_moving_transit_comparisons() {
    let artifact = comparison(
        natal(),
        transit("2026-01-01T00:00:00Z", 89.0, 241.0),
        &[
            CelestialObject::Sun,
            CelestialObject::Moon,
            CelestialObject::Mercury,
        ],
    );
    let json = artifact.to_json().unwrap();
    let validated = ComparisonArtifact::from_json(&json).unwrap();
    let scene = ChartScene::from_comparison(&validated).unwrap();
    assert_eq!(scene.timestamp, "2026-01-01T00:00:00+00:00");
    assert_eq!(scene.natal.houses.len(), 12);
    assert!(scene.aspects.iter().any(|aspect| aspect.kind == "Square"));
    assert!(
        ComparisonArtifact::from_json(&json.replacen("\"applying\"", "\"separating\"", 1)).is_err()
    );

    let generic = ComparisonArtifact::new(
        natal(),
        transit("2026-01-01T00:00:00Z", 89.0, 241.0),
        ComparisonSpecification::moving_second(
            ComparisonKind::Generic,
            definitions(),
            selected(&[CelestialObject::Sun]),
            selected(&[CelestialObject::Sun]),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        ChartScene::from_comparison(&generic),
        Err(TransitTimelineError::UnsupportedComparisonKind)
    ));

    let static_transit = ComparisonArtifact::new(
        natal(),
        transit("2026-01-01T00:00:00Z", 89.0, 241.0),
        ComparisonSpecification::new(
            ComparisonKind::TransitToNatal,
            definitions(),
            selected(&[CelestialObject::Sun]),
            selected(&[CelestialObject::Sun]),
            ComparisonMotionPolicy::None,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        ChartScene::from_comparison(&static_transit),
        Err(TransitTimelineError::UnsupportedMotionPolicy)
    ));

    let synthetic_natal = harmonic(&natal(), 2).unwrap();
    let non_physical = ComparisonArtifact::new(
        synthetic_natal,
        transit("2026-01-01T00:00:00Z", 89.0, 241.0),
        ComparisonSpecification::moving_second(
            ComparisonKind::TransitToNatal,
            definitions(),
            selected(&[CelestialObject::Sun]),
            selected(&[CelestialObject::Sun]),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        ChartScene::from_comparison(&non_physical),
        Err(TransitTimelineError::NonPhysicalLayer)
    ));
}

#[test]
fn timeline_rejects_natal_population_and_chronology_changes() {
    let points = [
        CelestialObject::Sun,
        CelestialObject::Moon,
        CelestialObject::Mercury,
    ];
    let first = comparison(
        natal(),
        transit("2026-01-01T00:00:00Z", 359.0, 1.0),
        &points,
    );
    let second = comparison(
        natal(),
        transit("2026-01-01T12:00:00Z", 1.0, 359.0),
        &points,
    );
    assert!(TransitTimeline::from_comparisons(&[first.clone(), second.clone()]).is_ok());
    assert!(matches!(
        TransitTimeline::from_comparisons(&[first.clone(), first.clone()]),
        Err(TransitTimelineError::DuplicateTimestamp(_))
    ));
    assert!(matches!(
        TransitTimeline::from_comparisons(&[second.clone(), first.clone()]),
        Err(TransitTimelineError::ReversedChronology { .. })
    ));

    let changed_natal = comparison(
        chart(
            "2000-01-01T00:00:00Z",
            &[
                (CelestialObject::Sun, 1.0, 0.0),
                (CelestialObject::Moon, 120.0, 0.0),
                (CelestialObject::Mercury, 240.0, 0.0),
            ],
            23.0,
        ),
        transit("2026-01-01T12:00:00Z", 1.0, 359.0),
        &points,
    );
    assert!(matches!(
        TransitTimeline::from_comparisons(&[first.clone(), changed_natal]),
        Err(TransitTimelineError::NatalChanged)
    ));

    let smaller_transit = chart(
        "2026-01-01T12:00:00Z",
        &[(CelestialObject::Sun, 1.0, 1.0)],
        180.0,
    );
    let smaller_population = comparison(natal(), smaller_transit, &[CelestialObject::Sun]);
    assert!(matches!(
        TransitTimeline::from_comparisons(&[first, smaller_population]),
        Err(TransitTimelineError::MovingPointPopulationChanged)
    ));
}

#[test]
fn dense_interpolation_respects_wrap_retrograde_stations_and_exact_aspects() {
    let points = [
        CelestialObject::Sun,
        CelestialObject::Moon,
        CelestialObject::Mercury,
    ];
    let first = comparison(
        natal(),
        transit("2026-01-01T00:00:00Z", 359.0, 1.0),
        &points,
    );
    let second = comparison(
        natal(),
        transit("2026-01-01T12:00:00Z", 1.0, 359.0),
        &points,
    );
    let first_aspects = first.aspects().len();
    let second_aspects = second.aspects().len();
    let timeline = TransitTimeline::from_comparisons(&[first, second]).unwrap();
    let middle = timeline.scene_at(time("2026-01-01T06:00:00Z"));
    let sun = middle
        .transit
        .iter()
        .find(|point| point.id == "Sun")
        .unwrap();
    let mercury = middle
        .transit
        .iter()
        .find(|point| point.id == "Mercury")
        .unwrap();
    let moon = middle
        .transit
        .iter()
        .find(|point| point.id == "Moon")
        .unwrap();
    assert!(sun.longitude_degrees < 1.0e-9);
    assert!(mercury.longitude_degrees < 1.0e-9);
    assert_eq!(moon.longitude_degrees, 45.0);
    assert!(mercury.retrograde);
    assert_eq!(middle.aspects.len(), first_aspects);
    assert_eq!(
        timeline
            .scene_at(time("2026-01-01T12:00:00Z"))
            .aspects
            .len(),
        second_aspects
    );
}

#[test]
fn large_gaps_are_exact_jumps() {
    let points = [CelestialObject::Sun];
    let first = comparison(
        natal(),
        transit("2026-01-01T00:00:00Z", 10.0, 250.0),
        &points,
    );
    let second = comparison(
        natal(),
        transit("2026-01-03T00:00:00Z", 20.0, 240.0),
        &points,
    );
    let timeline = TransitTimeline::from_comparisons(&[first, second]).unwrap();
    let middle = timeline.scene_at(time("2026-01-02T00:00:00Z"));
    assert_eq!(
        middle
            .transit
            .iter()
            .find(|point| point.id == "Sun")
            .unwrap()
            .longitude_degrees,
        10.0
    );
    assert_eq!(
        timeline
            .scene_at(time("2026-01-03T00:00:00Z"))
            .transit
            .iter()
            .find(|point| point.id == "Sun")
            .unwrap()
            .longitude_degrees,
        20.0
    );
}

#[test]
fn svg_is_deterministic_accessible_escaped_and_oriented() {
    let artifact = comparison(
        natal(),
        transit("2026-01-01T00:00:00Z", 359.0, 1.0),
        &[
            CelestialObject::Sun,
            CelestialObject::Moon,
            CelestialObject::Mercury,
        ],
    );
    let scene = ChartScene::from_comparison(&artifact).unwrap();
    let ascendant = render_biwheel_svg(
        &scene,
        &RenderOptions {
            orientation: WheelOrientation::AscendantLeft,
            title: "Fictional <chart> & test".into(),
        },
    );
    let zodiac = render_biwheel_svg(
        &scene,
        &RenderOptions {
            orientation: WheelOrientation::ZodiacZeroTop,
            title: "Fictional chart".into(),
        },
    );
    assert_eq!(
        ascendant,
        render_biwheel_svg(
            &scene,
            &RenderOptions {
                orientation: WheelOrientation::AscendantLeft,
                title: "Fictional <chart> & test".into(),
            }
        )
    );
    assert!(ascendant.contains("role=\"img\""));
    assert!(ascendant.contains("aria-labelledby=\"chart-title chart-description\""));
    assert!(ascendant.contains("Fictional &lt;chart&gt; &amp; test"));
    assert!(ascendant.contains("id=\"natal-point-sun\""));
    assert!(ascendant.contains("id=\"transit-tick-sun\""));
    assert!(ascendant.contains("id=\"aspect-layer\""));
    assert!(!ascendant.contains("NaN"));
    assert!(!ascendant.contains("inf"));
    assert_ne!(ascendant, zodiac);
    assert!(zodiac.contains("data-orientation=\"zodiac-zero-top\""));
    assert_eq!(
        format!("{:x}", Sha256::digest(ascendant.as_bytes())),
        include_str!("../../../fixtures/snapshots/transit-biwheel.sha256").trim()
    );
}

#[test]
fn collision_resolution_keeps_zero_crossing_clusters_together() {
    let input = [359.0, 0.0, 1.0, 90.0];
    let output = resolve_circular_collisions(&input, 224.0, 21.0);
    assert_eq!(output.len(), input.len());
    assert!(output.iter().all(|value| value.is_finite()));
    assert_ne!(output[..3], input[..3]);
    let circular_gap = |left: f64, right: f64| {
        let difference = (left - right).abs();
        difference.min(360.0 - difference)
    };
    assert!(circular_gap(output[0], output[1]) > 4.0);
    assert!(circular_gap(output[1], output[2]) > 4.0);
}
