use astraeus_core::{AspectKind, ChartPointId};
use oracle_studio_aspect_sets::{
    AspectSet, AspectSetError, AspectSetSettings, MAX_IMPORT_BYTES, STANDARD_ID, builtins,
};

fn rule(set: &AspectSet, kind: AspectKind) -> (f64, f64, f64, f64) {
    let orbs = set
        .rules()
        .iter()
        .find(|rule| rule.kind() == kind)
        .unwrap()
        .orbs();
    (
        orbs.luminary_applying_degrees(),
        orbs.luminary_separating_degrees(),
        orbs.other_applying_degrees(),
        orbs.other_separating_degrees(),
    )
}

#[test]
fn builtins_match_the_reviewed_orb_and_point_contract() {
    let sets = builtins();
    assert_eq!(sets.len(), 4);
    assert_eq!(sets[0].id(), "builtin.tight");
    assert_eq!(sets[1].id(), STANDARD_ID);
    assert_eq!(sets[2].id(), "builtin.synastry");
    assert_eq!(sets[3].id(), "builtin.synwide");
    for set in &sets {
        set.validate().unwrap();
        assert_eq!(set.revision(), 1);
        assert!(set.built_in());
        assert!(set.content_id().starts_with("sha256:"));
        assert_eq!(set.rules().len(), 5);
    }

    assert_eq!(
        rule(&sets[0], AspectKind::Conjunction),
        (2.0, 2.0, 1.0, 1.0)
    );
    assert_eq!(rule(&sets[0], AspectKind::Sextile), (1.5, 1.5, 1.0, 1.0));
    assert_eq!(rule(&sets[1], AspectKind::Square), (6.0, 6.0, 6.0, 6.0));
    assert_eq!(rule(&sets[1], AspectKind::Sextile), (4.0, 4.0, 4.0, 4.0));
    assert_eq!(
        rule(&sets[2], AspectKind::Opposition),
        (10.0, 8.0, 8.0, 6.0)
    );
    assert_eq!(rule(&sets[2], AspectKind::Trine), (8.0, 6.0, 6.0, 5.0));
    assert_eq!(rule(&sets[2], AspectKind::Sextile), (6.0, 5.0, 4.0, 3.0));
    assert_eq!(
        rule(&sets[3], AspectKind::Conjunction),
        (12.0, 10.0, 10.0, 8.0)
    );
    assert_eq!(rule(&sets[3], AspectKind::Square), (10.0, 8.0, 8.0, 6.0));
    assert_eq!(rule(&sets[3], AspectKind::Sextile), (8.0, 6.0, 6.0, 4.0));

    assert_eq!(sets[0].points().len(), 12);
    assert_eq!(sets[1].points().len(), 12);
    assert_eq!(sets[2].points().len(), 19);
    assert_eq!(sets[3].points().len(), 19);
    assert!(sets[2].points().contains(&ChartPointId::Vertex));
    assert!(!sets[2].points().contains(&ChartPointId::Chiron));
}

#[test]
fn user_workflows_preserve_builtins_and_never_replace_on_import() {
    let mut settings = AspectSetSettings::default();
    settings
        .duplicate_selected("user.my-standard", "My Standard")
        .unwrap();
    assert_eq!(settings.selected_aspect_set_id(), "user.my-standard");
    assert_eq!(settings.selected().revision(), 1);
    settings.rename_selected("Renamed Standard").unwrap();
    assert_eq!(settings.selected().revision(), 2);
    assert_eq!(settings.selected().name(), "Renamed Standard");

    let exported = settings.selected().to_pretty_json().unwrap();
    assert!(matches!(
        settings.import(&exported),
        Err(AspectSetError::DuplicateId(_))
    ));
    settings.delete_selected().unwrap();
    assert_eq!(settings.selected_aspect_set_id(), STANDARD_ID);
    assert!(matches!(
        settings.delete_selected(),
        Err(AspectSetError::ImmutableBuiltin(_))
    ));

    settings.duplicate_selected("user.keep", "Keep Me").unwrap();
    settings.select("builtin.tight").unwrap();
    settings.reset_builtins().unwrap();
    assert_eq!(settings.selected_aspect_set_id(), STANDARD_ID);
    assert!(settings.sets().iter().any(|set| set.id() == "user.keep"));
    settings.validate().unwrap();
}

#[test]
fn imports_are_bounded_strict_complete_and_reserved_safe() {
    assert!(matches!(
        AspectSet::from_json(&vec![b' '; MAX_IMPORT_BYTES + 1]),
        Err(AspectSetError::ImportTooLarge(_))
    ));
    let builtin = AspectSetSettings::default()
        .selected()
        .to_pretty_json()
        .unwrap();
    assert!(matches!(
        AspectSet::from_json(&builtin),
        Err(AspectSetError::ReservedId(_))
    ));

    let mut settings = AspectSetSettings::default();
    settings
        .duplicate_selected("user.importable", "Importable")
        .unwrap();
    let exported = String::from_utf8(settings.selected().to_pretty_json().unwrap()).unwrap();
    let unknown = exported.replacen("{", "{\"unknown\":true,", 1);
    assert!(matches!(
        AspectSet::from_json(unknown.as_bytes()),
        Err(AspectSetError::Json(_))
    ));
    let incomplete = exported.replacen(
        ",\n    {\n      \"kind\": \"sextile\"",
        "\n    {\n      \"kind\": \"sextile\"",
        1,
    );
    assert!(AspectSet::from_json(incomplete.as_bytes()).is_err());
    let excessive = exported.replacen(
        "\"luminary_applying_degrees\": 8.0",
        "\"luminary_applying_degrees\": 30.5",
        1,
    );
    assert!(matches!(
        AspectSet::from_json(excessive.as_bytes()),
        Err(AspectSetError::InvalidOrb(_)) | Err(AspectSetError::ContentIdMismatch { .. })
    ));
}

#[test]
fn snapshots_are_strict_and_keep_rules_points_and_identity() {
    let set = AspectSetSettings::default().selected().clone();
    let snapshot = set.snapshot();
    assert_eq!(snapshot.aspect_set_id(), STANDARD_ID);
    assert_eq!(snapshot.revision(), set.revision());
    assert_eq!(snapshot.content_id(), set.content_id());
    assert_eq!(snapshot.rules(), set.rules());
    assert_eq!(snapshot.points(), set.points());
    assert_eq!(
        snapshot.phase_aware_definitions().unwrap().as_slice().len(),
        5
    );
    assert_eq!(
        snapshot
            .legacy_uniform_definitions()
            .unwrap()
            .as_slice()
            .len(),
        5
    );
    assert_eq!(snapshot.point_selection().unwrap().as_slice(), set.points());

    let json = serde_json::to_string(&snapshot).unwrap();
    let decoded = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot, decoded);
    assert!(
        serde_json::from_str::<oracle_studio_aspect_sets::AspectSetSnapshot>(&json.replacen(
            "{",
            "{\"unknown\":true,",
            1
        ))
        .is_err()
    );
}

#[test]
fn legacy_uniform_snapshot_preserves_asymmetric_selections_and_disabled_rules() {
    let definitions = astraeus_core::AspectDefinitions::new(vec![
        astraeus_core::AspectDefinition::new(AspectKind::Square, 2.5).unwrap(),
    ])
    .unwrap();
    let first =
        astraeus_core::ChartPointSelection::new(vec![ChartPointId::Moon, ChartPointId::Sun])
            .unwrap();
    let second =
        astraeus_core::ChartPointSelection::new(vec![ChartPointId::Mars, ChartPointId::Sun])
            .unwrap();
    let snapshot =
        oracle_studio_aspect_sets::AspectSetSnapshot::legacy_uniform(&definitions, &first, &second)
            .unwrap();
    assert_eq!(
        snapshot.points(),
        &[ChartPointId::Moon, ChartPointId::Sun, ChartPointId::Mars]
    );
    assert_eq!(
        snapshot
            .rules()
            .iter()
            .filter(|rule| rule.enabled())
            .count(),
        1
    );
    assert_eq!(
        snapshot.legacy_uniform_definitions().unwrap().as_slice(),
        definitions.as_slice()
    );
}

#[test]
fn tracked_json_schema_is_machine_readable_and_matches_wire_v1() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/aspect-set-v1.schema.json")).unwrap();
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(schema["properties"]["rules"]["minItems"], 5);
    assert_eq!(schema["properties"]["rules"]["maxItems"], 5);
    assert_eq!(
        schema["$defs"]["rule"]["properties"]["orbs"]["properties"]["other_separating_degrees"]["maximum"],
        30
    );

    let exported: serde_json::Value =
        serde_json::from_slice(&builtins()[1].to_pretty_json().unwrap()).unwrap();
    for required in schema["required"].as_array().unwrap() {
        assert!(exported.get(required.as_str().unwrap()).is_some());
    }
    assert_eq!(exported["rules"].as_array().unwrap().len(), 5);
}
