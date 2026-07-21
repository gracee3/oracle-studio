use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, CalculationRequest, CelestialObject, ChartAngles, DeterministicMock,
    EphemerisAdapter, GeographicLocation, HouseCusps, HouseSystem, Position,
    UtcInstant as AstraeusInstant, Zodiac,
};
use oracle_studio_app::{ManualPlacementInput, ReadingRequest, SearchEntity, StudioService};
use oracle_studio_core::{
    ArtifactKind, JournalEntry, JournalEntryKind, PersonKind, PersonProfile, Session, StableId,
    VaultDocument,
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
