use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_comparison::{ComparisonArtifact, ComparisonKind, ComparisonSpecification};
use astraeus_core::{
    AngularPosition, AspectDefinition, AspectDefinitions, AspectKind, CalculationOptions,
    CalculationRequest, CelestialObject, ChartAngles, ChartPointId, ChartPointSelection,
    DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps, HouseSystem, Position,
    UtcInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;

const OBJECTS: [CelestialObject; 10] = [
    CelestialObject::Sun,
    CelestialObject::Moon,
    CelestialObject::Mercury,
    CelestialObject::Venus,
    CelestialObject::Mars,
    CelestialObject::Jupiter,
    CelestialObject::Saturn,
    CelestialObject::Uranus,
    CelestialObject::Neptune,
    CelestialObject::Pluto,
];

fn chart(
    timestamp: &str,
    longitudes: [f64; 10],
    speeds: [f64; 10],
    cusps: [f64; 12],
    ascendant: f64,
    midheaven: f64,
    vertex: f64,
) -> DerivedChartArtifact {
    let options = CalculationOptions::new(
        OBJECTS.to_vec(),
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .expect("fixture options are valid");
    let request = CalculationRequest::from_options(
        UtcInstant::parse_rfc3339(timestamp).expect("fixture timestamp is valid"),
        GeographicLocation::new(0.0, 0.0, 0.0).expect("fictional origin is valid"),
        options.clone(),
    );
    let positions = OBJECTS
        .into_iter()
        .zip(longitudes)
        .zip(speeds)
        .map(|((object, longitude), speed)| {
            (
                object,
                Position::new(longitude, 0.0, 1.0, speed).expect("fixture position is valid"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let houses = HouseCusps::new(
        cusps.to_vec(),
        ChartAngles::new(
            AngularPosition::new(ascendant, 0.0).expect("fixture ascendant is valid"),
            AngularPosition::new(midheaven, 0.0).expect("fixture midheaven is valid"),
            AngularPosition::new(vertex, 0.0).expect("fixture vertex is valid"),
        )
        .expect("fixture angles are valid"),
    )
    .expect("fixture cusps are valid");
    let result = DeterministicMock::new(positions, houses)
        .calculate(&request)
        .expect("fixture calculation is valid");
    DerivedChartArtifact::new(
        CalculationArtifact::new(request, result).expect("fixture calculation artifact is valid"),
        ChartSpecification::new(options, AspectDefinitions::new(Vec::new()).unwrap()),
    )
    .expect("fixture derived chart is valid")
}

fn selection(include_all_angles: bool) -> ChartPointSelection {
    let mut points = OBJECTS
        .into_iter()
        .map(ChartPointId::from)
        .collect::<Vec<_>>();
    points.extend([ChartPointId::Ascendant, ChartPointId::Midheaven]);
    if include_all_angles {
        points.extend([
            ChartPointId::Descendant,
            ChartPointId::ImumCoeli,
            ChartPointId::Vertex,
        ]);
    }
    ChartPointSelection::new(points).expect("fixture selection is unique")
}

fn definitions() -> AspectDefinitions {
    AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Conjunction, 4.0).unwrap(),
        AspectDefinition::new(AspectKind::Sextile, 3.0).unwrap(),
        AspectDefinition::new(AspectKind::Square, 4.0).unwrap(),
        AspectDefinition::new(AspectKind::Trine, 4.0).unwrap(),
        AspectDefinition::new(AspectKind::Opposition, 4.0).unwrap(),
    ])
    .unwrap()
}

fn fixture(frame: usize) -> ComparisonArtifact {
    let natal = chart(
        "2000-01-01T00:00:00Z",
        [
            359.4, 0.2, 1.1, 45.0, 89.5, 120.0, 150.0, 180.0, 240.0, 300.0,
        ],
        [0.0; 10],
        [
            29.999_9, 61.25, 93.5, 126.75, 159.2, 190.4, 218.8, 246.1, 275.6, 302.2, 329.4, 351.8,
        ],
        29.999_9,
        302.2,
        205.5,
    );
    let (timestamp, longitudes, speeds, ascendant, midheaven) = match frame {
        1 => (
            "2026-01-01T00:00:00Z",
            [
                359.0, 0.4, 1.2, 44.0, 88.0, 119.0, 151.0, 181.0, 239.0, 301.0,
            ],
            [1.0, 13.0, -1.0, 1.2, 0.5, 0.0, -0.05, 0.02, -0.01, 0.01],
            0.0,
            90.0,
        ),
        2 => (
            "2026-01-01T12:00:00Z",
            [
                0.5, 7.0, 0.2, 44.6, 88.25, 119.05, 150.98, 181.01, 238.995, 301.005,
            ],
            [1.0, 13.0, -1.0, 1.2, 0.5, 0.2, 0.0, 0.02, -0.01, 0.01],
            1.0,
            91.0,
        ),
        3 => (
            "2026-01-03T12:00:00Z",
            [
                3.0, 33.0, 358.0, 47.0, 89.5, 119.5, 150.8, 181.1, 238.9, 301.1,
            ],
            [1.0, 13.0, -1.0, 1.2, 0.5, 0.2, 0.05, 0.02, -0.01, 0.01],
            3.5,
            93.5,
        ),
        _ => panic!("expected fixture frame 1, 2, or 3"),
    };
    let transit = chart(
        timestamp,
        longitudes,
        speeds,
        [
            ascendant,
            (ascendant + 31.0).rem_euclid(360.0),
            (ascendant + 62.5).rem_euclid(360.0),
            (ascendant + 94.0).rem_euclid(360.0),
            (ascendant + 125.0).rem_euclid(360.0),
            (ascendant + 156.0).rem_euclid(360.0),
            (ascendant + 180.0).rem_euclid(360.0),
            (ascendant + 211.0).rem_euclid(360.0),
            (ascendant + 242.5).rem_euclid(360.0),
            (ascendant + 274.0).rem_euclid(360.0),
            (ascendant + 305.0).rem_euclid(360.0),
            (ascendant + 336.0).rem_euclid(360.0),
        ],
        ascendant,
        midheaven,
        (ascendant + 180.0).rem_euclid(360.0),
    );
    ComparisonArtifact::new(
        natal,
        transit,
        ComparisonSpecification::moving_second(
            ComparisonKind::TransitToNatal,
            definitions(),
            selection(true),
            selection(false),
        )
        .unwrap(),
    )
    .expect("fixture comparison is valid")
}

fn main() {
    let frame = std::env::args()
        .nth(1)
        .expect("pass frame number 1, 2, or 3")
        .parse::<usize>()
        .expect("frame number is an integer");
    println!("{}", fixture(frame).to_json().unwrap());
}
