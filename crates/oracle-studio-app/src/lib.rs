//! Reusable offline use-case services for Oracle Studio.

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{CalculationRequest, EphemerisAdapter};
use astraeus_swiss::SwissEphemerisAdapter;
use oracle_studio_assets::{DeckPackManifest, VerifiedAsset};
use oracle_studio_core::{
    ArtifactKind, ArtifactRecord, JournalEntry, PersonProfile, Session, StableId, VaultDocument,
};
use sibylla_artifacts::{Artifact, DeckArtifact, ReadingArtifact};
use sibylla_core::{
    DeckManifest, DrawProvenance, FollowUp, Orientation, Placement, SpreadDefinition, TarotReading,
    UtcInstant,
};
use thiserror::Error;

pub struct StudioService;

#[derive(Clone, Debug)]
pub struct ManualPlacementInput {
    pub deck_card_id: String,
    pub orientation: Orientation,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ReadingRequest {
    pub artifact_record_id: StableId,
    pub reading_id: String,
    pub deck_record_id: StableId,
    pub person_id: Option<StableId>,
    pub session_id: Option<StableId>,
    pub spread: SpreadDefinition,
    pub question: Option<String>,
    pub context: Option<String>,
    pub reader_notes: Option<String>,
    pub interpretation: Option<String>,
    pub timestamp: UtcInstant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchEntity {
    Person,
    Session,
    Artifact,
    JournalEntry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    entity: SearchEntity,
    id: StableId,
    snippet: String,
}

impl SearchHit {
    pub const fn entity(&self) -> SearchEntity {
        self.entity
    }
    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn snippet(&self) -> &str {
        &self.snippet
    }
}

impl StudioService {
    pub fn verify_deck_pack(
        document: &VaultDocument,
        deck_record_id: &StableId,
        pack_json: &str,
        asset_root: &std::path::Path,
    ) -> Result<Vec<VerifiedAsset>, AppError> {
        let record = document
            .artifacts()
            .iter()
            .find(|record| record.id() == deck_record_id)
            .ok_or(AppError::NotFound("deck artifact"))?;
        if record.kind() != ArtifactKind::SibyllaDeck {
            return Err(AppError::ExpectedDeck);
        }
        let pack = DeckPackManifest::from_json(pack_json)?;
        pack.verify_deck_artifact(record.canonical_json())?;
        Ok(pack.verify_files(asset_root)?)
    }

    pub fn deck_manifest(
        document: &VaultDocument,
        id: &StableId,
    ) -> Result<DeckManifest, AppError> {
        deck_for(document, id)
    }

    pub fn add_person(
        document: &VaultDocument,
        person: PersonProfile,
    ) -> Result<VaultDocument, AppError> {
        let mut people = document.people().to_vec();
        people.push(person);
        rebuild(
            document,
            people,
            document.sessions().to_vec(),
            document.artifacts().to_vec(),
            document.journal_entries().to_vec(),
        )
    }

    pub fn replace_person(
        document: &VaultDocument,
        person: PersonProfile,
    ) -> Result<VaultDocument, AppError> {
        let mut people = document.people().to_vec();
        let existing = people
            .iter_mut()
            .find(|existing| existing.id() == person.id())
            .ok_or(AppError::NotFound("person"))?;
        *existing = person;
        rebuild(
            document,
            people,
            document.sessions().to_vec(),
            document.artifacts().to_vec(),
            document.journal_entries().to_vec(),
        )
    }

    pub fn add_session(
        document: &VaultDocument,
        session: Session,
    ) -> Result<VaultDocument, AppError> {
        let mut sessions = document.sessions().to_vec();
        sessions.push(session);
        rebuild(
            document,
            document.people().to_vec(),
            sessions,
            document.artifacts().to_vec(),
            document.journal_entries().to_vec(),
        )
    }

    pub fn replace_session(
        document: &VaultDocument,
        session: Session,
    ) -> Result<VaultDocument, AppError> {
        let mut sessions = document.sessions().to_vec();
        let existing = sessions
            .iter_mut()
            .find(|existing| existing.id() == session.id())
            .ok_or(AppError::NotFound("session"))?;
        *existing = session;
        rebuild(
            document,
            document.people().to_vec(),
            sessions,
            document.artifacts().to_vec(),
            document.journal_entries().to_vec(),
        )
    }

    pub fn import_deck(
        document: &VaultDocument,
        record_id: StableId,
        json: &str,
    ) -> Result<VaultDocument, AppError> {
        let canonical = match Artifact::from_json(json) {
            Ok(Artifact::Deck(deck)) => deck.to_json()?,
            Ok(Artifact::Reading(_)) => return Err(AppError::ExpectedDeck),
            Err(_) => DeckArtifact::new(DeckManifest::from_json(json)?).to_json()?,
        };
        let record = ArtifactRecord::from_sibylla(record_id, None, None, &canonical)?;
        add_artifact(document, record)
    }

    pub fn import_chart(
        document: &VaultDocument,
        record_id: StableId,
        person_id: Option<StableId>,
        session_id: Option<StableId>,
        json: &str,
    ) -> Result<VaultDocument, AppError> {
        add_artifact(
            document,
            ArtifactRecord::from_astraeus_calculation(record_id, person_id, session_id, json)?,
        )
    }

    /// Calculate a chart with Astraeus and persist its immutable artifact.
    ///
    /// Local-time resolution, person/session ownership, and encrypted storage
    /// remain Oracle Studio concerns; Astraeus receives an exact UTC request.
    pub fn calculate_chart(
        document: &VaultDocument,
        record_id: StableId,
        person_id: Option<StableId>,
        session_id: Option<StableId>,
        request: CalculationRequest,
    ) -> Result<VaultDocument, AppError> {
        let result = SwissEphemerisAdapter::moshier()
            .calculate(&request)
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let artifact = CalculationArtifact::new(request, result)
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let json = artifact
            .to_json()
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        Self::import_chart(document, record_id, person_id, session_id, &json)
    }

    pub fn record_manual_reading(
        document: &VaultDocument,
        request: ReadingRequest,
        cards: Vec<ManualPlacementInput>,
    ) -> Result<VaultDocument, AppError> {
        let deck = deck_for(document, &request.deck_record_id)?;
        let timestamp = request.timestamp;
        if cards.len() != request.spread.positions().len() {
            return Err(AppError::PlacementCount);
        }
        let placements = request
            .spread
            .positions()
            .iter()
            .zip(cards)
            .enumerate()
            .map(|(index, (position, input))| {
                let card_id = sibylla_core::StableId::new("deck_card_id", input.deck_card_id)?;
                let card = deck
                    .cards()
                    .iter()
                    .find(|card| card.id() == &card_id)
                    .ok_or(AppError::UnknownDeckCard)?;
                Placement::new(
                    position.id().clone(),
                    position.label(),
                    card.identity().clone(),
                    card.id().clone(),
                    card.printed_title(),
                    input.orientation,
                    u32::try_from(index + 1).map_err(|_| AppError::PlacementCount)?,
                    input.notes,
                )
                .map_err(AppError::SibyllaValidation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        finish_reading(
            document,
            request,
            deck,
            placements,
            DrawProvenance::Manual {
                recorded_at: timestamp,
            },
        )
    }

    pub fn record_software_reading(
        document: &VaultDocument,
        request: ReadingRequest,
    ) -> Result<VaultDocument, AppError> {
        let deck = deck_for(document, &request.deck_record_id)?;
        let timestamp = request.timestamp;
        let shuffled = sibylla_shuffle::shuffle(&deck, timestamp)?;
        let needed = request.spread.positions().len();
        if needed > shuffled.cards().len() {
            return Err(AppError::PlacementCount);
        }
        let placements = request
            .spread
            .positions()
            .iter()
            .zip(shuffled.cards().iter().take(needed))
            .enumerate()
            .map(|(index, (position, card))| {
                Placement::new(
                    position.id().clone(),
                    position.label(),
                    card.card_identity().clone(),
                    card.deck_card_id().clone(),
                    card.printed_title(),
                    card.orientation(),
                    u32::try_from(index + 1).map_err(|_| AppError::PlacementCount)?,
                    None,
                )
                .map_err(AppError::SibyllaValidation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        finish_reading(
            document,
            request,
            deck,
            placements,
            shuffled.provenance().clone(),
        )
    }

    pub fn add_journal_entry(
        document: &VaultDocument,
        entry: JournalEntry,
    ) -> Result<VaultDocument, AppError> {
        let mut entries = document.journal_entries().to_vec();
        entries.push(entry);
        rebuild(
            document,
            document.people().to_vec(),
            document.sessions().to_vec(),
            document.artifacts().to_vec(),
            entries,
        )
    }

    pub fn search(document: &VaultDocument, query: &str) -> Result<Vec<SearchHit>, AppError> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Err(AppError::EmptyQuery);
        }
        let mut hits = Vec::new();
        for person in document.people() {
            push_hit(
                &mut hits,
                SearchEntity::Person,
                person.id(),
                &query,
                [Some(person.display_name()), person.notes()],
            );
        }
        for session in document.sessions() {
            push_hit(
                &mut hits,
                SearchEntity::Session,
                session.id(),
                &query,
                [Some(session.title()), session.context()],
            );
        }
        for artifact in document.artifacts() {
            push_hit(
                &mut hits,
                SearchEntity::Artifact,
                artifact.id(),
                &query,
                [Some(artifact.canonical_json()), None],
            );
        }
        for entry in document.journal_entries() {
            push_hit(
                &mut hits,
                SearchEntity::JournalEntry,
                entry.id(),
                &query,
                [Some(entry.content()), None],
            );
        }
        Ok(hits)
    }
}

fn finish_reading(
    document: &VaultDocument,
    request: ReadingRequest,
    deck: DeckManifest,
    placements: Vec<Placement>,
    provenance: DrawProvenance,
) -> Result<VaultDocument, AppError> {
    let reading = TarotReading::new(
        sibylla_core::StableId::new("reading.id", request.reading_id)?,
        request.person_id.as_ref().map(|id| id.as_str().to_owned()),
        request.session_id.as_ref().map(|id| id.as_str().to_owned()),
        deck,
        request.spread,
        request.question,
        request.context,
        placements,
        provenance,
        request.reader_notes,
        request.interpretation,
        Vec::<FollowUp>::new(),
        request.timestamp,
        request.timestamp,
    )?;
    let json = ReadingArtifact::new(reading).to_json()?;
    add_artifact(
        document,
        ArtifactRecord::from_sibylla(
            request.artifact_record_id,
            request.person_id,
            request.session_id,
            &json,
        )?,
    )
}

fn deck_for(document: &VaultDocument, id: &StableId) -> Result<DeckManifest, AppError> {
    let record = document
        .artifacts()
        .iter()
        .find(|record| record.id() == id)
        .ok_or(AppError::NotFound("deck artifact"))?;
    if record.kind() != ArtifactKind::SibyllaDeck {
        return Err(AppError::ExpectedDeck);
    }
    match Artifact::from_json(record.canonical_json())? {
        Artifact::Deck(deck) => Ok(deck.into_payload()),
        Artifact::Reading(_) => Err(AppError::ExpectedDeck),
    }
}

fn add_artifact(
    document: &VaultDocument,
    artifact: ArtifactRecord,
) -> Result<VaultDocument, AppError> {
    let mut artifacts = document.artifacts().to_vec();
    artifacts.push(artifact);
    rebuild(
        document,
        document.people().to_vec(),
        document.sessions().to_vec(),
        artifacts,
        document.journal_entries().to_vec(),
    )
}

fn rebuild(
    _document: &VaultDocument,
    people: Vec<PersonProfile>,
    sessions: Vec<Session>,
    artifacts: Vec<ArtifactRecord>,
    entries: Vec<JournalEntry>,
) -> Result<VaultDocument, AppError> {
    Ok(VaultDocument::new(people, sessions, artifacts, entries)?)
}

fn push_hit<'a>(
    hits: &mut Vec<SearchHit>,
    entity: SearchEntity,
    id: &StableId,
    query: &str,
    fields: impl IntoIterator<Item = Option<&'a str>>,
) {
    if let Some(field) = fields
        .into_iter()
        .flatten()
        .find(|field| field.to_lowercase().contains(query))
    {
        let snippet: String = field.chars().take(160).collect();
        hits.push(SearchHit {
            entity,
            id: id.clone(),
            snippet,
        });
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0} was not found")]
    NotFound(&'static str),
    #[error("expected a Sibylla deck artifact")]
    ExpectedDeck,
    #[error("reading placement count does not match the spread or deck")]
    PlacementCount,
    #[error("reading references an unknown deck card")]
    UnknownDeckCard,
    #[error("search query must not be blank")]
    EmptyQuery,
    #[error(transparent)]
    Model(#[from] oracle_studio_core::ModelError),
    #[error(transparent)]
    Assets(#[from] oracle_studio_assets::AssetError),
    #[error(transparent)]
    Artifact(#[from] sibylla_artifacts::ArtifactError),
    #[error(transparent)]
    Manifest(#[from] sibylla_core::ManifestError),
    #[error("invalid Sibylla value: {0}")]
    SibyllaValidation(#[from] sibylla_core::ValidationError),
    #[error("Astraeus calculation failed: {0}")]
    Astraeus(String),
    #[error(transparent)]
    Shuffle(#[from] sibylla_shuffle::ShuffleError),
}
