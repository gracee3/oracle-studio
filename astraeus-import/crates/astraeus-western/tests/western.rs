use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinitions, CalculationOptions, CalculationRequest, CelestialObject,
    ChartAngles, DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps, HouseSystem,
    Position, SignPlacement, UtcInstant, Zodiac, ZodiacSign,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use astraeus_western::{
    DecanPolicy, DignityKind, RulershipPolicy, WesternArtifactError, WesternChartArtifact,
    WesternPolicy, decan_ruler, dignities, sign_rulers,
};

fn sign_placement(longitude: f64) -> SignPlacement {
    SignPlacement::from_longitude(longitude).unwrap()
}

fn chart() -> DerivedChartArtifact {
    let options = CalculationOptions::new(
        vec![CelestialObject::Sun, CelestialObject::Saturn],
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
            AngularPosition::new(0.0, 360.0).unwrap(),
            AngularPosition::new(270.0, 360.0).unwrap(),
            AngularPosition::new(180.0, 360.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = DeterministicMock::new(
        BTreeMap::from([
            (
                CelestialObject::Sun,
                Position::new(19.0, 0.0, 1.0, 1.0).unwrap(),
            ),
            (
                CelestialObject::Saturn,
                Position::new(315.0, 0.0, 9.0, 0.1).unwrap(),
            ),
        ]),
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
fn traditional_and_modern_rulers_are_explicit() {
    assert_eq!(
        sign_rulers(ZodiacSign::Aquarius, RulershipPolicy::TraditionalV1),
        vec![CelestialObject::Saturn]
    );
    assert_eq!(
        sign_rulers(ZodiacSign::Aquarius, RulershipPolicy::ModernV1),
        vec![CelestialObject::Saturn, CelestialObject::Uranus]
    );
    assert_eq!(
        sign_rulers(ZodiacSign::Scorpio, RulershipPolicy::ModernV1),
        vec![CelestialObject::Mars, CelestialObject::Pluto]
    );
}

#[test]
fn both_decan_systems_cover_exact_boundaries() {
    assert_eq!(
        decan_ruler(ZodiacSign::Aries, 0.0, DecanPolicy::ChaldeanFacesV1),
        Some((1, CelestialObject::Mars))
    );
    assert_eq!(
        decan_ruler(ZodiacSign::Aries, 10.0, DecanPolicy::ChaldeanFacesV1),
        Some((2, CelestialObject::Sun))
    );
    assert_eq!(
        decan_ruler(ZodiacSign::Leo, 20.0, DecanPolicy::TriplicityDecansV1),
        Some((3, CelestialObject::Mars))
    );
    assert!(decan_ruler(ZodiacSign::Aries, 30.0, DecanPolicy::ChaldeanFacesV1).is_none());
}

#[test]
fn essential_dignities_include_correct_exact_degrees() {
    let sun = dignities(
        astraeus_core::ChartPointId::Sun,
        sign_placement(19.0),
        RulershipPolicy::TraditionalV1,
    );
    assert_eq!(sun.len(), 1);
    assert_eq!(sun[0].kind(), DignityKind::Exaltation);
    assert_eq!(sun[0].exact_longitude_degrees(), Some(19.0));
    assert_eq!(sun[0].distance_from_exact_degrees(), Some(0.0));

    let jupiter = dignities(
        astraeus_core::ChartPointId::Jupiter,
        sign_placement(105.0),
        RulershipPolicy::TraditionalV1,
    );
    assert_eq!(jupiter[0].kind(), DignityKind::Exaltation);
    assert_eq!(jupiter[0].exact_longitude_degrees(), Some(105.0));
}

#[test]
fn every_sign_and_decan_has_complete_policy_data() {
    for sign in ZodiacSign::ALL {
        assert!(!sign_rulers(sign, RulershipPolicy::TraditionalV1).is_empty());
        assert!(!sign_rulers(sign, RulershipPolicy::ModernV1).is_empty());
        for policy in [
            DecanPolicy::ChaldeanFacesV1,
            DecanPolicy::TriplicityDecansV1,
        ] {
            for (degree, expected_index) in [(0.0, 1), (10.0, 2), (20.0, 3)] {
                assert_eq!(decan_ruler(sign, degree, policy).unwrap().0, expected_index);
            }
        }
    }
}

#[test]
fn western_artifact_round_trips_and_rejects_tampering() {
    let artifact = WesternChartArtifact::new(
        chart(),
        WesternPolicy::new(RulershipPolicy::ModernV1, DecanPolicy::ChaldeanFacesV1),
    );
    let json = artifact.to_json().unwrap();
    assert_eq!(WesternChartArtifact::from_json(&json).unwrap(), artifact);
    assert_eq!(
        artifact.content_id().unwrap(),
        format!("sha256:{}", artifact.content_sha256().unwrap())
    );
    assert!(matches!(
        WesternChartArtifact::from_json(&json.replacen(
            "\"decan_index\":2",
            "\"decan_index\":3",
            1
        )),
        Err(WesternArtifactError::AnnotationMismatch)
    ));
}
