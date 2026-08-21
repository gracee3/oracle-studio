use astraeus_core::{
    AspectDefinitions, Ayanamsa, CalculationError, CalculationOptions, CalculationRequest,
    CelestialObject, EphemerisAdapter, GeographicLocation, HouseSystem, UtcInstant, Zodiac,
};
use astraeus_events::{
    EclipseClassification, EclipseKind, EclipseSearchDirection, EventCoordinateFrame,
    EventPositionProvider, EventPositionRequest, EventSelection, GlobalEclipseProvider,
    GlobalEclipseSearch, solve_global_eclipse,
};
use astraeus_fixtures::{GoldenFixture, parse_swetest_output};
use astraeus_specifications::ChartSpecification;
use astraeus_swiss::SwissEphemerisAdapter;
use astraeus_timeseries::{
    AspectTimelineRequest, AspectTimelineSubject, calculate_aspect_timeline,
};
use std::sync::Arc;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/swetest-v2.10.03"
);

fn fixture(name: &str) -> GoldenFixture {
    let json = std::fs::read_to_string(format!("{FIXTURES}/{name}.json")).unwrap();
    GoldenFixture::from_json(&json).unwrap()
}

#[test]
fn moshier_matches_tropical_and_sidereal_references() {
    let adapter = SwissEphemerisAdapter::moshier();
    for name in [
        "j2000-greenwich-tropical-placidus",
        "j2000-greenwich-sidereal-lahiri-placidus",
        "2024-new-york-tropical-koch",
        "2024-new-york-sidereal-fagan-koch",
    ] {
        let fixture = fixture(name);
        fixture
            .compare(&adapter.calculate(fixture.request()).unwrap())
            .unwrap();
    }
}

#[test]
fn successful_results_record_provider_source_and_version() {
    let adapter = SwissEphemerisAdapter::moshier();
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    let result = adapter.calculate(fixture.request()).unwrap();
    assert_eq!(result.provenance().provider(), "Swiss Ephemeris");
    assert_eq!(result.provenance().provider_version(), "2.10.03");
    assert_eq!(
        result.provenance().ephemeris_source(),
        astraeus_core::EphemerisSource::Moshier
    );
    assert_eq!(result.provenance().data_revision(), None);
}

#[test]
fn concurrent_tropical_and_sidereal_requests_do_not_leak_global_state() {
    let adapter = Arc::new(SwissEphemerisAdapter::moshier());
    let mut threads = Vec::new();
    for index in 0..8 {
        let adapter = Arc::clone(&adapter);
        threads.push(std::thread::spawn(move || {
            let name = if index % 2 == 0 {
                "j2000-greenwich-tropical-placidus"
            } else {
                "j2000-greenwich-sidereal-lahiri-placidus"
            };
            let fixture = fixture(name);
            for _ in 0..50 {
                let result = adapter.calculate(fixture.request()).unwrap();
                fixture.compare(&result).unwrap();
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn swiss_mode_rejects_silent_moshier_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let adapter = SwissEphemerisAdapter::swiss_files(directory.path()).unwrap();
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    assert!(matches!(
        adapter.calculate(fixture.request()),
        Err(CalculationError::DataUnavailable(_))
    ));
}

#[test]
fn pinned_verifier_rejects_missing_and_tampered_files() {
    let missing = tempfile::tempdir().unwrap();
    assert!(matches!(
        SwissEphemerisAdapter::pinned_swiss_files(missing.path()),
        Err(CalculationError::DataUnavailable(_))
    ));
    for name in ["sepl_18.se1", "semo_18.se1", "seas_18.se1"] {
        std::fs::write(missing.path().join(name), b"not pinned ephemeris data").unwrap();
    }
    assert!(matches!(
        SwissEphemerisAdapter::pinned_swiss_files(missing.path()),
        Err(CalculationError::DataUnavailable(_))
    ));
}

#[test]
fn undefined_polar_houses_are_explicit_failures() {
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    let request = CalculationRequest::new(
        fixture.request().instant(),
        GeographicLocation::new(89.0, 0.0, 0.0).unwrap(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    assert!(matches!(
        SwissEphemerisAdapter::moshier().calculate(&request),
        Err(CalculationError::Provider(_))
    ));
}

#[test]
fn moshier_rejects_file_only_chiron() {
    let fixture = fixture("j2000-greenwich-tropical-placidus");
    let mut objects = fixture.request().objects().to_vec();
    objects.push(CelestialObject::Chiron);
    let request = astraeus_core::CalculationRequest::new(
        fixture.request().instant(),
        fixture.request().location(),
        objects,
        fixture.request().zodiac(),
        fixture.request().ayanamsa(),
        fixture.request().house_system(),
    )
    .unwrap();
    assert_eq!(
        SwissEphemerisAdapter::moshier()
            .calculate(&request)
            .unwrap_err(),
        CalculationError::UnsupportedObject(CelestialObject::Chiron)
    );
}

#[test]
fn every_public_ayanamsa_maps_to_a_native_mode() {
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
        SwissEphemerisAdapter::moshier()
            .calculate(&request)
            .unwrap();
    }
}

#[test]
fn event_sampling_is_house_independent_and_supports_birth_epoch_ecliptic() {
    let adapter = SwissEphemerisAdapter::moshier();
    let future = UtcInstant::parse_rfc3339("2050-01-01T00:00:00Z").unwrap();
    let tropical = adapter
        .sample_event_positions(
            &EventPositionRequest::new(
                future,
                vec![CelestialObject::Sun],
                EventCoordinateFrame::TropicalOfDate,
            )
            .unwrap(),
        )
        .unwrap();
    let birth_epoch = adapter
        .sample_event_positions(
            &EventPositionRequest::new(
                future,
                vec![CelestialObject::Sun],
                EventCoordinateFrame::BirthEpochEcliptic {
                    epoch: UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
                },
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        (tropical.positions()[&CelestialObject::Sun].longitude_degrees()
            - birth_epoch.positions()[&CelestialObject::Sun].longitude_degrees())
        .abs()
            > 0.1
    );
    assert_eq!(tropical.provenance().provider(), "Swiss Ephemeris");
}

#[test]
fn moshier_drives_an_aspect_timeline() {
    let start = UtcInstant::parse_rfc3339("2024-04-08T00:00:00Z").unwrap();
    let end = UtcInstant::parse_rfc3339("2024-04-09T00:00:00Z").unwrap();
    let request = AspectTimelineRequest::new(
        AspectTimelineSubject::moving_moving(
            CelestialObject::Sun,
            CelestialObject::Moon,
            EventCoordinateFrame::TropicalOfDate,
        )
        .unwrap(),
        astraeus_core::AspectDefinition::new(astraeus_core::AspectKind::Conjunction, 10.0).unwrap(),
        start,
        end,
        21_600,
    )
    .unwrap();
    let artifact = calculate_aspect_timeline(&SwissEphemerisAdapter::moshier(), request).unwrap();
    assert_eq!(artifact.samples().len(), 5);
    assert_eq!(
        artifact.provider_provenance().ephemeris_source(),
        astraeus_core::EphemerisSource::Moshier
    );
}

#[test]
fn moshier_finds_known_global_eclipse_maxima_and_casts_event_chart() {
    let adapter = SwissEphemerisAdapter::moshier();
    let solar = adapter
        .find_global_eclipse(
            EclipseKind::Solar,
            UtcInstant::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
            EclipseSearchDirection::Forward,
        )
        .unwrap();
    let expected_solar = UtcInstant::parse_rfc3339("2024-04-08T18:17:00Z").unwrap();
    assert!(
        (solar.exact_instant().as_datetime() - expected_solar.as_datetime())
            .num_seconds()
            .abs()
            < 180
    );
    assert!(
        solar
            .classifications()
            .contains(&EclipseClassification::Total)
    );

    let lunar = adapter
        .find_global_eclipse(
            EclipseKind::Lunar,
            UtcInstant::parse_rfc3339("2022-01-01T00:00:00Z").unwrap(),
            EclipseSearchDirection::Forward,
        )
        .unwrap();
    let expected_lunar = UtcInstant::parse_rfc3339("2022-05-16T04:11:00Z").unwrap();
    assert!(
        (lunar.exact_instant().as_datetime() - expected_lunar.as_datetime())
            .num_seconds()
            .abs()
            < 180
    );
    assert!(
        lunar
            .classifications()
            .contains(&EclipseClassification::Total)
    );

    let specification = ChartSpecification::new(
        CalculationOptions::new(
            vec![CelestialObject::Sun, CelestialObject::Moon],
            Zodiac::Tropical,
            None,
            HouseSystem::WholeSign,
        )
        .unwrap(),
        AspectDefinitions::new(vec![]).unwrap(),
    );
    let artifact = solve_global_eclipse(
        &adapter,
        &specification,
        GeographicLocation::new(40.7128, -74.006, 10.0).unwrap(),
        GlobalEclipseSearch::new(
            EclipseKind::Solar,
            UtcInstant::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
            EventSelection::Next,
            180.0,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        artifact.chart().calculation().request().instant(),
        artifact.maximum().exact_instant()
    );
    let json = artifact.to_json().unwrap();
    let round_trip = astraeus_events::GlobalEclipseChartArtifact::from_json(&json).unwrap();
    assert_eq!(round_trip, artifact);
    assert_eq!(
        round_trip.content_id().unwrap(),
        artifact.content_id().unwrap()
    );
    assert!(
        astraeus_events::GlobalEclipseChartArtifact::from_json(&json.replacen(
            "\"total\"",
            "\"partial\"",
            1
        ))
        .is_err()
    );
    assert!(artifact.content_id().unwrap().starts_with("sha256:"));
}

#[test]
#[ignore = "set ASTRAEUS_SWISS_EPHEMERIS_PATH to a directory containing pinned .se1 files"]
fn swiss_files_match_tropical_and_sidereal_references() {
    let path = std::env::var("ASTRAEUS_SWISS_EPHEMERIS_PATH").unwrap();
    let adapter = SwissEphemerisAdapter::pinned_swiss_files(path).unwrap();
    for (zodiac, ayanamsa, file) in [
        (
            Zodiac::Tropical,
            None,
            "j2000-greenwich-swiss-tropical.stdout",
        ),
        (
            Zodiac::Sidereal,
            Some(Ayanamsa::Lahiri),
            "j2000-greenwich-swiss-sidereal-lahiri.stdout",
        ),
    ] {
        let request = CalculationRequest::new(
            UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
            GeographicLocation::new(51.4779, 0.0, 46.0).unwrap(),
            vec![
                CelestialObject::Sun,
                CelestialObject::Moon,
                CelestialObject::Chiron,
            ],
            zodiac,
            ayanamsa,
            HouseSystem::Placidus,
        )
        .unwrap();
        let raw = std::fs::read_to_string(format!("{FIXTURES}/{file}")).unwrap();
        let expected = parse_swetest_output(&request, &raw).unwrap();
        let actual = adapter.calculate(&request).unwrap();
        assert_eq!(
            actual.provenance().data_revision(),
            Some("cae9ecd4b201544d85e411aced17660932514d43")
        );
        for (object, expected_position) in expected.positions() {
            let actual_position = actual.positions().get(object).unwrap();
            assert!(
                (expected_position.longitude_degrees() - actual_position.longitude_degrees()).abs()
                    <= 1e-6
            );
            assert!(
                (expected_position.latitude_degrees() - actual_position.latitude_degrees()).abs()
                    <= 1e-6
            );
            assert!(
                (expected_position.distance_au() - actual_position.distance_au()).abs() <= 1e-9
            );
            assert!(
                (expected_position.longitude_speed_degrees_per_day()
                    - actual_position.longitude_speed_degrees_per_day())
                .abs()
                    <= 1e-6
            );
        }
        for (expected_cusp, actual_cusp) in expected
            .houses()
            .cusps_degrees()
            .iter()
            .zip(actual.houses().cusps_degrees())
        {
            assert!((expected_cusp - actual_cusp).abs() <= 1e-6);
        }
    }
}

#[test]
#[ignore = "set ASTRAEUS_SWISS_EPHEMERIS_PATH to the pinned .se1 bundle"]
fn swiss_files_drive_an_aspect_timeline() {
    let path = std::env::var("ASTRAEUS_SWISS_EPHEMERIS_PATH").unwrap();
    let adapter = SwissEphemerisAdapter::pinned_swiss_files(path).unwrap();
    let start = UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap();
    let end = UtcInstant::parse_rfc3339("2000-01-01T18:00:00Z").unwrap();
    let request = AspectTimelineRequest::new(
        AspectTimelineSubject::moving_moving(
            CelestialObject::Sun,
            CelestialObject::Chiron,
            EventCoordinateFrame::TropicalOfDate,
        )
        .unwrap(),
        astraeus_core::AspectDefinition::new(astraeus_core::AspectKind::Trine, 5.0).unwrap(),
        start,
        end,
        3_600,
    )
    .unwrap();
    let artifact = calculate_aspect_timeline(&adapter, request).unwrap();
    assert_eq!(artifact.samples().len(), 7);
    assert_eq!(
        artifact.provider_provenance().data_revision(),
        Some("cae9ecd4b201544d85e411aced17660932514d43")
    );
}
