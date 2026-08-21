use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_comparison::{
    ComparisonArtifact, ComparisonArtifactError, ComparisonKind, ComparisonMotionPolicy,
    ComparisonSpecification,
};
use astraeus_core::{
    AngularPosition, AspectDefinition, AspectDefinitions, AspectKind, AspectPhase,
    CalculationOptions, CalculationRequest, CelestialObject, ChartAngles, ChartPointId,
    ChartPointSelection, DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps,
    HouseSystem, Position, UtcInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use astraeus_techniques::harmonic;

fn chart(longitude: f64, speed: f64, zodiac: Zodiac) -> DerivedChartArtifact {
    let ayanamsa = (zodiac == Zodiac::Sidereal).then_some(astraeus_core::Ayanamsa::Lahiri);
    let options = CalculationOptions::new(
        vec![CelestialObject::Sun],
        zodiac,
        ayanamsa,
        HouseSystem::WholeSign,
    )
    .unwrap();
    let request = CalculationRequest::from_options(
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
        options.clone(),
    );
    let houses = HouseCusps::new(
        (0..12).map(|index| f64::from(index) * 30.0).collect(),
        ChartAngles::new(
            AngularPosition::new(0.0, 360.0).unwrap(),
            AngularPosition::new(270.0, 360.0).unwrap(),
            AngularPosition::new(180.0, 360.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = DeterministicMock::new(
        BTreeMap::from([(
            CelestialObject::Sun,
            Position::new(longitude, 0.0, 1.0, speed).unwrap(),
        )]),
        houses,
    )
    .calculate(&request)
    .unwrap();
    DerivedChartArtifact::new(
        CalculationArtifact::new(request, result).unwrap(),
        ChartSpecification::new(options, AspectDefinitions::new(vec![]).unwrap()),
    )
    .unwrap()
}

fn points() -> ChartPointSelection {
    ChartPointSelection::new(vec![ChartPointId::Sun]).unwrap()
}

fn square() -> AspectDefinitions {
    AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
    ])
    .unwrap()
}

#[test]
fn point_selection_accessors_preserve_declared_order_and_json_shape() {
    let first = [
        ChartPointId::Moon,
        ChartPointId::Sun,
        ChartPointId::Ascendant,
    ];
    let second = [
        ChartPointId::Vertex,
        ChartPointId::Mars,
        ChartPointId::Midheaven,
    ];
    let specification = ComparisonSpecification::new(
        ComparisonKind::Generic,
        square(),
        ChartPointSelection::new(first.to_vec()).unwrap(),
        ChartPointSelection::new(second.to_vec()).unwrap(),
        ComparisonMotionPolicy::BothInstantaneous,
    )
    .unwrap();

    assert_eq!(specification.first_points().as_slice(), first);
    assert_eq!(specification.second_points().as_slice(), second);

    let json = serde_json::to_string(&specification).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"generic","aspects":[{"kind":"square","orb_degrees":2.0}],"first_points":["moon","sun","ascendant"],"second_points":["vertex","mars","midheaven"],"motion":"both_instantaneous"}"#
    );
    let decoded: ComparisonSpecification = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.first_points().as_slice(), first);
    assert_eq!(decoded.second_points().as_slice(), second);
    assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
}

#[test]
fn same_point_across_layers_is_valid_and_synastry_has_no_phase() {
    let specification = ComparisonSpecification::synastry(
        AspectDefinitions::new(vec![
            AspectDefinition::new(AspectKind::Conjunction, 1.0).unwrap(),
        ])
        .unwrap(),
        points(),
        points(),
    )
    .unwrap();
    let artifact = ComparisonArtifact::new(
        chart(0.0, 1.0, Zodiac::Tropical),
        chart(0.0, 2.0, Zodiac::Tropical),
        specification,
    )
    .unwrap();
    assert_eq!(artifact.aspects().len(), 1);
    assert_eq!(artifact.aspects()[0].first(), ChartPointId::Sun);
    assert_eq!(artifact.aspects()[0].second(), ChartPointId::Sun);
    assert_eq!(artifact.aspects()[0].phase(), None);
    assert_eq!(artifact.aspects()[0].relative_speed_degrees_per_day(), None);
}

#[test]
fn moving_second_uses_only_second_layer_speed() {
    let specification = ComparisonSpecification::moving_second(
        ComparisonKind::TransitToNatal,
        square(),
        points(),
        points(),
    )
    .unwrap();
    let artifact = ComparisonArtifact::new(
        chart(0.0, 50.0, Zodiac::Tropical),
        chart(89.0, 1.0, Zodiac::Tropical),
        specification,
    )
    .unwrap();
    assert_eq!(artifact.aspects()[0].phase(), Some(AspectPhase::Applying));
    assert_eq!(
        artifact.aspects()[0].relative_speed_degrees_per_day(),
        Some(1.0)
    );
    assert_eq!(
        artifact.specification().motion(),
        ComparisonMotionPolicy::SecondMovesAgainstFirstFixed
    );
}

#[test]
fn mixed_coordinate_frames_and_missing_points_fail() {
    let specification = ComparisonSpecification::synastry(square(), points(), points()).unwrap();
    assert!(matches!(
        ComparisonArtifact::new(
            chart(0.0, 1.0, Zodiac::Tropical),
            chart(90.0, 1.0, Zodiac::Sidereal),
            specification,
        ),
        Err(ComparisonArtifactError::CoordinateFrameMismatch)
    ));

    let unavailable = ChartPointSelection::new(vec![ChartPointId::Moon]).unwrap();
    let specification = ComparisonSpecification::synastry(square(), points(), unavailable).unwrap();
    assert!(matches!(
        ComparisonArtifact::new(
            chart(0.0, 1.0, Zodiac::Tropical),
            chart(90.0, 1.0, Zodiac::Tropical),
            specification,
        ),
        Err(ComparisonArtifactError::MissingPoint {
            side: "second",
            point: ChartPointId::Moon
        })
    ));
}

#[test]
fn comparison_artifact_round_trips_and_rejects_tampering() {
    let artifact = ComparisonArtifact::new(
        chart(0.0, 1.0, Zodiac::Tropical),
        chart(89.0, 1.0, Zodiac::Tropical),
        ComparisonSpecification::moving_second(
            ComparisonKind::ProgressedToNatal,
            square(),
            points(),
            points(),
        )
        .unwrap(),
    )
    .unwrap();
    let json = artifact.to_json().unwrap();
    let content_id = artifact.content_id().unwrap();
    let decoded = ComparisonArtifact::from_json(&json).unwrap();
    assert_eq!(decoded, artifact);
    assert_eq!(decoded.to_json().unwrap(), json);
    assert_eq!(decoded.content_id().unwrap(), content_id);
    assert_eq!(
        content_id,
        format!("sha256:{}", artifact.content_sha256().unwrap())
    );
    assert!(matches!(
        ComparisonArtifact::from_json(&json.replacen("\"applying\"", "\"separating\"", 1)),
        Err(ComparisonArtifactError::AspectMismatch)
    ));
}

#[test]
fn static_synthetic_layers_reject_motion_dependent_comparisons() {
    let natal = chart(0.0, 1.0, Zodiac::Tropical);
    let harmonic = harmonic(&chart(45.0, 1.0, Zodiac::Tropical), 2).unwrap();
    let specification = ComparisonSpecification::moving_second(
        ComparisonKind::HarmonicToNatal,
        square(),
        points(),
        points(),
    )
    .unwrap();
    assert!(matches!(
        ComparisonArtifact::new(natal, harmonic, specification),
        Err(ComparisonArtifactError::MissingMotion {
            side: "second",
            point: ChartPointId::Sun
        })
    ));
}
