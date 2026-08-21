use std::collections::BTreeMap;

use astraeus_core::{
    AngularPosition, CalculationProvenance, CalculationRequest, CalculationResult, CelestialObject,
    ChartAngles, ChartPointId, EphemerisSource, GeographicLocation, HouseCusps, HouseNumber,
    HouseSystem, Position, UtcInstant, ValidationError, Zodiac, ZodiacSign, calculate_placements,
    chart_point_positions, house_for_longitude,
};

fn houses() -> HouseCusps {
    HouseCusps::new(
        vec![
            350.0, 20.0, 50.0, 80.0, 110.0, 140.0, 170.0, 200.0, 230.0, 260.0, 290.0, 320.0,
        ],
        ChartAngles::new(
            AngularPosition::new(350.0, 361.0).unwrap(),
            AngularPosition::new(80.0, 360.0).unwrap(),
            AngularPosition::new(175.0, 359.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn result() -> CalculationResult {
    let request = CalculationRequest::new(
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
        vec![CelestialObject::Sun, CelestialObject::MeanNode],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    CalculationResult::new(
        &request,
        BTreeMap::from([
            (
                CelestialObject::Sun,
                Position::new(20.0, 0.0, 1.0, 1.0).unwrap(),
            ),
            (
                CelestialObject::MeanNode,
                Position::new(5.0, 0.0, 1.0, -0.1).unwrap(),
            ),
        ]),
        houses(),
        CalculationProvenance::new("test", "1", EphemerisSource::Synthetic, None).unwrap(),
    )
    .unwrap()
}

#[test]
fn sign_boundaries_are_half_open() {
    for (longitude, sign) in [
        (0.0, ZodiacSign::Aries),
        (29.999_999, ZodiacSign::Aries),
        (30.0, ZodiacSign::Taurus),
        (330.0, ZodiacSign::Pisces),
        (359.999_999, ZodiacSign::Pisces),
    ] {
        assert_eq!(ZodiacSign::from_longitude(longitude).unwrap(), sign);
    }
}

#[test]
fn exact_cusps_belong_to_the_house_they_begin() {
    assert_eq!(house_for_longitude(350.0, &houses()).unwrap().get(), 1);
    assert_eq!(house_for_longitude(0.0, &houses()).unwrap().get(), 1);
    assert_eq!(house_for_longitude(20.0, &houses()).unwrap().get(), 2);
    assert_eq!(house_for_longitude(349.999, &houses()).unwrap().get(), 12);
}

#[test]
fn chart_points_include_opposite_angles_and_south_nodes() {
    let points = chart_point_positions(&result()).unwrap();
    assert_eq!(points[&ChartPointId::Descendant].longitude_degrees(), 170.0);
    assert_eq!(points[&ChartPointId::ImumCoeli].longitude_degrees(), 260.0);
    assert_eq!(
        points[&ChartPointId::MeanSouthNode].longitude_degrees(),
        185.0
    );
    assert_eq!(
        points[&ChartPointId::MeanSouthNode].longitude_speed_degrees_per_day(),
        -0.1
    );
    assert!(!points.contains_key(&ChartPointId::TrueSouthNode));
    assert_eq!(calculate_placements(&result()).unwrap().len(), 8);
}

#[test]
fn invalid_house_topology_and_numbers_are_rejected() {
    let repeated = HouseCusps::new(
        vec![0.0; 12],
        ChartAngles::new(
            AngularPosition::new(0.0, 0.0).unwrap(),
            AngularPosition::new(90.0, 0.0).unwrap(),
            AngularPosition::new(180.0, 0.0).unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(repeated.unwrap_err(), ValidationError::InvalidHouseTopology);
    assert_eq!(
        HouseNumber::new(0).unwrap_err(),
        ValidationError::InvalidHouseNumber(0)
    );
}
