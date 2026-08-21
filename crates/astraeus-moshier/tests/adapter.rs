use astraeus_core::{
    Ayanamsa, CalculationError, CalculationRequest, CelestialObject, EphemerisAdapter,
    EphemerisSource, UtcInstant, Zodiac,
};
use astraeus_fixtures::{FixtureMismatch, GoldenFixture};
use astraeus_moshier::MoshierEphemerisAdapter;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/astraeus/swetest-v2.10.03"
);

fn fixture(name: &str) -> GoldenFixture {
    let json = std::fs::read_to_string(format!("{FIXTURES}/{name}.json")).unwrap();
    GoldenFixture::from_json(&json).unwrap()
}

#[test]
fn pure_rust_moshier_matches_every_file_free_reference() {
    let adapter = MoshierEphemerisAdapter::new();
    for name in [
        "j2000-greenwich-tropical-placidus",
        "j2000-greenwich-sidereal-lahiri-placidus",
        "2024-new-york-tropical-koch",
        "2024-new-york-sidereal-fagan-koch",
    ] {
        let fixture = fixture(name);
        let actual = adapter.calculate(fixture.request()).unwrap();
        if let Err(error) = fixture.compare(&actual) {
            assert!(
                error.mismatches().iter().all(|mismatch| {
                    matches!(
                        mismatch,
                        FixtureMismatch::Numeric {
                            path,
                            expected,
                            actual,
                            ..
                        } if path == "positions.TrueNode.longitude_speed_degrees_per_day"
                            && (expected - actual).abs() <= 5e-6
                    )
                }),
                "unexpected fixture mismatch: {error:?}"
            );
        }
    }
}

#[test]
fn successful_result_identifies_the_pure_moshier_provider() {
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    let result = MoshierEphemerisAdapter::new()
        .calculate(fixture.request())
        .unwrap();
    assert_eq!(result.provenance().provider(), "swisseph-rs Moshier");
    assert_eq!(result.provenance().provider_version(), "0.2.0");
    assert_eq!(
        result.provenance().ephemeris_source(),
        EphemerisSource::Moshier
    );
    assert_eq!(result.provenance().data_revision(), None);
}

#[test]
fn every_astraeus_ayanamsa_is_supported() {
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    for ayanamsa in [
        Ayanamsa::FaganBradley,
        Ayanamsa::Lahiri,
        Ayanamsa::DeLuce,
        Ayanamsa::Raman,
        Ayanamsa::Krishnamurti,
        Ayanamsa::Yukteshwar,
        Ayanamsa::JnBhasin,
    ] {
        let request = CalculationRequest::new(
            fixture.request().instant(),
            fixture.request().location(),
            vec![CelestialObject::Sun],
            Zodiac::Sidereal,
            Some(ayanamsa),
            fixture.request().house_system(),
        )
        .unwrap();
        MoshierEphemerisAdapter::new().calculate(&request).unwrap();
    }
}

#[test]
fn chiron_and_dates_outside_the_moshier_range_are_rejected() {
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    let chiron = CalculationRequest::new(
        fixture.request().instant(),
        fixture.request().location(),
        vec![CelestialObject::Chiron],
        Zodiac::Tropical,
        None,
        fixture.request().house_system(),
    )
    .unwrap();
    assert_eq!(
        MoshierEphemerisAdapter::new()
            .calculate(&chiron)
            .unwrap_err(),
        CalculationError::UnsupportedObject(CelestialObject::Chiron)
    );

    let out_of_range = CalculationRequest::new(
        UtcInstant::parse_rfc3339("5000-01-01T00:00:00Z").unwrap(),
        fixture.request().location(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        fixture.request().house_system(),
    )
    .unwrap();
    assert!(matches!(
        MoshierEphemerisAdapter::new().calculate(&out_of_range),
        Err(CalculationError::DataUnavailable(_))
    ));
}
