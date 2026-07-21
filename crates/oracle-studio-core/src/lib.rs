//! Validated application-owned composition records for Oracle Studio.

use std::collections::BTreeSet;

use astraeus_artifacts::CalculationArtifact;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sibylla_artifacts::{Artifact as SibyllaArtifact, ArtifactKind as SibyllaKind};
use thiserror::Error;

pub const VAULT_DOCUMENT_SCHEMA_VERSION: u32 = 1;
pub const ASTRAEUS_REVISION: &str = "952a143b700ea5cad960498e7d8916a49ebb3691";
pub const SIBYLLA_REVISION: &str = "a154c32b83b110d2568a9ab10828b4f8b3dba7c7";

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct StableId(String);

impl StableId {
    pub fn new(field: &'static str, value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-' | b'.' | b':')
            });
        if !valid {
            return Err(ModelError::InvalidId { field });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonKind {
    Personal,
    ProfessionalClient,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PersonProfile {
    id: StableId,
    display_name: String,
    kind: PersonKind,
    notes: Option<String>,
}

impl PersonProfile {
    pub fn new(
        id: StableId,
        display_name: impl Into<String>,
        kind: PersonKind,
        notes: Option<String>,
    ) -> Result<Self, ModelError> {
        let display_name = display_name.into();
        validate_text("person.display_name", &display_name)?;
        validate_optional_text("person.notes", notes.as_deref())?;
        Ok(Self {
            id,
            display_name,
            kind,
            notes,
        })
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub const fn kind(&self) -> PersonKind {
        self.kind
    }
    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Session {
    id: StableId,
    person_id: Option<StableId>,
    title: String,
    context: Option<String>,
    created_at: String,
    modified_at: String,
}

impl Session {
    pub fn new(
        id: StableId,
        person_id: Option<StableId>,
        title: impl Into<String>,
        context: Option<String>,
        created_at: impl Into<String>,
        modified_at: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let title = title.into();
        let created_at = normalize_timestamp("session.created_at", created_at.into())?;
        let modified_at = normalize_timestamp("session.modified_at", modified_at.into())?;
        validate_text("session.title", &title)?;
        validate_optional_text("session.context", context.as_deref())?;
        if modified_at < created_at {
            return Err(ModelError::InvalidTimeline);
        }
        Ok(Self {
            id,
            person_id,
            title,
            context,
            created_at,
            modified_at,
        })
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn person_id(&self) -> Option<&StableId> {
        self.person_id.as_ref()
    }
    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    AstraeusCalculation,
    SibyllaDeck,
    SibyllaReading,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactRecord {
    id: StableId,
    person_id: Option<StableId>,
    session_id: Option<StableId>,
    kind: ArtifactKind,
    producer_revision: String,
    content_id: String,
    canonical_json: String,
}

impl ArtifactRecord {
    pub fn from_astraeus_calculation(
        id: StableId,
        person_id: Option<StableId>,
        session_id: Option<StableId>,
        json: &str,
    ) -> Result<Self, ModelError> {
        let artifact = CalculationArtifact::from_json(json)
            .map_err(|error| ModelError::InvalidArtifact(error.to_string()))?;
        Ok(Self {
            id,
            person_id,
            session_id,
            kind: ArtifactKind::AstraeusCalculation,
            producer_revision: ASTRAEUS_REVISION.into(),
            content_id: artifact
                .content_id()
                .map_err(|error| ModelError::InvalidArtifact(error.to_string()))?,
            canonical_json: artifact
                .to_json()
                .map_err(|error| ModelError::InvalidArtifact(error.to_string()))?,
        })
    }

    pub fn from_sibylla(
        id: StableId,
        person_id: Option<StableId>,
        session_id: Option<StableId>,
        json: &str,
    ) -> Result<Self, ModelError> {
        let artifact = SibyllaArtifact::from_json(json)
            .map_err(|error| ModelError::InvalidArtifact(error.to_string()))?;
        let kind = match artifact.kind() {
            SibyllaKind::Deck => ArtifactKind::SibyllaDeck,
            SibyllaKind::Reading => ArtifactKind::SibyllaReading,
        };
        Ok(Self {
            id,
            person_id,
            session_id,
            kind,
            producer_revision: SIBYLLA_REVISION.into(),
            content_id: artifact
                .content_id()
                .map_err(|error| ModelError::InvalidArtifact(error.to_string()))?
                .to_string(),
            canonical_json: artifact
                .to_json()
                .map_err(|error| ModelError::InvalidArtifact(error.to_string()))?,
        })
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn person_id(&self) -> Option<&StableId> {
        self.person_id.as_ref()
    }
    pub fn session_id(&self) -> Option<&StableId> {
        self.session_id.as_ref()
    }
    pub const fn kind(&self) -> ArtifactKind {
        self.kind
    }
    pub fn producer_revision(&self) -> &str {
        &self.producer_revision
    }
    pub fn content_id(&self) -> &str {
        &self.content_id
    }
    pub fn canonical_json(&self) -> &str {
        &self.canonical_json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultDocument {
    people: Vec<PersonProfile>,
    sessions: Vec<Session>,
    artifacts: Vec<ArtifactRecord>,
}

#[derive(Serialize)]
struct VaultDocumentRef<'a> {
    schema_version: u32,
    people: &'a [PersonProfile],
    sessions: &'a [Session],
    artifacts: &'a [ArtifactRecord],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultDocumentWire {
    schema_version: u32,
    people: Vec<PersonWire>,
    sessions: Vec<SessionWire>,
    artifacts: Vec<ArtifactWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersonWire {
    id: String,
    display_name: String,
    kind: PersonKind,
    notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionWire {
    id: String,
    person_id: Option<String>,
    title: String,
    context: Option<String>,
    created_at: String,
    modified_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    id: String,
    person_id: Option<String>,
    session_id: Option<String>,
    kind: ArtifactKind,
    producer_revision: String,
    content_id: String,
    canonical_json: String,
}

impl VaultDocument {
    pub fn new(
        people: Vec<PersonProfile>,
        sessions: Vec<Session>,
        artifacts: Vec<ArtifactRecord>,
    ) -> Result<Self, ModelError> {
        validate_unique(people.iter().map(PersonProfile::id), "person")?;
        validate_unique(sessions.iter().map(Session::id), "session")?;
        validate_unique(artifacts.iter().map(ArtifactRecord::id), "artifact")?;
        let person_ids: BTreeSet<_> = people.iter().map(PersonProfile::id).collect();
        let session_ids: BTreeSet<_> = sessions.iter().map(Session::id).collect();
        for session in &sessions {
            if session
                .person_id()
                .is_some_and(|id| !person_ids.contains(id))
            {
                return Err(ModelError::DanglingReference("session.person_id"));
            }
        }
        for artifact in &artifacts {
            if artifact
                .person_id()
                .is_some_and(|id| !person_ids.contains(id))
            {
                return Err(ModelError::DanglingReference("artifact.person_id"));
            }
            if artifact
                .session_id()
                .is_some_and(|id| !session_ids.contains(id))
            {
                return Err(ModelError::DanglingReference("artifact.session_id"));
            }
            if let (Some(artifact_person), Some(session_id)) =
                (artifact.person_id(), artifact.session_id())
            {
                let session = sessions
                    .iter()
                    .find(|session| session.id() == session_id)
                    .expect("session reference was validated above");
                if session.person_id().is_some_and(|id| id != artifact_person) {
                    return Err(ModelError::ArtifactPersonSessionMismatch);
                }
            }
        }
        Ok(Self {
            people,
            sessions,
            artifacts,
        })
    }

    pub fn empty() -> Self {
        Self {
            people: Vec::new(),
            sessions: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    pub fn people(&self) -> &[PersonProfile] {
        &self.people
    }
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }
    pub fn artifacts(&self) -> &[ArtifactRecord] {
        &self.artifacts
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        Ok(serde_json::to_string(&VaultDocumentRef {
            schema_version: VAULT_DOCUMENT_SCHEMA_VERSION,
            people: &self.people,
            sessions: &self.sessions,
            artifacts: &self.artifacts,
        })?)
    }

    pub fn from_json(input: &str) -> Result<Self, ModelError> {
        let wire: VaultDocumentWire = serde_json::from_str(input)?;
        if wire.schema_version != VAULT_DOCUMENT_SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema(wire.schema_version));
        }
        let people = wire
            .people
            .into_iter()
            .map(PersonWire::into_model)
            .collect::<Result<_, _>>()?;
        let sessions = wire
            .sessions
            .into_iter()
            .map(SessionWire::into_model)
            .collect::<Result<_, _>>()?;
        let artifacts = wire
            .artifacts
            .into_iter()
            .map(ArtifactWire::into_model)
            .collect::<Result<_, _>>()?;
        Self::new(people, sessions, artifacts)
    }
}

impl PersonWire {
    fn into_model(self) -> Result<PersonProfile, ModelError> {
        PersonProfile::new(
            StableId::new("person.id", self.id)?,
            self.display_name,
            self.kind,
            self.notes,
        )
    }
}

impl SessionWire {
    fn into_model(self) -> Result<Session, ModelError> {
        Session::new(
            StableId::new("session.id", self.id)?,
            self.person_id
                .map(|id| StableId::new("session.person_id", id))
                .transpose()?,
            self.title,
            self.context,
            self.created_at,
            self.modified_at,
        )
    }
}

impl ArtifactWire {
    fn into_model(self) -> Result<ArtifactRecord, ModelError> {
        let id = StableId::new("artifact.id", self.id)?;
        let person_id = self
            .person_id
            .map(|id| StableId::new("artifact.person_id", id))
            .transpose()?;
        let session_id = self
            .session_id
            .map(|id| StableId::new("artifact.session_id", id))
            .transpose()?;
        let rebuilt = match self.kind {
            ArtifactKind::AstraeusCalculation => ArtifactRecord::from_astraeus_calculation(
                id,
                person_id,
                session_id,
                &self.canonical_json,
            )?,
            ArtifactKind::SibyllaDeck | ArtifactKind::SibyllaReading => {
                ArtifactRecord::from_sibylla(id, person_id, session_id, &self.canonical_json)?
            }
        };
        if rebuilt.kind != self.kind
            || rebuilt.producer_revision != self.producer_revision
            || rebuilt.content_id != self.content_id
        {
            return Err(ModelError::ArtifactMetadataMismatch);
        }
        Ok(rebuilt)
    }
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("invalid stable ID in {field}")]
    InvalidId { field: &'static str },
    #[error("{0} must not be blank")]
    EmptyText(&'static str),
    #[error("invalid RFC 3339 timestamp in {0}")]
    InvalidTimestamp(&'static str),
    #[error("modified timestamp precedes created timestamp")]
    InvalidTimeline,
    #[error("duplicate {0} ID")]
    DuplicateId(&'static str),
    #[error("dangling reference in {0}")]
    DanglingReference(&'static str),
    #[error("invalid engine artifact: {0}")]
    InvalidArtifact(String),
    #[error("artifact metadata does not match its canonical payload")]
    ArtifactMetadataMismatch,
    #[error("artifact person does not match its session person")]
    ArtifactPersonSessionMismatch,
    #[error("unsupported vault document schema version {0}")]
    UnsupportedSchema(u32),
    #[error("invalid vault document JSON: {0}")]
    Json(#[from] serde_json::Error),
}

fn validate_text(field: &'static str, value: &str) -> Result<(), ModelError> {
    if value.trim().is_empty() {
        Err(ModelError::EmptyText(field))
    } else {
        Ok(())
    }
}

fn validate_optional_text(field: &'static str, value: Option<&str>) -> Result<(), ModelError> {
    value.map_or(Ok(()), |value| validate_text(field, value))
}

fn normalize_timestamp(field: &'static str, value: String) -> Result<String, ModelError> {
    DateTime::parse_from_rfc3339(&value)
        .map_err(|_| ModelError::InvalidTimestamp(field))
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
}

fn validate_unique<'a>(
    values: impl Iterator<Item = &'a StableId>,
    kind: &'static str,
) -> Result<(), ModelError> {
    let mut ids = BTreeSet::new();
    for id in values {
        if !ids.insert(id) {
            return Err(ModelError::DuplicateId(kind));
        }
    }
    Ok(())
}
