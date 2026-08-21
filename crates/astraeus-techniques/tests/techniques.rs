use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinitions, CalculationOptions, CalculationRequest, CelestialObject,
    ChartAngles, ChartPointId, DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps,
    HouseSystem, Position, UtcInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use astraeus_techniques::{
    AnglePolicy, ArcApplication, CompositeFramework, ProgressionMethod, SolarArcMethod,
    SyntheticChartArtifact, TechniqueError, cast_progressed, harmonic, midpoint_composite,
    solar_arc, symbolic_instant,
};

fn chart(longitude: f64) -> DerivedChartArtifact {
    let options = CalculationOptions::new(
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
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
            AngularPosition::new(0.0, 1.0).unwrap(),
            AngularPosition::new(270.0, 1.0).unwrap(),
            AngularPosition::new(180.0, 1.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = DeterministicMock::new(
        BTreeMap::from([(
            CelestialObject::Sun,
            Position::new(longitude, 0.0, 1.0, 1.0).unwrap(),
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

#[test]
fn progression_time_keys_are_explicit() {
    let natal = UtcInstant::parse_rfc3339("2000-01-01T00:00:00Z").unwrap();
    let target = UtcInstant::parse_rfc3339("2001-01-01T00:00:00Z").unwrap();
    let secondary = symbolic_instant(natal, target, ProgressionMethod::Secondary).unwrap();
    assert!((secondary.as_datetime() - natal.as_datetime()).num_hours() >= 23);
    assert!((secondary.as_datetime() - natal.as_datetime()).num_hours() <= 24);
    assert_eq!(
        symbolic_instant(target, natal, ProgressionMethod::Secondary)
            .unwrap_err()
            .to_string(),
        TechniqueError::TargetPrecedesNatal.to_string()
    );
}

#[test]
fn harmonic_transforms_points_without_fabricating_houses() {
    let artifact = harmonic(&chart(200.0), 2).unwrap();
    assert_eq!(
        artifact.points()[&ChartPointId::Sun].longitude_degrees(),
        40.0
    );
    assert!(artifact.house_cusps_degrees().is_none());
    assert!(harmonic(&chart(10.0), 1).is_err());
    assert!(artifact.content_id().unwrap().starts_with("sha256:"));
}

#[test]
fn composite_uses_shortest_arc_and_forward_opposition_tie() {
    let artifact =
        midpoint_composite(&chart(350.0), &chart(10.0), CompositeFramework::PointsOnly).unwrap();
    assert_eq!(
        artifact.points()[&ChartPointId::Sun].longitude_degrees(),
        0.0
    );
    let tie = midpoint_composite(
        &chart(10.0),
        &chart(190.0),
        CompositeFramework::MidpointAnglesAndCusps,
    )
    .unwrap();
    assert_eq!(tie.points()[&ChartPointId::Sun].longitude_degrees(), 100.0);
    assert!(tie.house_cusps_degrees().is_some());
}

#[test]
fn naibod_arc_can_be_restricted_to_angles() {
    let natal = chart(20.0);
    let target = UtcInstant::parse_rfc3339("2001-01-01T12:00:00Z").unwrap();
    let directed = solar_arc(
        &natal,
        None,
        target,
        SolarArcMethod::Naibod,
        ArcApplication::AnglesOnly,
    )
    .unwrap();
    assert_eq!(
        directed.points()[&ChartPointId::Sun].longitude_degrees(),
        20.0
    );
    assert!(directed.points()[&ChartPointId::Ascendant].longitude_degrees() > 0.9);
}

#[test]
fn technique_motion_is_defined_only_when_the_method_defines_it() {
    let natal = chart(20.0);
    let adapter = DeterministicMock::new(
        natal.calculation().result().positions().clone(),
        natal.calculation().result().houses().clone(),
    );
    let target = UtcInstant::parse_rfc3339("2001-01-01T12:00:00Z").unwrap();
    let secondary = cast_progressed(
        &adapter,
        &natal,
        target,
        ProgressionMethod::Secondary,
        AnglePolicy::NatalFixed,
    )
    .unwrap();
    assert!(
        secondary.points()[&ChartPointId::Sun]
            .motion_degrees_per_target_day()
            .is_some()
    );
    assert_eq!(
        secondary.points()[&ChartPointId::Ascendant].motion_degrees_per_target_day(),
        None
    );
    let tertiary = cast_progressed(
        &adapter,
        &natal,
        target,
        ProgressionMethod::TertiaryI,
        AnglePolicy::RecastAtSymbolicInstant,
    )
    .unwrap();
    assert_eq!(
        tertiary.points()[&ChartPointId::Sun].motion_degrees_per_target_day(),
        None
    );
}

#[test]
fn synthetic_artifact_revalidates_derived_values() {
    let artifact = harmonic(&chart(20.0), 2).unwrap();
    let json = artifact.to_json().unwrap();
    assert_eq!(SyntheticChartArtifact::from_json(&json).unwrap(), artifact);
    let mut tampered: serde_json::Value = serde_json::from_str(&json).unwrap();
    tampered["points"]["sun"]["longitude_degrees"] = serde_json::json!(41.0);
    assert!(SyntheticChartArtifact::from_json(&tampered.to_string()).is_err());
    assert!(
        SyntheticChartArtifact::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":2",
            1
        ))
        .is_err()
    );
}
