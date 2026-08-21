use std::collections::BTreeMap;

use astraeus_core::*;

fn location() -> GeographicLocation {
    GeographicLocation::new(40.7128, -74.0060, 10.0).unwrap()
}

fn instant() -> UtcInstant {
    UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap()
}

fn houses() -> HouseCusps {
    HouseCusps::new(
        (0..12).map(|n| f64::from(n) * 30.0).collect(),
        ChartAngles::new(
            AngularPosition::new(0.0, 360.0).unwrap(),
            AngularPosition::new(270.0, 360.0).unwrap(),
            AngularPosition::new(180.0, 360.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn utc_input_is_normalized() {
    let value = UtcInstant::parse_rfc3339("2000-01-01T07:00:00-05:00").unwrap();
    assert_eq!(value, instant());
}

#[test]
fn invalid_coordinates_are_rejected() {
    assert!(matches!(
        GeographicLocation::new(91.0, 0.0, 0.0),
        Err(ValidationError::OutOfRange {
            field: "latitude_degrees",
            ..
        })
    ));
    assert!(matches!(
        GeographicLocation::new(f64::NAN, 0.0, 0.0),
        Err(ValidationError::NonFinite {
            field: "latitude_degrees"
        })
    ));
}

#[test]
fn zodiac_and_ayanamsa_must_agree() {
    let tropical = CalculationRequest::new(
        instant(),
        location(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        Some(Ayanamsa::Lahiri),
        HouseSystem::Placidus,
    );
    assert_eq!(tropical.unwrap_err(), ValidationError::UnexpectedAyanamsa);

    let sidereal = CalculationRequest::new(
        instant(),
        location(),
        vec![CelestialObject::Sun],
        Zodiac::Sidereal,
        None,
        HouseSystem::WholeSign,
    );
    assert_eq!(sidereal.unwrap_err(), ValidationError::MissingAyanamsa);
}

#[test]
fn duplicate_and_empty_object_requests_are_rejected() {
    let empty = CalculationRequest::new(
        instant(),
        location(),
        vec![],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    );
    assert_eq!(empty.unwrap_err(), ValidationError::EmptyObjectSet);

    let duplicate = CalculationRequest::new(
        instant(),
        location(),
        vec![CelestialObject::Moon, CelestialObject::Moon],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    );
    assert_eq!(
        duplicate.unwrap_err(),
        ValidationError::DuplicateObject(CelestialObject::Moon)
    );
}

#[test]
fn speed_controls_retrograde_state() {
    let direct = Position::new(12.0, 0.0, 1.0, 0.1).unwrap();
    let retrograde = Position::new(12.0, 0.0, 1.0, -0.1).unwrap();
    assert!(!direct.is_retrograde());
    assert!(retrograde.is_retrograde());
}

#[test]
fn mock_fails_instead_of_returning_partial_success() {
    let mut positions = BTreeMap::new();
    positions.insert(
        CelestialObject::Sun,
        Position::new(280.0, 0.0, 1.0, 1.0).unwrap(),
    );
    let adapter = DeterministicMock::new(positions, houses());
    let request = CalculationRequest::new(
        instant(),
        location(),
        vec![CelestialObject::Sun, CelestialObject::Moon],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();

    assert_eq!(
        adapter.calculate(&request).unwrap_err(),
        CalculationError::MissingObject(CelestialObject::Moon)
    );
}

#[test]
fn mock_returns_only_the_complete_requested_set() {
    let mut positions = BTreeMap::new();
    positions.insert(
        CelestialObject::Sun,
        Position::new(280.0, 0.0, 1.0, 1.0).unwrap(),
    );
    positions.insert(
        CelestialObject::Moon,
        Position::new(223.0, 5.0, 0.002, 12.0).unwrap(),
    );
    let adapter = DeterministicMock::new(positions, houses());
    let request = CalculationRequest::new(
        instant(),
        location(),
        vec![CelestialObject::Moon],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();

    let result = adapter.calculate(&request).unwrap();
    assert_eq!(
        result.positions().keys().copied().collect::<Vec<_>>(),
        vec![CelestialObject::Moon]
    );
}

#[test]
fn result_rejects_unrequested_provider_output() {
    let request = CalculationRequest::new(
        instant(),
        location(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    let positions = BTreeMap::from([
        (
            CelestialObject::Sun,
            Position::new(280.0, 0.0, 1.0, 1.0).unwrap(),
        ),
        (
            CelestialObject::Moon,
            Position::new(223.0, 5.0, 0.002, 12.0).unwrap(),
        ),
    ]);

    let provenance =
        CalculationProvenance::new("test", "1", EphemerisSource::Synthetic, None).unwrap();
    assert_eq!(
        CalculationResult::new(&request, positions, houses(), provenance).unwrap_err(),
        CalculationError::UnexpectedObject(CelestialObject::Moon)
    );
}

#[test]
fn provenance_rejects_empty_identity_fields() {
    assert_eq!(
        CalculationProvenance::new(" ", "1", EphemerisSource::Synthetic, None).unwrap_err(),
        ValidationError::EmptyText { field: "provider" }
    );
}

#[test]
fn provenance_json_cannot_bypass_validation() {
    let invalid = r#"{
        "provider": "",
        "provider_version": "1",
        "ephemeris_source": "synthetic",
        "data_revision": null
    }"#;
    assert!(serde_json::from_str::<CalculationProvenance>(invalid).is_err());
}

#[test]
fn deserialization_preserves_domain_validation() {
    let invalid_location = r#"{
        "latitude_degrees": 91.0,
        "longitude_degrees": 0.0,
        "elevation_meters": 0.0
    }"#;
    assert!(serde_json::from_str::<GeographicLocation>(invalid_location).is_err());

    let invalid_position = r#"{
        "longitude_degrees": 360.0,
        "latitude_degrees": 0.0,
        "distance_au": 1.0,
        "longitude_speed_degrees_per_day": 1.0
    }"#;
    assert!(serde_json::from_str::<Position>(invalid_position).is_err());

    let invalid_houses = r#"{
        "cusps_degrees": [0.0, 30.0],
        "angles": {}
    }"#;
    assert!(serde_json::from_str::<HouseCusps>(invalid_houses).is_err());
}

#[test]
fn request_json_cannot_bypass_cross_field_rules() {
    let request = r#"{
        "instant": "2000-01-01T12:00:00Z",
        "location": {
            "latitude_degrees": 0.0,
            "longitude_degrees": 0.0,
            "elevation_meters": 0.0
        },
        "objects": ["sun", "sun"],
        "zodiac": "sidereal",
        "ayanamsa": null,
        "house_system": "placidus"
    }"#;
    assert!(serde_json::from_str::<CalculationRequest>(request).is_err());
}

#[test]
fn utc_json_round_trip_normalizes_offsets() {
    let instant: UtcInstant = serde_json::from_str(r#""2000-01-01T07:00:00-05:00""#).unwrap();
    assert_eq!(
        instant,
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap()
    );
    assert_eq!(
        serde_json::to_string(&instant).unwrap(),
        r#""2000-01-01T12:00:00Z""#
    );
}
