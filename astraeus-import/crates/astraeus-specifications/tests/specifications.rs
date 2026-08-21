use astraeus_core::{
    AspectDefinition, AspectDefinitions, AspectKind, Ayanamsa, CalculationOptions, CelestialObject,
    GeographicLocation, HouseSystem, UtcInstant, Zodiac,
};
use astraeus_specifications::{ChartSpecification, SpecificationError};

fn specification() -> ChartSpecification {
    ChartSpecification::new(
        CalculationOptions::new(
            vec![CelestialObject::Sun, CelestialObject::Moon],
            Zodiac::Sidereal,
            Some(Ayanamsa::Lahiri),
            HouseSystem::WholeSign,
        )
        .unwrap(),
        AspectDefinitions::new(vec![
            AspectDefinition::new(AspectKind::Conjunction, 8.0).unwrap(),
            AspectDefinition::new(AspectKind::Opposition, 8.0).unwrap(),
        ])
        .unwrap(),
    )
}

#[test]
fn schema_v1_round_trips_and_builds_requests() {
    let specification = specification();
    let json = specification.to_json().unwrap();
    let decoded = ChartSpecification::from_json(&json).unwrap();
    assert_eq!(decoded, specification);
    assert_eq!(decoded.to_json().unwrap(), json);

    let request = decoded.request(
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(51.4779, 0.0, 46.0).unwrap(),
    );
    assert_eq!(
        request.objects(),
        &[CelestialObject::Sun, CelestialObject::Moon]
    );
    assert_eq!(request.zodiac(), Zodiac::Sidereal);
    assert_eq!(request.ayanamsa(), Some(Ayanamsa::Lahiri));
    assert_eq!(request.house_system(), HouseSystem::WholeSign);
    assert_eq!(
        json,
        r#"{"schema_version":1,"calculation":{"objects":["sun","moon"],"zodiac":"sidereal","ayanamsa":"lahiri","house_system":"whole_sign"},"aspects":[{"kind":"conjunction","orb_degrees":8.0},{"kind":"opposition","orb_degrees":8.0}],"aspect_points":["sun","moon","ascendant","midheaven","descendant","imum_coeli","vertex"]}"#
    );
}

#[test]
fn unsupported_versions_and_unknown_fields_are_rejected() {
    let json = specification().to_json().unwrap();
    assert!(matches!(
        ChartSpecification::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":2",
            1
        )),
        Err(SpecificationError::UnsupportedSchema(2))
    ));
    assert!(matches!(
        ChartSpecification::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"extra\":true",
            1
        )),
        Err(SpecificationError::Json(_))
    ));
}

#[test]
fn nested_calculation_and_aspect_invariants_remain_active() {
    let json = specification().to_json().unwrap();
    for invalid in [
        json.replacen("\"sun\",\"moon\"", "\"sun\",\"sun\"", 1),
        json.replacen("\"sidereal\"", "\"tropical\"", 1),
        json.replacen("\"orb_degrees\":8.0", "\"orb_degrees\":-1.0", 1),
    ] {
        assert!(matches!(
            ChartSpecification::from_json(&invalid),
            Err(SpecificationError::Json(_))
        ));
    }
}
