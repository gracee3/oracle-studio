use std::collections::BTreeMap;

use astraeus_core::{
    CalculationProvenance, CalculationRequest, CalculationResult, CelestialObject, EphemerisSource,
    Position,
};
use astraeus_fixtures::{FixtureError, FixtureMismatch, GoldenFixture, parse_swetest_output};

const ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/swetest-v2.10.03"
);

fn load(name: &str) -> (GoldenFixture, String) {
    let json = std::fs::read_to_string(format!("{ROOT}/{name}.json")).unwrap();
    let raw = std::fs::read_to_string(format!("{ROOT}/{name}.stdout")).unwrap();
    (GoldenFixture::from_json(&json).unwrap(), raw)
}

#[test]
fn committed_tropical_and_sidereal_references_are_self_consistent() {
    for name in [
        "j2000-greenwich-tropical-placidus",
        "j2000-greenwich-sidereal-lahiri-placidus",
        "2024-new-york-tropical-koch",
        "2024-new-york-sidereal-fagan-koch",
    ] {
        let (fixture, raw) = load(name);
        fixture.verify_raw_output(&raw).unwrap();
        let parsed = parse_swetest_output(fixture.request(), &raw).unwrap();
        fixture.compare(&parsed).unwrap();
    }
}

#[test]
fn changed_transcript_fails_provenance_check() {
    let (fixture, mut raw) = load("j2000-greenwich-tropical-placidus");
    raw.push(' ');
    assert!(matches!(
        fixture.verify_raw_output(&raw),
        Err(FixtureError::RawOutputHash { .. })
    ));
}

#[test]
fn unsupported_schema_is_rejected() {
    let json = std::fs::read_to_string(format!("{ROOT}/j2000-greenwich-tropical-placidus.json"))
        .unwrap()
        .replacen(r#""schema_version": 1"#, r#""schema_version": 2"#, 1);
    assert!(matches!(
        GoldenFixture::from_json(&json),
        Err(FixtureError::UnsupportedSchema(2))
    ));
}

#[test]
fn unknown_fixture_engine_is_rejected() {
    let json = std::fs::read_to_string(format!("{ROOT}/j2000-greenwich-tropical-placidus.json"))
        .unwrap()
        .replacen(r#""engine": "moshier""#, r#""engine": "mosheir""#, 1);
    assert!(matches!(
        GoldenFixture::from_json(&json),
        Err(FixtureError::UnsupportedEngine(engine)) if engine == "mosheir"
    ));
}

#[test]
fn comparison_reports_numeric_and_object_mismatches_together() {
    let (fixture, _) = load("j2000-greenwich-tropical-placidus");
    let mut objects = fixture.request().objects().to_vec();
    objects.retain(|object| *object != CelestialObject::Moon);
    objects.push(CelestialObject::Chiron);
    let alternate_request = CalculationRequest::new(
        fixture.request().instant(),
        fixture.request().location(),
        objects,
        fixture.request().zodiac(),
        fixture.request().ayanamsa(),
        fixture.request().house_system(),
    )
    .unwrap();

    let mut positions: BTreeMap<_, _> = fixture.expected().positions().clone();
    positions.remove(&CelestialObject::Moon);
    positions.insert(
        CelestialObject::Chiron,
        Position::new(1.0, 0.0, 1.0, 0.1).unwrap(),
    );
    positions.insert(
        CelestialObject::Sun,
        Position::new(281.0, 0.0002323, 0.983327645, 1.0194321).unwrap(),
    );
    let changed = CalculationResult::new(
        &alternate_request,
        positions,
        fixture.expected().houses().clone(),
        CalculationProvenance::new("test", "1", EphemerisSource::Synthetic, None).unwrap(),
    )
    .unwrap();
    let error = fixture.compare(&changed).unwrap_err();
    assert!(
        error
            .mismatches()
            .contains(&FixtureMismatch::MissingObject(CelestialObject::Moon))
    );
    assert!(
        error
            .mismatches()
            .contains(&FixtureMismatch::UnexpectedObject(CelestialObject::Chiron))
    );
    assert!(error.mismatches().iter().any(|mismatch| matches!(
        mismatch,
        FixtureMismatch::Numeric { path, .. } if path.ends_with("longitude_degrees")
    )));
}

#[test]
fn swetest_parser_rejects_unknown_rows() {
    let (fixture, raw) = load("j2000-greenwich-tropical-placidus");
    let raw = format!("{raw}unknown row,01.01.2000 12:00:00 UT,0.0\n");
    assert!(parse_swetest_output(fixture.request(), &raw).is_err());
}

#[test]
fn swetest_parser_rejects_a_timestamp_other_than_the_request() {
    let (fixture, raw) = load("j2000-greenwich-tropical-placidus");
    let raw = raw.replacen("01.01.2000 12:00:00 UT", "02.01.2000 12:00:00 UT", 1);
    assert!(parse_swetest_output(fixture.request(), &raw).is_err());
}
