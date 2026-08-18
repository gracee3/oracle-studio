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

fn chart_with_houses(
    timestamp: &str,
    points: &[(CelestialObject, f64, f64)],
    cusps: [f64; 12],
    ascendant: f64,
    midheaven: f64,
    vertex: f64,
) -> DerivedChartArtifact {
    let objects: Vec<_> = points.iter().map(|(object, _, _)| *object).collect();
    let options =
        CalculationOptions::new(objects, Zodiac::Tropical, None, HouseSystem::Placidus).unwrap();
    let request = CalculationRequest::from_options(
        UtcInstant::parse_rfc3339(timestamp).unwrap(),
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
        options.clone(),
    );
    let houses = HouseCusps::new(
        cusps.to_vec(),
        ChartAngles::new(
            AngularPosition::new(ascendant, 0.0).unwrap(),
            AngularPosition::new(midheaven, 0.0).unwrap(),
            AngularPosition::new(vertex, 0.0).unwrap(),
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

fn selected_ids(points: &[ChartPointId]) -> ChartPointSelection {
    ChartPointSelection::new(points.to_vec()).unwrap()
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
    let selection = points
        .iter()
        .copied()
        .map(ChartPointId::from)
        .collect::<Vec<_>>();
    comparison_with_selections(natal, transit, &selection, &selection)
}

fn comparison_with_selections(
    natal: DerivedChartArtifact,
    transit: DerivedChartArtifact,
    first_points: &[ChartPointId],
    second_points: &[ChartPointId],
) -> ComparisonArtifact {
    ComparisonArtifact::new(
        natal,
        transit,
        ComparisonSpecification::moving_second(
            ComparisonKind::TransitToNatal,
            definitions(),
            selected_ids(first_points),
            selected_ids(second_points),
        )
        .unwrap(),
    )
    .unwrap()
}

fn ten_planets(offset: f64, speed: f64) -> Vec<(CelestialObject, f64, f64)> {
    [
        CelestialObject::Sun,
        CelestialObject::Moon,
        CelestialObject::Mercury,
        CelestialObject::Venus,
        CelestialObject::Mars,
        CelestialObject::Jupiter,
        CelestialObject::Saturn,
        CelestialObject::Uranus,
        CelestialObject::Neptune,
        CelestialObject::Pluto,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, object)| {
        (
            object,
            (offset + index as f64 * 36.0).rem_euclid(360.0),
            speed,
        )
    })
    .collect()
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
fn committed_fixtures_cover_full_selected_populations_and_motion_edges() {
    let first =
        ComparisonArtifact::from_json(include_str!("../../../fixtures/comparisons/frame-01.json"))
            .unwrap();
    let second =
        ComparisonArtifact::from_json(include_str!("../../../fixtures/comparisons/frame-02.json"))
            .unwrap();
    let third =
        ComparisonArtifact::from_json(include_str!("../../../fixtures/comparisons/frame-03.json"))
            .unwrap();
    let scene = ChartScene::from_comparison(&first).unwrap();
    assert_eq!(scene.natal.points.len(), 15);
    assert_eq!(scene.transit.len(), 12);
    assert_eq!(scene.natal.points[0].id, "Sun");
    assert_eq!(scene.natal.points[10].id, "Ascendant");
    assert_eq!(scene.natal.points[14].id, "Vertex");
    assert_eq!(scene.transit[10].id, "Ascendant");
    assert_eq!(scene.transit[11].id, "Midheaven");
    assert!(
        scene
            .natal
            .houses
            .windows(2)
            .any(|pair| ((pair[1] - pair[0]).rem_euclid(360.0) - 30.0).abs() > 0.5)
    );
    assert!(
        scene
            .transit
            .iter()
            .any(|point| point.id == "Mercury" && point.retrograde)
    );
    assert!(
        scene
            .transit
            .iter()
            .any(|point| { point.id == "Jupiter" && point.longitude_speed_degrees_per_day == 0.0 })
    );
    assert!(scene.aspects.iter().any(|aspect| {
        aspect.natal_point_id == "Ascendant" || aspect.transit_point_id == "Midheaven"
    }));
    assert!(scene.transit[0].longitude_degrees > 358.0);
    assert!(scene.transit[1].longitude_degrees < 1.0);
    let timeline = TransitTimeline::from_comparisons(&[first, second, third]).unwrap();
    assert!(
        timeline
            .scene_at(time("2026-01-01T06:00:00Z"))
            .transit
            .iter()
            .any(|point| point.id == "Jupiter" && point.longitude_speed_degrees_per_day > 0.0)
    );
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
fn selected_order_structural_angles_and_normalized_lanes_drive_the_svg() {
    let natal_chart = chart_with_houses(
        "2000-01-01T00:00:00Z",
        &ten_planets(0.0, 0.0),
        [
            29.999_9, 61.25, 93.5, 126.75, 159.2, 190.4, 218.8, 246.1, 275.6, 302.2, 329.4, 351.8,
        ],
        29.999_9,
        302.2,
        205.5,
    );
    let transit_chart = chart_with_houses(
        "2026-01-01T00:00:00Z",
        &ten_planets(359.5, 1.0),
        [
            0.0, 31.0, 62.0, 93.0, 124.0, 155.0, 186.0, 217.0, 248.0, 279.0, 310.0, 341.0,
        ],
        0.0,
        270.0,
        180.0,
    );
    let natal_selection = [
        ChartPointId::Vertex,
        ChartPointId::Sun,
        ChartPointId::Ascendant,
        ChartPointId::Moon,
        ChartPointId::Midheaven,
        ChartPointId::Descendant,
        ChartPointId::ImumCoeli,
    ];
    let transit_selection = [
        ChartPointId::Midheaven,
        ChartPointId::Mercury,
        ChartPointId::Ascendant,
        ChartPointId::Descendant,
        ChartPointId::ImumCoeli,
        ChartPointId::Vertex,
        ChartPointId::Pluto,
    ];
    let artifact = comparison_with_selections(
        natal_chart,
        transit_chart,
        &natal_selection,
        &transit_selection,
    );
    let scene = ChartScene::from_comparison(&artifact).unwrap();
    assert_eq!(
        scene
            .natal
            .points
            .iter()
            .map(|point| point.id.as_str())
            .collect::<Vec<_>>(),
        [
            "Vertex",
            "Sun",
            "Ascendant",
            "Moon",
            "Midheaven",
            "Descendant",
            "ImumCoeli",
        ]
    );
    assert_eq!(
        scene
            .transit
            .iter()
            .map(|point| point.id.as_str())
            .collect::<Vec<_>>(),
        [
            "Midheaven",
            "Mercury",
            "Ascendant",
            "Descendant",
            "ImumCoeli",
            "Vertex",
            "Pluto",
        ]
    );
    assert!(!scene.natal.points.iter().any(|point| point.id == "Jupiter"));

    let svg = render_biwheel_svg(&scene, &RenderOptions::default());
    assert!(svg.contains("data-aspect-radius=\"136.920\""));
    assert!(svg.contains("data-natal-sign-radius=\"149.960\""));
    assert!(svg.contains("data-transit-glyph-radius=\"280.360\""));
    assert!(svg.contains("data-cusp-label-radius=\"309.700\""));
    assert!(!svg.contains("id=\"zodiac-layer\""));
    assert!(!svg.contains("id=\"house-label-"));
    assert_eq!(
        svg.matches("class=\"house-cusp house-cusp--axis\"").count(),
        4
    );
    assert_eq!(svg.matches("data-role=\"cusp-label\"").count(), 12);
    assert!(svg.contains(">00° ♉ 00′</text>"));
    assert!(svg.contains("id=\"natal-point-vertex\""));
    assert!(!svg.contains("id=\"natal-point-ascendant\""));
    assert!(!svg.contains("id=\"natal-point-midheaven\""));
    assert!(!svg.contains("id=\"natal-point-descendant\""));
    assert!(!svg.contains("id=\"natal-point-imumcoeli\""));
    assert!(svg.contains("id=\"transit-point-ascendant\""));
    assert!(svg.contains("id=\"transit-point-midheaven\""));
    assert!(svg.contains("id=\"transit-point-descendant\""));
    assert!(svg.contains("id=\"transit-point-imumcoeli\""));
    assert!(svg.contains("id=\"transit-point-vertex\""));
    assert!(!svg.contains("id=\"transit-point-jupiter\""));
    assert!(svg.contains("data-role=\"sign\""));
    assert!(svg.contains("data-role=\"position\""));
    assert!(svg.contains("data-role=\"glyph\""));
    assert!(svg.contains("data-role=\"tick\""));
    assert!(svg.contains("data-role=\"leader\""));
    assert!(svg.contains("data-role=\"aspect-line\""));
    assert!(svg.contains("data-role=\"aspect-glyph\""));
    assert!(svg.contains("☌"));
    assert!(svg.contains("id=\"aspect--sun--midheaven--square--line\""));
    let endpoint = |visual_longitude: f64| {
        let radians = (visual_longitude - 90.0).to_radians();
        (
            360.0 + 136.92 * radians.cos(),
            360.0 + 136.92 * radians.sin(),
        )
    };
    let natal_sun = endpoint(240.0);
    let transit_midheaven = endpoint(150.0);
    assert!(svg.contains(&format!(
        "id=\"aspect--sun--midheaven--square--line\" data-role=\"aspect-line\" x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"",
        natal_sun.0, natal_sun.1, transit_midheaven.0, transit_midheaven.1
    )));
    assert!(svg.contains("Sun Square Midheaven (orb 0.000000°, phase"));
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
        },
    );
    let zodiac = render_biwheel_svg(
        &scene,
        &RenderOptions {
            orientation: WheelOrientation::ZodiacZeroTop,
        },
    );
    assert_eq!(
        ascendant,
        render_biwheel_svg(
            &scene,
            &RenderOptions {
                orientation: WheelOrientation::AscendantLeft,
            }
        )
    );
    assert!(ascendant.contains("role=\"img\""));
    assert!(ascendant.contains("aria-labelledby=\"chart-title chart-description\""));
    assert!(ascendant.contains("<title id=\"chart-title\">Transit biwheel</title>"));
    assert!(!ascendant.contains(&scene.timestamp));
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
