use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, CalculationRequest, CelestialObject, ChartAngles, DeterministicMock,
    EphemerisAdapter, GeographicLocation, HouseCusps, HouseSystem, Position,
    UtcInstant as AstraeusInstant, Zodiac,
};
use oracle_studio_app::{
    ChartCalculationRequest, ComparisonCalculationRequest, ManualPlacementInput, ReadingRequest,
    SearchEntity, StudioService,
};
use oracle_studio_core::{
    ArtifactKind, ChartCalculationOptions, ChartDefinition, ChartPointId, ChartRole,
    ComparisonPreset, JournalEntry, JournalEntryKind, LocalDateTimeInput, LocationProvenance,
    PersonKind, PersonProfile, SavedLocation, Session, StableId, VaultDocument, WheelOrientation,
    WorkspaceState, default_aspects, default_chart_points,
};
use sibylla_artifacts::{Artifact, ReadingArtifact};
use sibylla_core::{Orientation, RandomnessSource, SpreadDefinition, UtcInstant};

const DECK: &str = r#"{
  "schema_version": 1,
  "id": "fictional_workflow_deck",
  "name": "Fictional Workflow Deck",
  "attribution": {"author": "Oracle Studio contributors", "artist": null, "publisher": null},
  "tradition": "Original metadata-only workflow fixture",
  "rights": {"license": "AGPL-3.0-or-later", "source": null, "notes": "No artwork."},
  "reversal_rate_basis_points": 5000,
  "cards": [
    {"id":"fool","identity":{"kind":"conventional","id":"fool"},"printed_title":"The Fool","printed_number":null,"printed_suit":null,"printed_rank":null,"enabled":true,"asset_id":null,"correspondences":[],"notes":null},
    {"id":"magician","identity":{"kind":"conventional","id":"magician"},"printed_title":"The Magician","printed_number":null,"printed_suit":null,"printed_rank":null,"enabled":true,"asset_id":null,"correspondences":[],"notes":null},
    {"id":"star","identity":{"kind":"conventional","id":"star"},"printed_title":"The Star","printed_number":null,"printed_suit":null,"printed_rank":null,"enabled":true,"asset_id":null,"correspondences":[],"notes":null}
  ]
}"#;

fn id(field: &'static str, value: &str) -> StableId {
    StableId::new(field, value).unwrap()
}

fn composed() -> VaultDocument {
    let person = PersonProfile::new(
        id("person.id", "fictional_client"),
        "Fictional Client",
        PersonKind::ProfessionalClient,
        Some("Interested in creative work.".into()),
    )
    .unwrap();
    let session = Session::new(
        id("session.id", "fictional_session"),
        Some(person.id().clone()),
        "Creative Direction",
        Some("A fictional session context.".into()),
        "2026-07-21T14:00:00Z",
        "2026-07-21T14:00:00Z",
    )
    .unwrap();
    VaultDocument::new(vec![person], vec![session], vec![], vec![]).unwrap()
}

fn with_deck() -> VaultDocument {
    StudioService::import_deck(&composed(), id("artifact.id", "deck_record"), DECK).unwrap()
}

fn with_bound_deck() -> VaultDocument {
    let document = with_deck();
    let mut artifacts = document.artifacts().to_vec();
    let content_id = artifacts[0].content_id().to_owned();
    artifacts[0]
        .bind_deck_pack(id("deck_pack.id", "creative_pack"), content_id)
        .unwrap();
    VaultDocument::new(
        document.people().to_vec(),
        document.sessions().to_vec(),
        artifacts,
        document.journal_entries().to_vec(),
    )
    .unwrap()
}

fn request(record: &str, reading: &str) -> ReadingRequest {
    ReadingRequest {
        artifact_record_id: id("artifact.id", record),
        reading_id: reading.into(),
        deck_record_id: id("artifact.id", "deck_record"),
        person_id: Some(id("person.id", "fictional_client")),
        session_id: Some(id("session.id", "fictional_session")),
        spread: SpreadDefinition::one_card(),
        question: Some("What supports the fictional creative work?".into()),
        context: Some("A test-only context.".into()),
        reader_notes: Some("A test-only note.".into()),
        interpretation: None,
        timestamp: UtcInstant::parse_rfc3339("2026-07-21T14:00:00Z").unwrap(),
    }
}

#[test]
fn raw_decks_and_manual_readings_become_validated_immutable_artifacts() {
    let document = StudioService::record_manual_reading(
        &with_deck(),
        request("manual_record", "manual_reading"),
        vec![ManualPlacementInput {
            deck_card_id: "fool".into(),
            orientation: Orientation::Unspecified,
            notes: Some("Entered from a fictional physical layout.".into()),
        }],
    )
    .unwrap();

    assert_eq!(document.artifacts().len(), 2);
    let record = &document.artifacts()[1];
    assert_eq!(record.kind(), ArtifactKind::SibyllaReading);
    let reading = ReadingArtifact::from_json(record.canonical_json()).unwrap();
    assert_eq!(
        reading.payload().placements()[0].deck_card_id().as_str(),
        "fool"
    );
    assert_eq!(
        reading.payload().placements()[0].orientation(),
        Orientation::Unspecified
    );
    assert_eq!(reading.payload().subject_ref(), Some("fictional_client"));
}

#[test]
fn search_exposes_bound_deck_pack_provenance() {
    let hits = StudioService::search(&with_bound_deck(), "creative_pack").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entity(), SearchEntity::Artifact);
    assert!(hits[0].snippet().contains("pack=creative_pack"));
}

#[test]
fn software_readings_use_the_os_random_production_entrypoint() {
    let document = StudioService::record_software_reading(
        &with_deck(),
        request("software_record", "software_reading"),
    )
    .unwrap();
    let reading = match Artifact::from_json(document.artifacts()[1].canonical_json()).unwrap() {
        Artifact::Reading(reading) => reading,
        Artifact::Deck(_) => panic!("expected reading"),
    };
    assert!(matches!(
        reading.payload().draw_provenance(),
        sibylla_core::DrawProvenance::SoftwareShuffle {
            randomness_source: RandomnessSource::OperatingSystem,
            ..
        }
    ));
    assert_eq!(reading.payload().placements().len(), 1);
}

#[test]
fn source_linked_annotations_are_searchable_only_in_memory() {
    let document = StudioService::add_journal_entry(
        &with_deck(),
        JournalEntry::new(
            id("journal_entry.id", "creative_observation"),
            Some(id("person.id", "fictional_client")),
            Some(id("session.id", "fictional_session")),
            Some(id("artifact.id", "deck_record")),
            JournalEntryKind::Annotation,
            "A recurring fictional creative theme.",
            "2026-07-21T15:00:00Z",
        )
        .unwrap(),
    )
    .unwrap();

    let hits = StudioService::search(&document, "creative").unwrap();
    assert!(hits.iter().any(|hit| hit.entity() == SearchEntity::Person));
    assert!(hits.iter().any(|hit| hit.entity() == SearchEntity::Session));
    assert!(
        hits.iter()
            .any(|hit| hit.entity() == SearchEntity::JournalEntry)
    );
}

#[test]
fn calculation_artifacts_are_validated_and_associated_without_recalculation() {
    // Original synthetic fixture following the pinned Astraeus public API.
    let request = CalculationRequest::new(
        AstraeusInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(51.4779, 0.0, 46.0).unwrap(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    let positions = BTreeMap::from([(
        CelestialObject::Sun,
        Position::new(280.3689197, 0.0002323, 0.983327645, 1.0194321).unwrap(),
    )]);
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
    let result = DeterministicMock::new(positions, houses)
        .calculate(&request)
        .unwrap();
    let calculated = StudioService::calculate_chart(
        &composed(),
        id("artifact.id", "calculated_chart_record"),
        Some(id("person.id", "fictional_client")),
        Some(id("session.id", "fictional_session")),
        request.clone(),
    )
    .unwrap();
    assert_eq!(
        calculated.artifacts()[0].kind(),
        ArtifactKind::AstraeusCalculation
    );
    let json = CalculationArtifact::new(request, result)
        .unwrap()
        .to_json()
        .unwrap();
    let document = StudioService::import_chart(
        &composed(),
        id("artifact.id", "chart_record"),
        Some(id("person.id", "fictional_client")),
        Some(id("session.id", "fictional_session")),
        &json,
    )
    .unwrap();
    assert_eq!(
        document.artifacts()[0].kind(),
        ArtifactKind::AstraeusCalculation
    );
}

fn fictional_location(label: &str, latitude: f64) -> SavedLocation {
    SavedLocation::new(
        id("saved_location.id", "fictional_city"),
        label,
        vec!["Example County".into(), "New York".into()],
        "US",
        latitude,
        -73.7562,
        Some(84.0),
        "America/New_York",
        LocationProvenance::Manual,
    )
    .unwrap()
}

fn fictional_chart(
    chart_id: &str,
    label: &str,
    role: ChartRole,
    date: &str,
    time: &str,
    default_natal: bool,
) -> ChartDefinition {
    ChartDefinition::new(
        id("chart_definition.id", chart_id),
        label,
        role,
        (role == ChartRole::Natal).then(|| id("person.id", "fictional_client")),
        LocalDateTimeInput::new(date, time, "America/New_York").unwrap(),
        ChartCalculationOptions::default(),
        default_chart_points(),
        default_natal,
    )
    .unwrap()
}

#[test]
fn schema_v3_chart_history_and_comparison_sources_are_immutable_and_canonical() {
    let document = StudioService::add_saved_location(
        &composed(),
        fictional_location("Fictional Albany", 42.6526),
    )
    .unwrap();
    let document = StudioService::add_chart_definition(
        &document,
        fictional_chart(
            "fictional_natal",
            "Fictional natal",
            ChartRole::Natal,
            "1990-05-12",
            "08:30",
            true,
        ),
    )
    .unwrap();
    let document = StudioService::add_chart_definition(
        &document,
        fictional_chart(
            "fictional_transit",
            "Fictional transit",
            ChartRole::Transit,
            "2026-08-18",
            "14:15",
            false,
        ),
    )
    .unwrap();
    let document = StudioService::calculate_chart_definition(
        &document,
        ChartCalculationRequest {
            chart_calculation_id: id("chart_calculation.id", "natal_calculation_1"),
            calculation_artifact_id: id("artifact.id", "natal_artifact_1"),
            chart_definition_id: id("chart_definition.id", "fictional_natal"),
            saved_location_id: id("saved_location.id", "fictional_city"),
            ambiguous_time_choice: None,
            calculated_at: "2026-08-18T18:00:00Z".into(),
        },
    )
    .unwrap();
    let first_snapshot = document.chart_calculations()[0].clone();

    let document = StudioService::replace_saved_location(
        &document,
        fictional_location("Fictional Albany, edited", 42.7000),
    )
    .unwrap();
    assert_eq!(
        first_snapshot.location_snapshot().label(),
        "Fictional Albany"
    );
    assert_eq!(
        document.chart_calculations()[0].location_snapshot(),
        first_snapshot.location_snapshot()
    );

    let document = StudioService::calculate_chart_definition(
        &document,
        ChartCalculationRequest {
            chart_calculation_id: id("chart_calculation.id", "natal_calculation_2"),
            calculation_artifact_id: id("artifact.id", "natal_artifact_2"),
            chart_definition_id: id("chart_definition.id", "fictional_natal"),
            saved_location_id: id("saved_location.id", "fictional_city"),
            ambiguous_time_choice: None,
            calculated_at: "2026-08-18T18:01:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(document.chart_calculations().len(), 2);
    assert_eq!(document.chart_calculations()[0], first_snapshot);
    assert_eq!(
        document.chart_definitions()[0]
            .current_calculation_id()
            .unwrap()
            .as_str(),
        "natal_calculation_2"
    );

    let document = StudioService::calculate_chart_definition(
        &document,
        ChartCalculationRequest {
            chart_calculation_id: id("chart_calculation.id", "transit_calculation_1"),
            calculation_artifact_id: id("artifact.id", "transit_artifact_1"),
            chart_definition_id: id("chart_definition.id", "fictional_transit"),
            saved_location_id: id("saved_location.id", "fictional_city"),
            ambiguous_time_choice: None,
            calculated_at: "2026-08-18T18:02:00Z".into(),
        },
    )
    .unwrap();
    let preset = ComparisonPreset::new(
        id("comparison_preset.id", "fictional_biwheel"),
        "Fictional biwheel",
        id("chart_definition.id", "fictional_natal"),
        id("chart_definition.id", "fictional_transit"),
        vec![
            ChartPointId::Moon,
            ChartPointId::Sun,
            ChartPointId::Ascendant,
        ],
        vec![
            ChartPointId::Moon,
            ChartPointId::Sun,
            ChartPointId::Ascendant,
        ],
        default_aspects(),
        WheelOrientation::AscendantLeft,
    )
    .unwrap();
    let document = StudioService::add_comparison_preset(&document, preset).unwrap();
    let document = StudioService::calculate_comparison(
        &document,
        ComparisonCalculationRequest {
            comparison_artifact_id: id("artifact.id", "comparison_artifact_1"),
            comparison_preset_id: id("comparison_preset.id", "fictional_biwheel"),
        },
    )
    .unwrap();
    let comparison = &document.comparison_presets()[0];
    assert_eq!(
        comparison.current_inner_calculation_id().unwrap().as_str(),
        "natal_calculation_2"
    );
    assert_eq!(
        comparison.current_outer_calculation_id().unwrap().as_str(),
        "transit_calculation_1"
    );
    assert_eq!(
        document.artifacts().last().unwrap().kind(),
        ArtifactKind::AstraeusComparison
    );

    let document = StudioService::set_workspace_state(
        &document,
        WorkspaceState::new(
            Some(id("person.id", "fictional_client")),
            Some(id("comparison_preset.id", "fictional_biwheel")),
        ),
    )
    .unwrap();
    let reopened = VaultDocument::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(reopened, document);
}

#[test]
fn schema_v3_enforces_one_default_natal_per_person() {
    let first = StudioService::add_chart_definition(
        &composed(),
        fictional_chart(
            "first_default",
            "First default",
            ChartRole::Natal,
            "1990-01-01",
            "12:00",
            true,
        ),
    )
    .unwrap();
    assert!(
        StudioService::add_chart_definition(
            &first,
            fictional_chart(
                "second_default",
                "Second default",
                ChartRole::Natal,
                "1991-01-01",
                "12:00",
                true,
            ),
        )
        .is_err()
    );
}
