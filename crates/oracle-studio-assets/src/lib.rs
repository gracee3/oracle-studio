//! Validated, application-owned indexes for local deck assets.
//!
//! Sibylla owns optional opaque `asset_id` references but deliberately does not
//! own files, licensing metadata, or image processing. This crate validates the
//! sidecar that lets Oracle Studio resolve those references without changing a
//! tarot artifact.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sibylla_artifacts::{Artifact, ArtifactError, ArtifactKind};
use thiserror::Error;

pub const DECK_PACK_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeckPackManifest {
    schema_version: u32,
    pack_id: String,
    deck_content_id: String,
    assets: Vec<DeckAsset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckPackWire {
    schema_version: u32,
    pack_id: String,
    deck_content_id: String,
    assets: Vec<DeckAsset>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckAsset {
    asset_id: String,
    local_path: String,
    sha256: String,
    mime: String,
    width_pixels: u32,
    height_pixels: u32,
    source: AssetSource,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSource {
    file_page: String,
    original_url: String,
    license: String,
    usage_terms: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedAsset {
    pub asset_id: String,
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("invalid deck-pack JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported deck-pack schema version {0}")]
    UnsupportedSchema(u32),
    #[error("deck-pack ID is invalid")]
    InvalidPackId,
    #[error("deck content ID must be sha256 followed by 64 lowercase hexadecimal digits")]
    InvalidDeckContentId,
    #[error("deck-pack contains no assets")]
    EmptyPack,
    #[error("duplicate deck asset ID `{0}`")]
    DuplicateAssetId(String),
    #[error("deck asset ID is invalid: `{0}`")]
    InvalidAssetId(String),
    #[error("deck asset path must be relative and cannot contain `..`: `{0}`")]
    InvalidAssetPath(String),
    #[error("deck asset hash is invalid for `{0}`")]
    InvalidAssetHash(String),
    #[error("deck asset dimensions are invalid for `{0}`")]
    InvalidDimensions(String),
    #[error("deck asset source metadata is invalid for `{0}`")]
    InvalidSource(String),
    #[error("deck asset file is missing for `{0}`: {1}")]
    MissingAsset(String, PathBuf),
    #[error("unsupported PNG asset data for `{0}`")]
    UnsupportedImage(String),
    #[error("deck asset file is missing: {0}")]
    MissingFile(PathBuf),
    #[error("deck asset path is a symbolic link: {0}")]
    SymbolicLink(PathBuf),
    #[error("could not read deck asset {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("deck asset hash mismatch for {path}: expected {expected}, found {actual}")]
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("expected a Sibylla deck artifact, found {0}")]
    ExpectedDeck(ArtifactKind),
    #[error(transparent)]
    Sibylla(#[from] ArtifactError),
}

impl DeckPackManifest {
    /// Builds a v1 sidecar for decks whose local assets are `<asset_id>.png`.
    ///
    /// This intentionally handles only PNG dimensions: it keeps pack creation
    /// deterministic and avoids making image decoding part of the journal.
    pub fn from_deck_artifact_png(
        pack_id: impl Into<String>,
        deck_json: &str,
        root: &Path,
        source: AssetSource,
    ) -> Result<Self, AssetError> {
        let deck = sibylla_artifacts::DeckArtifact::from_json(deck_json)?;
        let deck_content_id = deck.content_id()?.to_string();
        let mut assets = Vec::new();
        for card in deck.payload().enabled_cards() {
            let Some(asset_id) = card.asset_id() else {
                continue;
            };
            let asset_id = asset_id.as_str();
            let local_path = format!("{asset_id}.png");
            let path = root.join(&local_path);
            let bytes = fs::read(&path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    AssetError::MissingAsset(asset_id.to_owned(), path.clone())
                } else {
                    AssetError::ReadFile {
                        path: path.clone(),
                        source: error,
                    }
                }
            })?;
            let (width_pixels, height_pixels) = png_dimensions(asset_id, &bytes)?;
            assets.push(DeckAsset::new(
                asset_id,
                local_path,
                format!("{:x}", Sha256::digest(bytes)),
                "image/png",
                width_pixels,
                height_pixels,
                source.clone(),
            )?);
        }
        Self::new(pack_id, deck_content_id, assets)
    }

    pub fn new(
        pack_id: impl Into<String>,
        deck_content_id: impl Into<String>,
        assets: Vec<DeckAsset>,
    ) -> Result<Self, AssetError> {
        let pack_id = pack_id.into();
        let deck_content_id = deck_content_id.into();
        validate_id(&pack_id).map_err(|_| AssetError::InvalidPackId)?;
        validate_content_id(&deck_content_id)?;
        if assets.is_empty() {
            return Err(AssetError::EmptyPack);
        }
        let mut ids = std::collections::BTreeSet::new();
        for asset in &assets {
            asset.validate()?;
            if !ids.insert(asset.asset_id.clone()) {
                return Err(AssetError::DuplicateAssetId(asset.asset_id.clone()));
            }
        }
        Ok(Self {
            schema_version: DECK_PACK_SCHEMA_VERSION,
            pack_id,
            deck_content_id,
            assets,
        })
    }

    pub fn from_json(input: &str) -> Result<Self, AssetError> {
        let wire: DeckPackWire = serde_json::from_str(input)?;
        if wire.schema_version != DECK_PACK_SCHEMA_VERSION {
            return Err(AssetError::UnsupportedSchema(wire.schema_version));
        }
        Self::new(wire.pack_id, wire.deck_content_id, wire.assets)
    }

    pub fn to_json(&self) -> Result<String, AssetError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn pack_id(&self) -> &str {
        &self.pack_id
    }

    pub fn deck_content_id(&self) -> &str {
        &self.deck_content_id
    }

    pub fn assets(&self) -> &[DeckAsset] {
        &self.assets
    }

    /// Confirms that the pack belongs to the exact immutable Sibylla deck.
    pub fn verify_deck_artifact(&self, json: &str) -> Result<(), AssetError> {
        let artifact = Artifact::from_json(json)?;
        let actual = match artifact {
            Artifact::Deck(deck) => deck.content_id()?.to_string(),
            Artifact::Reading(_) => return Err(AssetError::ExpectedDeck(ArtifactKind::Reading)),
        };
        if actual != self.deck_content_id {
            return Err(AssetError::HashMismatch {
                path: PathBuf::from("deck artifact"),
                expected: self.deck_content_id.clone(),
                actual,
            });
        }
        Ok(())
    }

    /// Verifies every referenced file without following symlink targets.
    pub fn verify_files(&self, root: &Path) -> Result<Vec<VerifiedAsset>, AssetError> {
        self.assets
            .iter()
            .map(|asset| {
                let path = root.join(&asset.local_path);
                let metadata = fs::symlink_metadata(&path).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        AssetError::MissingFile(path.clone())
                    } else {
                        AssetError::ReadFile {
                            path: path.clone(),
                            source: error,
                        }
                    }
                })?;
                if metadata.file_type().is_symlink() {
                    return Err(AssetError::SymbolicLink(path));
                }
                let bytes = fs::read(&path).map_err(|source| AssetError::ReadFile {
                    path: path.clone(),
                    source,
                })?;
                let actual = format!("{:x}", Sha256::digest(bytes));
                if actual != asset.sha256 {
                    return Err(AssetError::HashMismatch {
                        path,
                        expected: asset.sha256.clone(),
                        actual,
                    });
                }
                Ok(VerifiedAsset {
                    asset_id: asset.asset_id.clone(),
                    path,
                    sha256: actual,
                })
            })
            .collect()
    }
}

impl DeckAsset {
    pub fn new(
        asset_id: impl Into<String>,
        local_path: impl Into<String>,
        sha256: impl Into<String>,
        mime: impl Into<String>,
        width_pixels: u32,
        height_pixels: u32,
        source: AssetSource,
    ) -> Result<Self, AssetError> {
        let asset = Self {
            asset_id: asset_id.into(),
            local_path: local_path.into(),
            sha256: sha256.into(),
            mime: mime.into(),
            width_pixels,
            height_pixels,
            source,
        };
        asset.validate()?;
        Ok(asset)
    }

    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }
    pub fn local_path(&self) -> &str {
        &self.local_path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn mime(&self) -> &str {
        &self.mime
    }
    pub const fn width_pixels(&self) -> u32 {
        self.width_pixels
    }
    pub const fn height_pixels(&self) -> u32 {
        self.height_pixels
    }
    pub fn source(&self) -> &AssetSource {
        &self.source
    }

    fn validate(&self) -> Result<(), AssetError> {
        validate_id(&self.asset_id)
            .map_err(|_| AssetError::InvalidAssetId(self.asset_id.clone()))?;
        let path = Path::new(&self.local_path);
        if path.is_absolute()
            || self.local_path.is_empty()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(AssetError::InvalidAssetPath(self.local_path.clone()));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AssetError::InvalidAssetHash(self.asset_id.clone()));
        }
        if self.width_pixels == 0 || self.height_pixels == 0 {
            return Err(AssetError::InvalidDimensions(self.asset_id.clone()));
        }
        if self.mime.is_empty() || self.mime.len() > 128 || self.mime.contains(char::is_whitespace)
        {
            return Err(AssetError::InvalidSource(self.asset_id.clone()));
        }
        self.source.validate(&self.asset_id)
    }
}

impl AssetSource {
    pub fn new(
        file_page: impl Into<String>,
        original_url: impl Into<String>,
        license: impl Into<String>,
        usage_terms: Option<String>,
    ) -> Self {
        Self {
            file_page: file_page.into(),
            original_url: original_url.into(),
            license: license.into(),
            usage_terms,
        }
    }

    pub fn file_page(&self) -> &str {
        &self.file_page
    }
    pub fn original_url(&self) -> &str {
        &self.original_url
    }
    pub fn license(&self) -> &str {
        &self.license
    }
    pub fn usage_terms(&self) -> Option<&str> {
        self.usage_terms.as_deref()
    }

    fn validate(&self, asset_id: &str) -> Result<(), AssetError> {
        for url in [&self.file_page, &self.original_url] {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(AssetError::InvalidSource(asset_id.to_owned()));
            }
        }
        if self.license.trim().is_empty() {
            return Err(AssetError::InvalidSource(asset_id.to_owned()));
        }
        Ok(())
    }
}

fn validate_id(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-' | b'.' | b':')
        })
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_content_id(value: &str) -> Result<(), AssetError> {
    let Some(hash) = value.strip_prefix("sha256:") else {
        return Err(AssetError::InvalidDeckContentId);
    };
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AssetError::InvalidDeckContentId);
    }
    Ok(())
}

fn png_dimensions(asset_id: &str, bytes: &[u8]) -> Result<(u32, u32), AssetError> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err(AssetError::UnsupportedImage(asset_id.to_owned()));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("checked length"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("checked length"));
    if width == 0 || height == 0 {
        return Err(AssetError::UnsupportedImage(asset_id.to_owned()));
    }
    Ok((width, height))
}
