use std::fs;

use oracle_studio_assets::{AssetSource, DeckAsset, DeckPackManifest};
use sha2::{Digest, Sha256};
use sibylla_artifacts::DeckArtifact;
use sibylla_core::DeckManifest;

const DECK: &str = r#"{
  "schema_version": 1,
  "id": "fictional_deck",
  "name": "Fictional Deck",
  "attribution": {"author": "Oracle Studio", "artist": null, "publisher": null},
  "tradition": "Original metadata-only fixture",
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
    "asset_id": "fool",
    "correspondences": [],
    "notes": null
  }]
}"#;

fn asset(bytes: &[u8]) -> DeckAsset {
    DeckAsset::new(
        "fool",
        "images/fool.png",
        format!("{:x}", Sha256::digest(bytes)),
        "image/png",
        500,
        857,
        AssetSource::new(
            "https://commons.wikimedia.org/wiki/File:The_Fool.png",
            "https://upload.wikimedia.org/example.png",
            "Public domain",
            Some("https://creativecommons.org/publicdomain/mark/1.0/".into()),
        ),
    )
    .unwrap()
}

#[test]
fn pack_round_trips_and_verifies_local_bytes() {
    let bytes = b"fictional image bytes";
    let pack = DeckPackManifest::new(
        "fictional_pack",
        format!("sha256:{}", "a".repeat(64)),
        vec![asset(bytes)],
    )
    .unwrap();
    let reopened = DeckPackManifest::from_json(&pack.to_json().unwrap()).unwrap();
    assert_eq!(reopened, pack);

    let root = tempfile_dir();
    fs::create_dir_all(root.join("images")).unwrap();
    fs::write(root.join("images/fool.png"), bytes).unwrap();
    let verified = pack.verify_files(&root).unwrap();
    assert_eq!(verified[0].asset_id, "fool");
}

#[test]
fn paths_hashes_and_unknown_fields_are_rejected() {
    let bytes = b"fictional image bytes";
    let json = DeckPackManifest::new(
        "fictional_pack",
        format!("sha256:{}", "a".repeat(64)),
        vec![asset(bytes)],
    )
    .unwrap()
    .to_json()
    .unwrap();
    assert!(DeckPackManifest::from_json(&json.replace("images/fool.png", "../fool.png")).is_err());
    assert!(DeckPackManifest::from_json(&json.replace("\"sha256\":\"", "\"sha256\":\"z")).is_err());
    assert!(
        DeckPackManifest::from_json(
            &json.replace("\"assets\":[", "\"unexpected\":true,\"assets\":[")
        )
        .is_err()
    );
}

#[test]
fn pack_belongs_to_the_exact_sibylla_deck_content_id() {
    let deck = DeckManifest::from_json(DECK).unwrap();
    let envelope = DeckArtifact::new(deck).to_json().unwrap();
    let content_id = DeckArtifact::from_json(&envelope)
        .unwrap()
        .content_id()
        .unwrap()
        .to_string();
    let pack = DeckPackManifest::new(
        "fictional_pack",
        content_id,
        vec![asset(b"fictional image bytes")],
    )
    .unwrap();
    pack.verify_deck_artifact(&envelope).unwrap();
}

fn tempfile_dir() -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("oracle-studio-assets-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir(&path).unwrap();
    path
}
