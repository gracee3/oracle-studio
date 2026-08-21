use std::collections::BTreeMap;

use astraeus_core::{
    ASPECT_EXACT_TOLERANCE_DEGREES, AngularPosition, Aspect, AspectDefinition, AspectDefinitions,
    AspectKind, AspectPhase, ChartPointId, ValidationError, calculate_aspects, measure_aspect,
};

fn position(longitude: f64) -> AngularPosition {
    position_with_speed(longitude, 0.0)
}

#[test]
fn provider_independent_measurement_exposes_signed_error_and_motion() {
    let measurement = measure_aspect(
        position_with_speed(359.0, 0.25),
        position_with_speed(88.0, 1.25),
        AspectKind::Square,
    );
    assert_eq!(measurement.signed_separation_degrees(), 89.0);
    assert_eq!(measurement.separation_degrees(), 89.0);
    assert_eq!(measurement.signed_aspect_error_degrees(), -1.0);
    assert_eq!(measurement.angular_error_degrees(), 1.0);
    assert_eq!(measurement.relative_speed_degrees_per_day(), 1.0);
    assert_eq!(measurement.phase(), AspectPhase::Applying);
}

fn position_with_speed(longitude: f64, speed: f64) -> AngularPosition {
    AngularPosition::new(longitude, speed).unwrap()
}

#[test]
fn detects_every_ptolemaic_aspect_at_exactitude() {
    for (kind, longitude) in [
        (AspectKind::Conjunction, 0.0),
        (AspectKind::Sextile, 60.0),
        (AspectKind::Square, 90.0),
        (AspectKind::Trine, 120.0),
        (AspectKind::Opposition, 180.0),
    ] {
        let positions = BTreeMap::from([
            (ChartPointId::Sun, position(0.0)),
            (ChartPointId::Moon, position(longitude)),
        ]);
        let definitions =
            AspectDefinitions::new(vec![AspectDefinition::new(kind, 0.0).unwrap()]).unwrap();
        let aspects = calculate_aspects(&positions, &definitions);
        assert_eq!(aspects.len(), 1);
        assert_eq!(aspects[0].kind(), kind);
        assert_eq!(aspects[0].separation_degrees(), longitude);
        assert_eq!(aspects[0].orb_degrees(), 0.0);
        assert_eq!(aspects[0].phase(), AspectPhase::Exact);
    }
}

#[test]
fn phase_tracks_motion_on_both_sides_of_an_aspect() {
    let definitions = AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
    ])
    .unwrap();
    for (longitude, speed, expected) in [
        (89.0, 1.0, AspectPhase::Applying),
        (91.0, 1.0, AspectPhase::Separating),
        (271.0, -1.0, AspectPhase::Applying),
        (269.0, -1.0, AspectPhase::Separating),
    ] {
        let positions = BTreeMap::from([
            (ChartPointId::Sun, position(0.0)),
            (ChartPointId::Moon, position_with_speed(longitude, speed)),
        ]);
        assert_eq!(
            calculate_aspects(&positions, &definitions)[0].phase(),
            expected
        );
    }
}

#[test]
fn conjunction_and_opposition_wraparound_preserve_phase() {
    for (kind, first, second, speed) in [
        (AspectKind::Conjunction, 359.0, 1.0, -1.0),
        (AspectKind::Conjunction, 1.0, 359.0, 1.0),
        (AspectKind::Opposition, 0.0, 179.0, 1.0),
        (AspectKind::Opposition, 0.0, 181.0, -1.0),
    ] {
        let definitions =
            AspectDefinitions::new(vec![AspectDefinition::new(kind, 2.0).unwrap()]).unwrap();
        let positions = BTreeMap::from([
            (ChartPointId::Sun, position(first)),
            (ChartPointId::Moon, position_with_speed(second, speed)),
        ]);
        assert_eq!(
            calculate_aspects(&positions, &definitions)[0].phase(),
            AspectPhase::Applying
        );
    }
}

#[test]
fn exactitude_precedes_stationary_classification() {
    let definitions = AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
    ])
    .unwrap();
    for (longitude, expected) in [
        (89.0, AspectPhase::Stationary),
        (
            90.0 + ASPECT_EXACT_TOLERANCE_DEGREES / 2.0,
            AspectPhase::Exact,
        ),
    ] {
        let positions = BTreeMap::from([
            (ChartPointId::Sun, position_with_speed(0.0, 1.0)),
            (ChartPointId::Moon, position_with_speed(longitude, 1.0)),
        ]);
        assert_eq!(
            calculate_aspects(&positions, &definitions)[0].phase(),
            expected
        );
    }
}

#[test]
fn detects_wraparound_and_inclusive_orbs() {
    let positions = BTreeMap::from([
        (ChartPointId::Sun, position(358.0)),
        (ChartPointId::Moon, position(2.0)),
        (ChartPointId::Mars, position(92.0)),
    ]);
    let definitions = AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Conjunction, 4.0).unwrap(),
        AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
    ])
    .unwrap();

    let aspects = calculate_aspects(&positions, &definitions);
    assert_eq!(aspects.len(), 2);
    assert_eq!(aspects[0].kind(), AspectKind::Conjunction);
    assert_eq!(aspects[0].separation_degrees(), 4.0);
    assert_eq!(aspects[0].orb_degrees(), 4.0);
    assert_eq!(aspects[1].kind(), AspectKind::Square);
    assert_eq!(aspects[1].orb_degrees(), 0.0);
}

#[test]
fn closest_aspect_wins_when_windows_overlap() {
    let positions = BTreeMap::from([
        (ChartPointId::Sun, position(0.0)),
        (ChartPointId::Moon, position(80.0)),
    ]);
    let definitions = AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Sextile, 25.0).unwrap(),
        AspectDefinition::new(AspectKind::Square, 25.0).unwrap(),
    ])
    .unwrap();

    let aspects = calculate_aspects(&positions, &definitions);
    assert_eq!(aspects[0].kind(), AspectKind::Square);
    assert_eq!(aspects[0].orb_degrees(), 10.0);
}

#[test]
fn definition_order_does_not_change_tie_breaking() {
    let positions = BTreeMap::from([
        (ChartPointId::Sun, position(0.0)),
        (ChartPointId::Moon, position(75.0)),
    ]);
    for kinds in [
        [AspectKind::Sextile, AspectKind::Square],
        [AspectKind::Square, AspectKind::Sextile],
    ] {
        let definitions = AspectDefinitions::new(
            kinds
                .into_iter()
                .map(|kind| AspectDefinition::new(kind, 15.0).unwrap())
                .collect(),
        )
        .unwrap();
        assert_eq!(
            calculate_aspects(&positions, &definitions)[0].kind(),
            AspectKind::Sextile
        );
    }
}

#[test]
fn no_match_is_empty_and_multiple_pairs_are_canonically_ordered() {
    let definitions = AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Conjunction, 1.0).unwrap(),
    ])
    .unwrap();
    let no_match = BTreeMap::from([
        (ChartPointId::Sun, position(0.0)),
        (ChartPointId::Moon, position(10.0)),
    ]);
    assert!(calculate_aspects(&no_match, &definitions).is_empty());

    let positions = BTreeMap::from([
        (ChartPointId::Mars, position(0.0)),
        (ChartPointId::Sun, position(0.0)),
        (ChartPointId::Moon, position(0.0)),
    ]);
    let aspects = calculate_aspects(&positions, &definitions);
    assert_eq!(aspects.len(), 3);
    assert_eq!(
        aspects
            .iter()
            .map(|aspect| (aspect.first(), aspect.second()))
            .collect::<Vec<_>>(),
        vec![
            (ChartPointId::Sun, ChartPointId::Moon),
            (ChartPointId::Sun, ChartPointId::Mars),
            (ChartPointId::Moon, ChartPointId::Mars),
        ]
    );
}

#[test]
fn definitions_reject_invalid_orbs_and_duplicates() {
    assert!(matches!(
        AspectDefinition::new(AspectKind::Trine, f64::NAN),
        Err(ValidationError::InvalidAspectOrb(_))
    ));
    let conjunction = AspectDefinition::new(AspectKind::Conjunction, 8.0).unwrap();
    assert_eq!(
        AspectDefinitions::new(vec![conjunction, conjunction]).unwrap_err(),
        ValidationError::DuplicateAspect(AspectKind::Conjunction)
    );
}

#[test]
fn json_cannot_bypass_definition_validation() {
    assert!(
        serde_json::from_str::<AspectDefinition>(r#"{"kind":"square","orb_degrees":-1.0}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<AspectDefinitions>(
            r#"[{"kind":"square","orb_degrees":3.0},{"kind":"square","orb_degrees":4.0}]"#
        )
        .is_err()
    );

    for invalid in [
        r#"{"first":"moon","second":"sun","kind":"square","separation_degrees":90.0,"orb_degrees":0.0}"#,
        r#"{"first":"sun","second":"moon","kind":"square","separation_degrees":181.0,"orb_degrees":91.0}"#,
        r#"{"first":"sun","second":"moon","kind":"square","separation_degrees":91.0,"orb_degrees":2.0}"#,
        r#"{"first":"sun","second":"moon","kind":"square","separation_degrees":90.0,"orb_degrees":0.0,"extra":true}"#,
    ] {
        assert!(serde_json::from_str::<Aspect>(invalid).is_err());
    }

    let definitions = AspectDefinitions::new(vec![
        AspectDefinition::new(AspectKind::Square, 2.0).unwrap(),
    ])
    .unwrap();
    let positions = BTreeMap::from([
        (ChartPointId::Sun, position(0.0)),
        (ChartPointId::Moon, position_with_speed(89.0, 1.0)),
    ]);
    let aspect = calculate_aspects(&positions, &definitions)[0];
    let json = serde_json::to_string(&aspect).unwrap();
    assert_eq!(serde_json::from_str::<Aspect>(&json).unwrap(), aspect);
    assert!(
        serde_json::from_str::<Aspect>(&json.replacen("\"applying\"", "\"separating\"", 1))
            .is_err()
    );
}
