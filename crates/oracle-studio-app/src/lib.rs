//! Reusable offline use-case services for Oracle Studio.

use astraeus_artifacts::CalculationArtifact;
use astraeus_comparison::{ComparisonArtifact, ComparisonKind, ComparisonSpecification};
use astraeus_core::{
    AspectDefinition as AstraeusAspectDefinition, AspectDefinitions,
    AspectKind as AstraeusAspectKind, Ayanamsa, CalculationOptions, CalculationRequest,
    CelestialObject, ChartPointId as AstraeusChartPointId, ChartPointSelection, EphemerisAdapter,
    GeographicLocation, HouseSystem, UtcInstant as AstraeusInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use astraeus_swiss::SwissEphemerisAdapter;
use oracle_studio_assets::{DeckPackManifest, VerifiedAsset};
use oracle_studio_core::{
    AmbiguousTimeChoice, ArtifactKind, ArtifactRecord, AspectDefinition, AspectKindId, AyanamsaId,
    CelestialObjectId, ChartCalculation, ChartCalculationOptions, ChartDefinition, ChartPointId,
    ChartRole, ComparisonPreset, HouseSystemId, JournalEntry, PersonProfile, SavedLocation,
    Session, StableId, VaultDocument, WorkspaceState, ZodiacId, select_local_time,
};
use sibylla_artifacts::{Artifact, DeckArtifact, ReadingArtifact};
use sibylla_core::{
    DeckManifest, DrawProvenance, FollowUp, Orientation, Placement, SpreadDefinition, TarotReading,
    UtcInstant,
};
use thiserror::Error;

pub struct StudioService;

#[derive(Clone, Debug)]
pub struct ChartCalculationRequest {
    pub chart_calculation_id: StableId,
    pub calculation_artifact_id: StableId,
    pub chart_definition_id: StableId,
    pub saved_location_id: StableId,
    pub ambiguous_time_choice: Option<AmbiguousTimeChoice>,
    pub calculated_at: String,
}

#[derive(Clone, Debug)]
pub struct ComparisonCalculationRequest {
    pub comparison_artifact_id: StableId,
    pub comparison_preset_id: StableId,
}

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

    pub fn bind_verified_deck_pack(
        document: &VaultDocument,
        deck_record_id: &StableId,
        pack_json: &str,
        asset_root: &std::path::Path,
    ) -> Result<VaultDocument, AppError> {
        let pack = DeckPackManifest::from_json(pack_json)?;
        let record = document
            .artifacts()
            .iter()
            .find(|record| record.id() == deck_record_id)
            .ok_or(AppError::NotFound("deck artifact"))?;
        if record.kind() != ArtifactKind::SibyllaDeck {
            return Err(AppError::ExpectedDeck);
        }
        pack.verify_deck_artifact(record.canonical_json())?;
        pack.verify_files(asset_root)?;
        let mut artifacts = document.artifacts().to_vec();
        let record = artifacts
            .iter_mut()
            .find(|record| record.id() == deck_record_id)
            .ok_or(AppError::NotFound("deck artifact"))?;
        record.bind_deck_pack(
            StableId::new("deck_pack.id", pack.pack_id())?,
            pack.deck_content_id(),
        )?;
        rebuild(
            document,
            document.people().to_vec(),
            document.sessions().to_vec(),
            artifacts,
            document.journal_entries().to_vec(),
        )
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

    pub fn add_saved_location(
        document: &VaultDocument,
        location: SavedLocation,
    ) -> Result<VaultDocument, AppError> {
        let mut locations = document.saved_locations().to_vec();
        locations.push(location);
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            locations,
            document.chart_definitions().to_vec(),
            document.chart_calculations().to_vec(),
            document.comparison_presets().to_vec(),
            document.workspace_state().clone(),
        )
    }

    pub fn replace_saved_location(
        document: &VaultDocument,
        location: SavedLocation,
    ) -> Result<VaultDocument, AppError> {
        let mut locations = document.saved_locations().to_vec();
        let existing = locations
            .iter_mut()
            .find(|existing| existing.id() == location.id())
            .ok_or(AppError::NotFound("saved location"))?;
        *existing = location;
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            locations,
            document.chart_definitions().to_vec(),
            document.chart_calculations().to_vec(),
            document.comparison_presets().to_vec(),
            document.workspace_state().clone(),
        )
    }

    pub fn add_chart_definition(
        document: &VaultDocument,
        chart: ChartDefinition,
    ) -> Result<VaultDocument, AppError> {
        let mut charts = document.chart_definitions().to_vec();
        charts.push(chart);
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            document.saved_locations().to_vec(),
            charts,
            document.chart_calculations().to_vec(),
            document.comparison_presets().to_vec(),
            document.workspace_state().clone(),
        )
    }

    pub fn replace_chart_definition(
        document: &VaultDocument,
        mut chart: ChartDefinition,
    ) -> Result<VaultDocument, AppError> {
        let mut charts = document.chart_definitions().to_vec();
        let existing = charts
            .iter_mut()
            .find(|existing| existing.id() == chart.id())
            .ok_or(AppError::NotFound("chart definition"))?;
        if let Some(calculation_id) = existing.current_calculation_id()
            && chart.current_calculation_id().is_none()
        {
            chart.set_current_calculation(calculation_id.clone());
        }
        *existing = chart;
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            document.saved_locations().to_vec(),
            charts,
            document.chart_calculations().to_vec(),
            document.comparison_presets().to_vec(),
            document.workspace_state().clone(),
        )
    }

    /// Resolve a chart's local wall time, calculate it, and append an immutable snapshot.
    ///
    /// Recalculation never edits an earlier [`ChartCalculation`]. It appends a new
    /// calculation and artifact, then advances only the chart's current-result pointer.
    pub fn calculate_chart_definition(
        document: &VaultDocument,
        request: ChartCalculationRequest,
    ) -> Result<VaultDocument, AppError> {
        let chart = document
            .chart_definitions()
            .iter()
            .find(|chart| chart.id() == &request.chart_definition_id)
            .ok_or(AppError::NotFound("chart definition"))?;
        let location = document
            .saved_locations()
            .iter()
            .find(|location| location.id() == &request.saved_location_id)
            .ok_or(AppError::NotFound("saved location"))?;
        if chart.local_input().time_zone() != location.time_zone() {
            return Err(AppError::ChartLocationTimeZoneMismatch);
        }

        let resolved = select_local_time(chart.local_input(), request.ambiguous_time_choice)?;
        let options = astraeus_options(chart.calculation_options())?;
        let calculation_request = CalculationRequest::from_options(
            AstraeusInstant::parse_rfc3339(resolved.utc_instant())
                .map_err(|error| AppError::Astraeus(error.to_string()))?,
            GeographicLocation::new(
                location.latitude_degrees(),
                location.longitude_degrees(),
                location.elevation_meters().unwrap_or(0.0),
            )
            .map_err(|error| AppError::Astraeus(error.to_string()))?,
            options,
        );
        let result = SwissEphemerisAdapter::moshier()
            .calculate(&calculation_request)
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let artifact = CalculationArtifact::new(calculation_request, result)
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let artifact_json = artifact
            .to_json()
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let artifact_record = ArtifactRecord::from_astraeus_calculation(
            request.calculation_artifact_id.clone(),
            chart.person_id().cloned(),
            None,
            &artifact_json,
        )?;
        let calculation = ChartCalculation::new(
            request.chart_calculation_id.clone(),
            chart.id().clone(),
            chart.local_input().clone(),
            resolved,
            location.clone(),
            request.calculation_artifact_id,
            request.calculated_at,
        )?;

        let mut artifacts = document.artifacts().to_vec();
        artifacts.push(artifact_record);
        let mut calculations = document.chart_calculations().to_vec();
        calculations.push(calculation);
        let mut charts = document.chart_definitions().to_vec();
        charts
            .iter_mut()
            .find(|candidate| candidate.id() == &request.chart_definition_id)
            .expect("chart was resolved from the same document")
            .set_current_calculation(request.chart_calculation_id);
        rebuild_studio(
            document,
            artifacts,
            document.saved_locations().to_vec(),
            charts,
            calculations,
            document.comparison_presets().to_vec(),
            document.workspace_state().clone(),
        )
    }

    pub fn add_comparison_preset(
        document: &VaultDocument,
        preset: ComparisonPreset,
    ) -> Result<VaultDocument, AppError> {
        let mut presets = document.comparison_presets().to_vec();
        presets.push(preset);
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            document.saved_locations().to_vec(),
            document.chart_definitions().to_vec(),
            document.chart_calculations().to_vec(),
            presets,
            document.workspace_state().clone(),
        )
    }

    pub fn replace_comparison_preset(
        document: &VaultDocument,
        mut preset: ComparisonPreset,
    ) -> Result<VaultDocument, AppError> {
        let mut presets = document.comparison_presets().to_vec();
        let existing = presets
            .iter_mut()
            .find(|existing| existing.id() == preset.id())
            .ok_or(AppError::NotFound("comparison preset"))?;
        if preset.current_comparison_artifact_id().is_none()
            && let (Some(inner), Some(outer), Some(artifact)) = (
                existing.current_inner_calculation_id(),
                existing.current_outer_calculation_id(),
                existing.current_comparison_artifact_id(),
            )
        {
            preset.set_current_comparison(inner.clone(), outer.clone(), artifact.clone());
        }
        *existing = preset;
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            document.saved_locations().to_vec(),
            document.chart_definitions().to_vec(),
            document.chart_calculations().to_vec(),
            presets,
            document.workspace_state().clone(),
        )
    }

    /// Build a comparison from the exact immutable calculations currently selected by its charts.
    pub fn calculate_comparison(
        document: &VaultDocument,
        request: ComparisonCalculationRequest,
    ) -> Result<VaultDocument, AppError> {
        let preset = document
            .comparison_presets()
            .iter()
            .find(|preset| preset.id() == &request.comparison_preset_id)
            .ok_or(AppError::NotFound("comparison preset"))?;
        let inner_chart = chart_for(document, preset.inner_chart_definition_id())?;
        let outer_chart = chart_for(document, preset.outer_chart_definition_id())?;
        let inner_calculation = current_calculation(document, inner_chart)?;
        let outer_calculation = current_calculation(document, outer_chart)?;
        let inner_artifact = calculation_artifact(document, inner_calculation)?;
        let outer_artifact = calculation_artifact(document, outer_calculation)?;
        let aspects = astraeus_aspects(preset.aspects())?;
        let inner_points = astraeus_points(preset.inner_points())?;
        let outer_points = astraeus_points(preset.outer_points())?;
        let inner_specification = ChartSpecification::with_aspect_points(
            astraeus_options(inner_chart.calculation_options())?,
            aspects.clone(),
            inner_points.clone(),
        )
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let outer_specification = ChartSpecification::with_aspect_points(
            astraeus_options(outer_chart.calculation_options())?,
            aspects.clone(),
            outer_points.clone(),
        )
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let inner_derived = DerivedChartArtifact::new(inner_artifact, inner_specification)
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let outer_derived = DerivedChartArtifact::new(outer_artifact, outer_specification)
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let comparison_specification =
            if inner_chart.role() == ChartRole::Natal && outer_chart.role() == ChartRole::Natal {
                ComparisonSpecification::synastry(aspects, inner_points, outer_points)
            } else {
                ComparisonSpecification::moving_second(
                    comparison_kind(inner_chart.role(), outer_chart.role()),
                    aspects,
                    inner_points,
                    outer_points,
                )
            }
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let comparison =
            ComparisonArtifact::new(inner_derived, outer_derived, comparison_specification)
                .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let json = comparison
            .to_json()
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
        let artifact = ArtifactRecord::from_astraeus_comparison(
            request.comparison_artifact_id.clone(),
            None,
            None,
            &json,
        )?;

        let mut artifacts = document.artifacts().to_vec();
        artifacts.push(artifact);
        let mut presets = document.comparison_presets().to_vec();
        presets
            .iter_mut()
            .find(|candidate| candidate.id() == &request.comparison_preset_id)
            .expect("preset was resolved from the same document")
            .set_current_comparison(
                inner_calculation.id().clone(),
                outer_calculation.id().clone(),
                request.comparison_artifact_id,
            );
        rebuild_studio(
            document,
            artifacts,
            document.saved_locations().to_vec(),
            document.chart_definitions().to_vec(),
            document.chart_calculations().to_vec(),
            presets,
            document.workspace_state().clone(),
        )
    }

    pub fn set_workspace_state(
        document: &VaultDocument,
        workspace: WorkspaceState,
    ) -> Result<VaultDocument, AppError> {
        rebuild_studio(
            document,
            document.artifacts().to_vec(),
            document.saved_locations().to_vec(),
            document.chart_definitions().to_vec(),
            document.chart_calculations().to_vec(),
            document.comparison_presets().to_vec(),
            workspace,
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
            let pack_metadata = artifact
                .deck_pack_id()
                .zip(artifact.deck_pack_content_id())
                .map(|(pack_id, content_id)| {
                    format!("pack={} deck_content_id={}", pack_id.as_str(), content_id)
                });
            if pack_metadata
                .as_deref()
                .is_some_and(|metadata| metadata.to_lowercase().contains(&query))
            {
                push_hit(
                    &mut hits,
                    SearchEntity::Artifact,
                    artifact.id(),
                    &query,
                    [pack_metadata.as_deref(), None],
                );
            } else {
                push_hit(
                    &mut hits,
                    SearchEntity::Artifact,
                    artifact.id(),
                    &query,
                    [Some(artifact.canonical_json()), None],
                );
            }
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
    let deck_record = document
        .artifacts()
        .iter()
        .find(|record| record.id() == &request.deck_record_id)
        .ok_or(AppError::NotFound("deck artifact"))?;
    let pack_snapshot = deck_record
        .deck_pack_id()
        .zip(deck_record.deck_pack_content_id())
        .map(|(id, content_id)| (id.clone(), content_id.to_owned()));
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
    let mut record = ArtifactRecord::from_sibylla(
        request.artifact_record_id,
        request.person_id,
        request.session_id,
        &json,
    )?;
    if let Some((pack_id, content_id)) = pack_snapshot {
        record.snapshot_deck_pack(pack_id, content_id)?;
    }
    add_artifact(document, record)
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

fn chart_for<'a>(
    document: &'a VaultDocument,
    id: &StableId,
) -> Result<&'a ChartDefinition, AppError> {
    document
        .chart_definitions()
        .iter()
        .find(|chart| chart.id() == id)
        .ok_or(AppError::NotFound("chart definition"))
}

fn current_calculation<'a>(
    document: &'a VaultDocument,
    chart: &ChartDefinition,
) -> Result<&'a ChartCalculation, AppError> {
    let id = chart
        .current_calculation_id()
        .ok_or(AppError::MissingCurrentCalculation)?;
    document
        .chart_calculations()
        .iter()
        .find(|calculation| calculation.id() == id)
        .ok_or(AppError::NotFound("chart calculation"))
}

fn calculation_artifact(
    document: &VaultDocument,
    calculation: &ChartCalculation,
) -> Result<CalculationArtifact, AppError> {
    let record = document
        .artifacts()
        .iter()
        .find(|artifact| artifact.id() == calculation.calculation_artifact_id())
        .ok_or(AppError::NotFound("calculation artifact"))?;
    if record.kind() != ArtifactKind::AstraeusCalculation {
        return Err(AppError::ExpectedCalculation);
    }
    CalculationArtifact::from_json(record.canonical_json())
        .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn astraeus_options(options: &ChartCalculationOptions) -> Result<CalculationOptions, AppError> {
    CalculationOptions::new(
        options
            .ordered_objects()
            .iter()
            .copied()
            .map(astraeus_object)
            .collect(),
        match options.zodiac() {
            ZodiacId::Tropical => Zodiac::Tropical,
            ZodiacId::Sidereal => Zodiac::Sidereal,
        },
        options.ayanamsa().map(|ayanamsa| match ayanamsa {
            AyanamsaId::FaganBradley => Ayanamsa::FaganBradley,
            AyanamsaId::Lahiri => Ayanamsa::Lahiri,
            AyanamsaId::DeLuce => Ayanamsa::DeLuce,
            AyanamsaId::Raman => Ayanamsa::Raman,
            AyanamsaId::Krishnamurti => Ayanamsa::Krishnamurti,
            AyanamsaId::Yukteshwar => Ayanamsa::Yukteshwar,
            AyanamsaId::JnBhasin => Ayanamsa::JnBhasin,
        }),
        match options.house_system() {
            HouseSystemId::Placidus => HouseSystem::Placidus,
            HouseSystemId::Koch => HouseSystem::Koch,
            HouseSystemId::Porphyry => HouseSystem::Porphyry,
            HouseSystemId::Regiomontanus => HouseSystem::Regiomontanus,
            HouseSystemId::Campanus => HouseSystem::Campanus,
            HouseSystemId::Equal => HouseSystem::Equal,
            HouseSystemId::WholeSign => HouseSystem::WholeSign,
        },
    )
    .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn astraeus_object(object: CelestialObjectId) -> CelestialObject {
    match object {
        CelestialObjectId::Moon => CelestialObject::Moon,
        CelestialObjectId::Sun => CelestialObject::Sun,
        CelestialObjectId::Mercury => CelestialObject::Mercury,
        CelestialObjectId::Venus => CelestialObject::Venus,
        CelestialObjectId::Mars => CelestialObject::Mars,
        CelestialObjectId::Jupiter => CelestialObject::Jupiter,
        CelestialObjectId::Saturn => CelestialObject::Saturn,
        CelestialObjectId::Uranus => CelestialObject::Uranus,
        CelestialObjectId::Neptune => CelestialObject::Neptune,
        CelestialObjectId::Pluto => CelestialObject::Pluto,
        CelestialObjectId::MeanNode => CelestialObject::MeanNode,
        CelestialObjectId::TrueNode => CelestialObject::TrueNode,
        CelestialObjectId::Chiron => CelestialObject::Chiron,
    }
}

fn astraeus_point(point: ChartPointId) -> AstraeusChartPointId {
    match point {
        ChartPointId::Moon => AstraeusChartPointId::Moon,
        ChartPointId::Sun => AstraeusChartPointId::Sun,
        ChartPointId::Mercury => AstraeusChartPointId::Mercury,
        ChartPointId::Venus => AstraeusChartPointId::Venus,
        ChartPointId::Mars => AstraeusChartPointId::Mars,
        ChartPointId::Jupiter => AstraeusChartPointId::Jupiter,
        ChartPointId::Saturn => AstraeusChartPointId::Saturn,
        ChartPointId::Uranus => AstraeusChartPointId::Uranus,
        ChartPointId::Neptune => AstraeusChartPointId::Neptune,
        ChartPointId::Pluto => AstraeusChartPointId::Pluto,
        ChartPointId::MeanNode => AstraeusChartPointId::MeanNode,
        ChartPointId::TrueNode => AstraeusChartPointId::TrueNode,
        ChartPointId::Chiron => AstraeusChartPointId::Chiron,
        ChartPointId::MeanSouthNode => AstraeusChartPointId::MeanSouthNode,
        ChartPointId::TrueSouthNode => AstraeusChartPointId::TrueSouthNode,
        ChartPointId::Ascendant => AstraeusChartPointId::Ascendant,
        ChartPointId::Midheaven => AstraeusChartPointId::Midheaven,
        ChartPointId::Descendant => AstraeusChartPointId::Descendant,
        ChartPointId::ImumCoeli => AstraeusChartPointId::ImumCoeli,
        ChartPointId::Vertex => AstraeusChartPointId::Vertex,
    }
}

fn astraeus_points(points: &[ChartPointId]) -> Result<ChartPointSelection, AppError> {
    ChartPointSelection::new(points.iter().copied().map(astraeus_point).collect())
        .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn astraeus_aspects(aspects: &[AspectDefinition]) -> Result<AspectDefinitions, AppError> {
    let definitions = aspects
        .iter()
        .map(|aspect| {
            let kind = match aspect.kind() {
                AspectKindId::Conjunction => AstraeusAspectKind::Conjunction,
                AspectKindId::Opposition => AstraeusAspectKind::Opposition,
                AspectKindId::Square => AstraeusAspectKind::Square,
                AspectKindId::Trine => AstraeusAspectKind::Trine,
                AspectKindId::Sextile => AstraeusAspectKind::Sextile,
            };
            AstraeusAspectDefinition::new(kind, aspect.orb_degrees())
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    AspectDefinitions::new(definitions).map_err(|error| AppError::Astraeus(error.to_string()))
}

fn comparison_kind(inner: ChartRole, outer: ChartRole) -> ComparisonKind {
    match (inner, outer) {
        (ChartRole::Natal, ChartRole::Transit) => ComparisonKind::TransitToNatal,
        (ChartRole::Natal, ChartRole::Event) => ComparisonKind::EventToNatal,
        (ChartRole::Transit, ChartRole::Transit) => ComparisonKind::TransitToTransit,
        _ => ComparisonKind::Generic,
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_studio(
    document: &VaultDocument,
    artifacts: Vec<ArtifactRecord>,
    locations: Vec<SavedLocation>,
    charts: Vec<ChartDefinition>,
    calculations: Vec<ChartCalculation>,
    comparisons: Vec<ComparisonPreset>,
    workspace: WorkspaceState,
) -> Result<VaultDocument, AppError> {
    Ok(VaultDocument::with_studio_records(
        document.people().to_vec(),
        document.sessions().to_vec(),
        artifacts,
        document.journal_entries().to_vec(),
        locations,
        charts,
        calculations,
        comparisons,
        workspace,
    )?)
}

fn rebuild(
    document: &VaultDocument,
    people: Vec<PersonProfile>,
    sessions: Vec<Session>,
    artifacts: Vec<ArtifactRecord>,
    entries: Vec<JournalEntry>,
) -> Result<VaultDocument, AppError> {
    Ok(VaultDocument::with_studio_records(
        people,
        sessions,
        artifacts,
        entries,
        document.saved_locations().to_vec(),
        document.chart_definitions().to_vec(),
        document.chart_calculations().to_vec(),
        document.comparison_presets().to_vec(),
        document.workspace_state().clone(),
    )?)
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
    #[error("expected an Astraeus calculation artifact")]
    ExpectedCalculation,
    #[error("chart has no current calculation")]
    MissingCurrentCalculation,
    #[error("chart and saved location must use the same IANA time zone")]
    ChartLocationTimeZoneMismatch,
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
