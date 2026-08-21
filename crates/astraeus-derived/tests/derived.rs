use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinition, AspectDefinitions, AspectKind, AspectPhase,
    CalculationOptions, CalculationRequest, CelestialObject, ChartAngles, ChartPointId,
    ChartPointSelection, DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps,
    HouseSystem, Position, UtcInstant, Zodiac,
};
use astraeus_derived::{DerivedArtifactError, DerivedChartArtifact};
use astraeus_specifications::ChartSpecification;

fn inputs() -> (CalculationArtifact, ChartSpecification) {
    let options = CalculationOptions::new(
        vec![CelestialObject::Sun, CelestialObject::Moon],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    let request = CalculationRequest::from_options(
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(51.4779, 0.0, 46.0).unwrap(),
        options.clone(),
    );
    let positions = BTreeMap::from([
        (
            CelestialObject::Sun,
            Position::new(0.0, 0.0, 1.0, 1.0).unwrap(),
        ),
        (
            CelestialObject::Moon,
            Position::new(89.0, 0.0, 0.002, 2.0).unwrap(),
        ),
    ]);
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
    let result = DeterministicMock::new(positions, houses)
        .calculate(&request)
        .unwrap();
    let calculation = CalculationArtifact::new(request, result).unwrap();
    let specification = ChartSpecification::with_aspect_points(
        options,
        AspectDefinitions::new(vec![
            AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
        ])
        .unwrap(),
        ChartPointSelection::new(vec![ChartPointId::Sun, ChartPointId::Moon]).unwrap(),
    )
    .unwrap();
    (calculation, specification)
}

#[test]
fn derived_artifact_round_trips_with_stable_identity() {
    let (calculation, specification) = inputs();
    let artifact = DerivedChartArtifact::new(calculation, specification).unwrap();
    assert_eq!(artifact.aspects().len(), 1);
    assert_eq!(artifact.aspects()[0].phase(), AspectPhase::Applying);

    let json = artifact.to_json().unwrap();
    let decoded = DerivedChartArtifact::from_json(&json).unwrap();
    assert_eq!(decoded, artifact);
    assert_eq!(decoded.to_json().unwrap(), json);
    assert_eq!(
        artifact.content_id().unwrap(),
        format!("sha256:{}", artifact.content_sha256().unwrap())
    );
    assert_eq!(
        artifact.content_sha256().unwrap(),
        "defc7d052eab84b0942b37ebb54b3f621c55979aa1b78b780f3304695501aa11"
    );
}

#[test]
fn calculation_policy_must_match() {
    let (calculation, _) = inputs();
    let specification = ChartSpecification::new(
        CalculationOptions::new(
            vec![CelestialObject::Sun],
            Zodiac::Tropical,
            None,
            HouseSystem::Placidus,
        )
        .unwrap(),
        AspectDefinitions::new(vec![]).unwrap(),
    );
    assert!(matches!(
        DerivedChartArtifact::new(calculation, specification),
        Err(DerivedArtifactError::CalculationPolicyMismatch)
    ));
}

#[test]
fn derived_values_versions_and_unknown_fields_are_revalidated() {
    let (calculation, specification) = inputs();
    let json = DerivedChartArtifact::new(calculation, specification)
        .unwrap()
        .to_json()
        .unwrap();
    assert!(matches!(
        DerivedChartArtifact::from_json(&json.replacen("\"applying\"", "\"separating\"", 1)),
        Err(DerivedArtifactError::Json(_))
    ));
    assert!(matches!(
        DerivedChartArtifact::from_json(&json.replacen(
            "\"orb_degrees\":1.0",
            "\"orb_degrees\":0.5",
            1
        )),
        Err(DerivedArtifactError::Json(_))
    ));
    assert!(matches!(
        DerivedChartArtifact::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":2",
            1
        )),
        Err(DerivedArtifactError::UnsupportedSchema(2))
    ));
    assert!(matches!(
        DerivedChartArtifact::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"extra\":true",
            1
        )),
        Err(DerivedArtifactError::Json(_))
    ));
    let mut missing_aspects: serde_json::Value = serde_json::from_str(&json).unwrap();
    missing_aspects["aspects"] = serde_json::json!([]);
    assert!(matches!(
        DerivedChartArtifact::from_json(&serde_json::to_string(&missing_aspects).unwrap()),
        Err(DerivedArtifactError::AspectMismatch)
    ));
}
