use oracle_studio_core::{
    ArtifactKind, ArtifactRecord, ModelError, PersonKind, PersonProfile, Session, StableId,
    VaultDocument,
};

const SIBYLLA_DECK_ARTIFACT: &str = r#"{
  "schema_version": 1,
  "artifact_type": "deck",
  "payload": {
    "schema_version": 1,
    "id": "fictional_minimal_deck",
    "name": "Fictional Minimal Deck",
    "attribution": {"author": "Oracle Studio contributors", "artist": null, "publisher": null},
    "tradition": "Original metadata-only test fixture",
    "rights": {"license": "AGPL-3.0-or-later", "source": null, "notes": "No artwork."},
    "reversal_rate_basis_points": 0,
    "cards": [{
      "id": "fool",
      "identity": {"kind": "conventional", "id": "fool"},
      "printed_title": "The Fool",
      "printed_number": null,
      "printed_suit": null,
      "printed_rank": null,
      "enabled": true,
      "asset_id": null,
      "correspondences": [],
      "notes": null
    }]
  }
}"#;

fn profile() -> PersonProfile {
    PersonProfile::new(
        StableId::new("person.id", "fictional_client").unwrap(),
        "Fictional Client",
        PersonKind::ProfessionalClient,
        None,
    )
    .unwrap()
}

#[test]
fn fictional_composition_document_round_trips() {
    let person = profile();
    let session = Session::new(
        StableId::new("session.id", "first_session").unwrap(),
        Some(person.id().clone()),
        "First Session",
        Some("Fictional context only.".into()),
        "2026-07-21T10:00:00-04:00",
        "2026-07-21T14:30:00Z",
    )
    .unwrap();
    let document = VaultDocument::new(vec![person], vec![session], vec![]).unwrap();
    let reopened = VaultDocument::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(reopened, document);
    assert!(document.to_json().unwrap().contains("2026-07-21T14:00:00Z"));
}

#[test]
fn identifiers_text_timeline_and_references_are_validated() {
    assert!(StableId::new("id", "Display Name").is_err());
    assert!(
        PersonProfile::new(
            StableId::new("id", "person").unwrap(),
            " ",
            PersonKind::Personal,
            None,
        )
        .is_err()
    );
    assert!(matches!(
        Session::new(
            StableId::new("id", "session").unwrap(),
            None,
            "Session",
            None,
            "2026-07-22T00:00:00Z",
            "2026-07-21T00:00:00Z",
        ),
        Err(ModelError::InvalidTimeline)
    ));

    let session = Session::new(
        StableId::new("id", "session").unwrap(),
        Some(StableId::new("id", "missing").unwrap()),
        "Session",
        None,
        "2026-07-21T00:00:00Z",
        "2026-07-21T00:00:00Z",
    )
    .unwrap();
    assert!(matches!(
        VaultDocument::new(vec![], vec![session], vec![]),
        Err(ModelError::DanglingReference("session.person_id"))
    ));
}

#[test]
fn deserialization_rejects_unknown_fields_and_bad_versions() {
    let json = VaultDocument::empty().to_json().unwrap();
    assert!(matches!(
        VaultDocument::from_json(&json.replacen("\"schema_version\":1", "\"schema_version\":2", 1)),
        Err(ModelError::UnsupportedSchema(2))
    ));
    assert!(matches!(
        VaultDocument::from_json(&json.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"unexpected\":true",
            1
        )),
        Err(ModelError::Json(_))
    ));
}

#[test]
fn pinned_sibylla_artifacts_are_validated_and_canonicalized() {
    let record = ArtifactRecord::from_sibylla(
        StableId::new("artifact.id", "deck_record").unwrap(),
        None,
        None,
        SIBYLLA_DECK_ARTIFACT,
    )
    .unwrap();

    assert_eq!(record.kind(), ArtifactKind::SibyllaDeck);
    assert_eq!(
        record.producer_revision(),
        oracle_studio_core::SIBYLLA_REVISION
    );
    assert!(record.content_id().starts_with("sha256:"));
    assert!(!record.canonical_json().contains('\n'));
}

#[test]
fn artifact_metadata_is_revalidated_when_a_document_reopens() {
    let record = ArtifactRecord::from_sibylla(
        StableId::new("artifact.id", "deck_record").unwrap(),
        None,
        None,
        SIBYLLA_DECK_ARTIFACT,
    )
    .unwrap();
    let document = VaultDocument::new(vec![], vec![], vec![record]).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    value["artifacts"][0]["content_id"] =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();

    assert!(matches!(
        VaultDocument::from_json(&serde_json::to_string(&value).unwrap()),
        Err(ModelError::ArtifactMetadataMismatch)
    ));
}
