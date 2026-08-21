use std::collections::BTreeMap;

use astraeus_artifacts::{ArtifactError, CalculationArtifact};
use astraeus_core::{
    AngularPosition, CalculationRequest, CelestialObject, ChartAngles, DeterministicMock,
    EphemerisAdapter, GeographicLocation, HouseCusps, HouseSystem, Position, UtcInstant, Zodiac,
};

fn artifact() -> CalculationArtifact {
    let request = CalculationRequest::new(
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(51.4779, 0.0, 46.0).unwrap(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    let positions = BTreeMap::from([(
        CelestialObject::Sun,
        Position::new(280.3689197, 0.0002323, 0.983327645, 1.0194321).unwrap(),
    )]);
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
    CalculationArtifact::new(request, result).unwrap()
}

#[test]
fn canonical_json_round_trips_without_changing_identity() {
    let artifact = artifact();
    let json = artifact.to_json().unwrap();
    let decoded = CalculationArtifact::from_json(&json).unwrap();
    assert_eq!(decoded, artifact);
    assert_eq!(decoded.to_json().unwrap(), json);
    assert_eq!(
        decoded.content_sha256().unwrap(),
        artifact.content_sha256().unwrap()
    );
    assert_eq!(
        artifact.content_id().unwrap(),
        format!("sha256:{}", artifact.content_sha256().unwrap())
    );
    assert_eq!(
        artifact.content_sha256().unwrap(),
        "2c2a67043bdc129710a5c99b085877fdaa8fb5e16c036b121ea006520c12bf78"
    );
}

#[test]
fn unsupported_versions_are_rejected() {
    let json =
        artifact()
            .to_json()
            .unwrap()
            .replacen("\"schema_version\":1", "\"schema_version\":2", 1);
    assert!(matches!(
        CalculationArtifact::from_json(&json),
        Err(ArtifactError::UnsupportedSchema(2))
    ));
}

#[test]
fn request_and_result_object_sets_must_match() {
    let json = artifact().to_json().unwrap().replacen(
        "\"objects\":[\"sun\"]",
        "\"objects\":[\"sun\",\"moon\"]",
        1,
    );
    assert!(matches!(
        CalculationArtifact::from_json(&json),
        Err(ArtifactError::InvalidResult(_))
    ));
}

#[test]
fn nested_domain_validation_remains_active() {
    let json = artifact().to_json().unwrap().replacen(
        "\"longitude_degrees\":280.3689197",
        "\"longitude_degrees\":360.0",
        1,
    );
    assert!(matches!(
        CalculationArtifact::from_json(&json),
        Err(ArtifactError::Json(_))
    ));
}

#[test]
fn unknown_fields_are_rejected() {
    let json = artifact().to_json().unwrap().replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"unexpected\":true",
        1,
    );
    assert!(matches!(
        CalculationArtifact::from_json(&json),
        Err(ArtifactError::Json(_))
    ));
}
