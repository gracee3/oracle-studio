use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinitions, CalculationError, CalculationOptions,
    CalculationProvenance, CalculationRequest, CalculationResult, CelestialObject, ChartAngles,
    EphemerisAdapter, EphemerisSource, GeographicLocation, HouseCusps, HouseSystem, Position,
    UtcInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_events::{
    EventDefinition, EventPositionProvider, EventPositionRequest, EventPositionSample, EventSearch,
    EventSelection, ReturnFrame, SeasonalPoint, solve_event, solve_return,
};
use astraeus_specifications::ChartSpecification;

struct LinearAdapter;

impl EphemerisAdapter for LinearAdapter {
    fn calculate(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        let epoch = UtcInstant::parse_rfc3339("2000-01-01T00:00:00Z").unwrap();
        let days = (request.instant().as_datetime() - epoch.as_datetime()).num_milliseconds()
            as f64
            / 86_400_000.0;
        let positions = request
            .objects()
            .iter()
            .map(|object| {
                let longitude = match object {
                    CelestialObject::Sun => days.rem_euclid(360.0),
                    CelestialObject::Moon => (13.0 * days).rem_euclid(360.0),
                    _ => days.rem_euclid(360.0),
                };
                (*object, Position::new(longitude, 0.0, 1.0, 1.0).unwrap())
            })
            .collect::<BTreeMap<_, _>>();
        let houses = HouseCusps::new(
            (0..12).map(|i| f64::from(i) * 30.0).collect(),
            ChartAngles::new(
                AngularPosition::new(0.0, 1.0).unwrap(),
                AngularPosition::new(270.0, 1.0).unwrap(),
                AngularPosition::new(180.0, 1.0).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        CalculationResult::new(
            request,
            positions,
            houses,
            CalculationProvenance::new("linear", "1", EphemerisSource::Synthetic, None).unwrap(),
        )
    }
}

impl EventPositionProvider for LinearAdapter {
    fn sample_event_positions(
        &self,
        request: &EventPositionRequest,
    ) -> Result<EventPositionSample, astraeus_events::EventError> {
        let epoch = UtcInstant::parse_rfc3339("2000-01-01T00:00:00Z").unwrap();
        let days = (request.instant().as_datetime() - epoch.as_datetime()).num_milliseconds()
            as f64
            / 86_400_000.0;
        let positions = request
            .objects()
            .iter()
            .map(|object| {
                let (longitude, speed) = match object {
                    CelestialObject::Moon => ((13.0 * days).rem_euclid(360.0), 13.0),
                    _ => (days.rem_euclid(360.0), 1.0),
                };
                (*object, AngularPosition::new(longitude, speed).unwrap())
            })
            .collect();
        EventPositionSample::new(
            request.clone(),
            positions,
            CalculationProvenance::new("linear", "1", EphemerisSource::Synthetic, None).unwrap(),
        )
    }
}

fn specification() -> ChartSpecification {
    ChartSpecification::new(
        CalculationOptions::new(
            vec![CelestialObject::Sun, CelestialObject::Moon],
            Zodiac::Tropical,
            None,
            HouseSystem::WholeSign,
        )
        .unwrap(),
        AspectDefinitions::new(vec![]).unwrap(),
    )
}

fn natal() -> DerivedChartArtifact {
    let specification = specification();
    let request = specification.request(
        UtcInstant::parse_rfc3339("2000-01-01T00:00:00Z").unwrap(),
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
    );
    let result = LinearAdapter.calculate(&request).unwrap();
    DerivedChartArtifact::new(
        CalculationArtifact::new(request, result).unwrap(),
        specification,
    )
    .unwrap()
}

#[test]
fn previous_selects_the_latest_prior_root() {
    let artifact = solve_return(
        &LinearAdapter,
        &specification(),
        GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
        &natal(),
        CelestialObject::Sun,
        ReturnFrame::ConfiguredZodiac,
        EventSearch {
            reference: UtcInstant::parse_rfc3339("2000-06-29T00:00:00Z").unwrap(),
            selection: EventSelection::Previous,
            window_days: 200.0,
            scan_step_hours: 24.0,
        },
    )
    .unwrap();
    let expected = UtcInstant::parse_rfc3339("2000-01-01T00:00:00Z").unwrap();
    assert!(
        (artifact.exact_instant().as_datetime() - expected.as_datetime())
            .num_seconds()
            .abs()
            <= 1
    );
    assert!(artifact.residual_degrees() <= 1e-5);
    assert!(artifact.content_id().unwrap().starts_with("sha256:"));
    let json = artifact.to_json().unwrap();
    assert_eq!(
        astraeus_events::EventChartArtifact::from_json(&json).unwrap(),
        artifact
    );
    let mut tampered: serde_json::Value = serde_json::from_str(&json).unwrap();
    tampered["target_longitude_degrees"] = serde_json::json!(1.0);
    assert!(astraeus_events::EventChartArtifact::from_json(&tampered.to_string()).is_err());
}

#[test]
fn seasonal_solver_casts_chart_at_exact_instant() {
    let artifact = solve_event(
        &LinearAdapter,
        &specification(),
        GeographicLocation::new(10.0, 20.0, 0.0).unwrap(),
        EventDefinition::Seasonal {
            point: SeasonalPoint::JuneSolstice,
        },
        EventSearch {
            reference: UtcInstant::parse_rfc3339("2000-03-01T00:00:00Z").unwrap(),
            selection: EventSelection::Next,
            window_days: 100.0,
            scan_step_hours: 24.0,
        },
    )
    .unwrap();
    let expected = UtcInstant::parse_rfc3339("2000-03-31T00:00:00Z").unwrap();
    assert!(
        (artifact.exact_instant().as_datetime() - expected.as_datetime())
            .num_seconds()
            .abs()
            <= 1
    );
    assert_eq!(
        artifact.chart().calculation().request().instant(),
        artifact.exact_instant()
    );
}
