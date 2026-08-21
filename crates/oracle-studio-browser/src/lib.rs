//! Browser-only state ownership and transactional persistence.

use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc};

use astraeus_moshier::MoshierEphemerisAdapter;
use oracle_studio_app::{
    ChartCalculationRequest, ComparisonCalculationRequest, PreparedWorkbenchPreview,
    WorkbenchCalculationRequest, calculate_chart, calculate_comparison,
    calculate_workbench_preview, commit_workbench_save_as, commit_workbench_update,
};
use oracle_studio_core::{
    PersonProfile, VaultDocument, generate_unique_id, resolve_local_time, select_local_time,
};
use oracle_studio_location_catalog::{CatalogInstallInput, CatalogMetadata, LocationCatalog};
use oracle_studio_platform::{
    ActiveWorkspace, CapabilityStatus, ChartSummary, EntitySummary, EphemerisStatus,
    PlatformCommand, PlatformError, PlatformErrorCode, PlatformResponse, PreviewCommitOutcome,
    PreviewGeneration, PreviewSaveMode, VaultLockState, VaultSummary, WheelTemplateSettings,
    WheelTemplateSettingsV1, WorkbenchChartSummary, WorkbenchPresentation, WorkbenchPreviewSource,
    WorkspaceSummary,
};
use oracle_studio_vault::{UnlockedVault, create, inspect, open, revision};
use serde::{Deserialize, Serialize};

pub const IDLE_LOCK_MILLIS: f64 = 15.0 * 60.0 * 1000.0;

type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + 'a>>;

pub trait BrowserStore {
    fn list_vaults(&self) -> StoreFuture<'_, Vec<VaultRecord>>;
    fn insert_vault(&self, record: VaultRecord) -> StoreFuture<'_, ()>;
    fn replace_vault(&self, record: VaultRecord, expected: String) -> StoreFuture<'_, ()>;
    fn delete_vault(&self, id: String) -> StoreFuture<'_, ()>;
    fn load_catalog(&self) -> StoreFuture<'_, Option<CatalogInstallInput>>;
    fn save_catalog(
        &self,
        input: CatalogInstallInput,
        metadata: CatalogMetadata,
    ) -> StoreFuture<'_, ()>;
    fn load_wheel_template_settings(&self) -> StoreFuture<'_, Option<String>>;
    fn save_wheel_template_settings(&self, settings: String) -> StoreFuture<'_, ()>;
    fn request_persistence(&self) -> StoreFuture<'_, bool>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultRecord {
    pub id: String,
    pub title: String,
    pub envelope: Vec<u8>,
    pub revision: String,
    pub created_at: String,
    pub modified_at: String,
}

impl VaultRecord {
    fn from_envelope(
        envelope: Vec<u8>,
        created_at: String,
        modified_at: String,
    ) -> Result<Self, PlatformError> {
        let header = inspect(&envelope).map_err(vault_error)?;
        Ok(Self {
            id: header.id().into(),
            title: header.title().into(),
            revision: revision(&envelope),
            envelope,
            created_at,
            modified_at,
        })
    }

    fn validate(&self) -> Result<(), StoreError> {
        let header =
            inspect(&self.envelope).map_err(|error| StoreError::Corrupt(error.to_string()))?;
        if header.id() != self.id
            || header.title() != self.title
            || revision(&self.envelope) != self.revision
        {
            return Err(StoreError::Corrupt("vault record metadata mismatch".into()));
        }
        Ok(())
    }
}

struct MountedVault {
    vault: UnlockedVault,
    last_access_millis: f64,
}

#[derive(Clone)]
struct PendingWorkbenchPreview {
    generation: PreviewGeneration,
    preview: PreparedWorkbenchPreview,
    source_vault_id: Option<String>,
    source_vault_revision: Option<String>,
}

pub struct BrowserStudioEngine {
    store: Rc<dyn BrowserStore>,
    loaded: bool,
    records: BTreeMap<String, VaultRecord>,
    mounted: BTreeMap<String, MountedVault>,
    scratch: Option<VaultDocument>,
    scratch_dirty: bool,
    active: Option<ActiveWorkspace>,
    catalog: Option<LocationCatalog>,
    persistence_requested: bool,
    persistence_granted: Option<bool>,
    wheel_templates: WheelTemplateSettings,
    pending_preview: Option<PendingWorkbenchPreview>,
}

impl BrowserStudioEngine {
    pub fn new(store: Rc<dyn BrowserStore>) -> Self {
        Self {
            store,
            loaded: false,
            records: BTreeMap::new(),
            mounted: BTreeMap::new(),
            scratch: None,
            scratch_dirty: false,
            active: None,
            catalog: None,
            persistence_requested: false,
            persistence_granted: None,
            wheel_templates: WheelTemplateSettings::default(),
            pending_preview: None,
        }
    }

    pub async fn execute(
        &mut self,
        command: PlatformCommand,
        now_millis: f64,
        now: String,
    ) -> Result<PlatformResponse, PlatformError> {
        self.lock_idle(now_millis);
        if !self.loaded && !matches!(command, PlatformCommand::Initialize) {
            self.initialize().await?;
        }
        match command {
            PlatformCommand::Initialize => {
                self.initialize().await?;
                Ok(PlatformResponse::Ready {
                    vaults: self.vault_summaries(),
                    workspace: self.workspace_summary(),
                    capabilities: self.capabilities(),
                    wheel_templates: self.wheel_templates.clone(),
                })
            }
            PlatformCommand::CreateScratch => {
                if self.scratch.is_none() {
                    self.scratch = Some(VaultDocument::empty());
                }
                self.active = Some(ActiveWorkspace::Scratch);
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::DiscardScratch { confirmed } => {
                if self.scratch_dirty && !confirmed {
                    return Err(error(
                        PlatformErrorCode::ConfirmationRequired,
                        "dirty scratch work requires explicit discard confirmation",
                    ));
                }
                self.scratch = None;
                self.scratch_dirty = false;
                if self.active == Some(ActiveWorkspace::Scratch) {
                    self.active = None;
                }
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::SaveScratch { title, password } => {
                let document = self.scratch.clone().ok_or_else(|| {
                    error(PlatformErrorCode::NotFound, "scratch workspace not found")
                })?;
                let (vault, envelope) = create(&title, &password, document).map_err(vault_error)?;
                let record = VaultRecord::from_envelope(envelope, now.clone(), now)?;
                self.store
                    .insert_vault(record.clone())
                    .await
                    .map_err(store_error)?;
                let id = record.id.clone();
                self.records.insert(id.clone(), record);
                self.mounted.insert(
                    id.clone(),
                    MountedVault {
                        vault,
                        last_access_millis: now_millis,
                    },
                );
                self.scratch = None;
                self.scratch_dirty = false;
                self.active = Some(ActiveWorkspace::Vault(id));
                self.pending_preview = None;
                self.request_persistence().await;
                Ok(self.updated())
            }
            PlatformCommand::ListVaults => Ok(PlatformResponse::Vaults(self.vault_summaries())),
            PlatformCommand::ImportVault {
                bytes,
                replace_confirmed,
            } => {
                let header = inspect(&bytes).map_err(vault_error)?;
                let id = header.id().to_owned();
                let record = VaultRecord::from_envelope(bytes, now.clone(), now)?;
                if let Some(existing) = self.records.get(&id) {
                    if !replace_confirmed {
                        return Err(error(
                            PlatformErrorCode::DuplicateVault,
                            "a vault with this public ID already exists; replacement requires confirmation",
                        ));
                    }
                    self.store
                        .replace_vault(record.clone(), existing.revision.clone())
                        .await
                        .map_err(store_error)?;
                    self.mounted.remove(&id);
                    if self.active == Some(ActiveWorkspace::Vault(id.clone())) {
                        self.active = None;
                    }
                } else {
                    self.store
                        .insert_vault(record.clone())
                        .await
                        .map_err(store_error)?;
                }
                self.records.insert(id, record);
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::ExportVault { vault_id } => {
                let record = self
                    .records
                    .get(&vault_id)
                    .ok_or_else(|| error(PlatformErrorCode::NotFound, "vault not found"))?;
                Ok(PlatformResponse::Export {
                    filename: format!("{}.oracle-vault", safe_filename(&record.title)),
                    bytes: record.envelope.clone(),
                })
            }
            PlatformCommand::UnlockVault { vault_id, password } => {
                let record = self
                    .records
                    .get(&vault_id)
                    .ok_or_else(|| error(PlatformErrorCode::NotFound, "vault not found"))?;
                let vault = open(&record.envelope, &password).map_err(vault_error)?;
                self.mounted.insert(
                    vault_id.clone(),
                    MountedVault {
                        vault,
                        last_access_millis: now_millis,
                    },
                );
                self.active = Some(ActiveWorkspace::Vault(vault_id));
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::LockVault { vault_id } | PlatformCommand::UnloadVault { vault_id } => {
                if self.mounted.remove(&vault_id).is_none() {
                    return Err(error(
                        PlatformErrorCode::NotFound,
                        "mounted vault not found",
                    ));
                }
                if self.active == Some(ActiveWorkspace::Vault(vault_id)) {
                    self.active = None;
                }
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::ActivateVault { vault_id } => {
                let mounted = self.mounted.get_mut(&vault_id).ok_or_else(|| {
                    error(
                        PlatformErrorCode::Locked,
                        "unlock the vault before activating it",
                    )
                })?;
                mounted.last_access_millis = now_millis;
                self.retain_pending_for_vault(&vault_id);
                self.active = Some(ActiveWorkspace::Vault(vault_id));
                Ok(self.updated())
            }
            PlatformCommand::RemoveVault {
                vault_id,
                confirmed,
            } => {
                if !confirmed {
                    return Err(error(
                        PlatformErrorCode::ConfirmationRequired,
                        "vault removal requires confirmation and does not replace portable backups",
                    ));
                }
                if !self.records.contains_key(&vault_id) {
                    return Err(error(PlatformErrorCode::NotFound, "vault not found"));
                }
                self.store
                    .delete_vault(vault_id.clone())
                    .await
                    .map_err(store_error)?;
                self.records.remove(&vault_id);
                self.mounted.remove(&vault_id);
                if self.active == Some(ActiveWorkspace::Vault(vault_id)) {
                    self.active = None;
                }
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::AddPerson {
                id,
                display_name,
                kind,
                notes,
            } => {
                let person =
                    PersonProfile::new(id, display_name, kind, notes).map_err(model_error)?;
                let document = self
                    .active_document()?
                    .clone()
                    .with_person(person)
                    .map_err(model_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::SaveLocation { location } => {
                let document = self
                    .active_document()?
                    .clone()
                    .with_location(location)
                    .map_err(model_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::SaveChart { chart } => {
                let document = self
                    .active_document()?
                    .clone()
                    .with_chart(chart)
                    .map_err(model_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::UpdateChartBasics {
                chart_id,
                label,
                role,
                local_input,
            } => {
                let existing = self
                    .active_document()?
                    .chart_definitions()
                    .iter()
                    .find(|chart| chart.id() == &chart_id)
                    .ok_or_else(|| error(PlatformErrorCode::NotFound, "chart was not found"))?;
                let chart = oracle_studio_core::ChartDefinition::new(
                    existing.id().clone(),
                    label,
                    role,
                    existing.person_id().cloned(),
                    local_input,
                    existing.calculation_options().clone(),
                    existing.ordered_points().to_vec(),
                    existing.default_natal(),
                )
                .map_err(model_error)?;
                let document = self
                    .active_document()?
                    .clone()
                    .with_chart(chart)
                    .map_err(model_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::ResolveLocalTime { input, choice } => {
                if choice.is_some() {
                    let selected = select_local_time(&input, choice).map_err(model_error)?;
                    Ok(PlatformResponse::LocalTime(
                        oracle_studio_core::LocalTimeResolution::Unique(selected),
                    ))
                } else {
                    Ok(PlatformResponse::LocalTime(
                        resolve_local_time(&input).map_err(model_error)?,
                    ))
                }
            }
            PlatformCommand::CalculateChart {
                id,
                chart_definition_id,
                saved_location_id,
                choice,
                calculated_at,
            } => {
                let document = calculate_chart(
                    self.active_document()?,
                    ChartCalculationRequest {
                        id,
                        chart_definition_id,
                        saved_location_id,
                        ambiguous_time_choice: choice,
                        calculated_at,
                    },
                    &MoshierEphemerisAdapter::new(),
                )
                .map_err(app_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::SaveComparison { preset } => {
                let document = self
                    .active_document()?
                    .clone()
                    .with_comparison(preset)
                    .map_err(model_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::CalculateComparison {
                id,
                comparison_preset_id,
                calculated_at,
            } => {
                let document = calculate_comparison(
                    self.active_document()?,
                    ComparisonCalculationRequest {
                        id,
                        comparison_preset_id,
                        calculated_at,
                    },
                )
                .map_err(app_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::WorkbenchPreview { request } => {
                let (source_vault_id, source_vault_title, source_vault_revision) =
                    self.preview_source()?;
                let prepared = calculate_workbench_preview(
                    self.active_document()?,
                    WorkbenchCalculationRequest {
                        inner_chart_definition_id: request.inner_chart_definition_id,
                        outer_chart_definition_id: request.outer_chart_definition_id,
                        inner_saved_location_id: request.inner_saved_location_id,
                        outer_saved_location_id: request.outer_saved_location_id,
                        outer_local_input: request.outer_local_input,
                        outer_ambiguous_time_choice: request.outer_ambiguous_time_choice,
                    },
                    &MoshierEphemerisAdapter::new(),
                )
                .map_err(app_error)?;
                let presentation = workbench_presentation(
                    request.generation,
                    &prepared,
                    request.adjustment_notice,
                    source_vault_id.clone(),
                    source_vault_title.clone(),
                    source_vault_revision.clone(),
                );
                self.pending_preview = Some(PendingWorkbenchPreview {
                    generation: request.generation,
                    preview: prepared,
                    source_vault_id,
                    source_vault_revision,
                });
                Ok(PlatformResponse::WorkbenchPreview(presentation))
            }
            PlatformCommand::CommitWorkbenchPreview {
                generation,
                save_mode,
            } => {
                let pending = self.pending_preview.clone().ok_or_else(|| {
                        error(
                            PlatformErrorCode::StalePreview,
                            "the unsaved preview is no longer available; return to Workbench and calculate it again",
                        )
                    })?;
                if pending.generation != generation {
                    return Err(error(
                        PlatformErrorCode::StalePreview,
                        "the workbench preview is stale; wait for the newest calculation",
                    ));
                }
                let Some(source_vault_id) = pending.source_vault_id.as_ref() else {
                    return Err(error(
                        PlatformErrorCode::InvalidInput,
                        "save the scratch workspace as an encrypted vault before committing its preview",
                    ));
                };
                let Some(source_revision) = pending.source_vault_revision.as_ref() else {
                    return Err(error(
                        PlatformErrorCode::Internal,
                        "the pending vault preview has no source revision",
                    ));
                };
                let active_matches = self.active
                    == Some(ActiveWorkspace::Vault(source_vault_id.clone()))
                    && self.mounted.contains_key(source_vault_id)
                    && self
                        .records
                        .get(source_vault_id)
                        .is_some_and(|record| record.revision == *source_revision);
                if !active_matches {
                    self.pending_preview = None;
                    return Err(error(
                        PlatformErrorCode::StalePreview,
                        "the active vault or its revision changed; the unsaved preview was invalidated",
                    ));
                }
                let document = self.active_document()?;
                let calculation_ids = document
                    .chart_calculations()
                    .iter()
                    .map(|calculation| calculation.id().as_str().to_owned())
                    .collect();
                let calculation_id = generate_unique_id(
                    "chart-calculation",
                    &format!("{} calculation", pending.preview.outer.definition.label()),
                    &calculation_ids,
                )
                .map_err(model_error)?;
                let (updated, outcome) = match save_mode {
                    PreviewSaveMode::UpdateChart { confirmed } => {
                        if !confirmed {
                            return Err(error(
                                PlatformErrorCode::ConfirmationRequired,
                                "updating the existing chart requires explicit confirmation",
                            ));
                        }
                        let chart_id = pending.preview.outer.definition.id().as_str().to_owned();
                        let label = pending.preview.outer.definition.label().to_owned();
                        (
                            commit_workbench_update(
                                document,
                                &pending.preview,
                                calculation_id,
                                now.clone(),
                            ),
                            PreviewCommitOutcome::Updated { chart_id, label },
                        )
                    }
                    PreviewSaveMode::SaveAs { name } => {
                        let name = name.trim();
                        if name.is_empty() {
                            return Err(error(
                                PlatformErrorCode::InvalidInput,
                                "Save As requires a new chart name",
                            ));
                        }
                        if document
                            .chart_definitions()
                            .iter()
                            .any(|chart| chart.label().trim().to_lowercase() == name.to_lowercase())
                        {
                            return Err(error(
                                PlatformErrorCode::Conflict,
                                "a chart with that name already exists; Save As never overwrites",
                            ));
                        }
                        let chart_ids = document
                            .chart_definitions()
                            .iter()
                            .map(|chart| chart.id().as_str().to_owned())
                            .collect();
                        let chart_id =
                            generate_unique_id("chart", name, &chart_ids).map_err(model_error)?;
                        let outcome = PreviewCommitOutcome::SavedAs {
                            chart_id: chart_id.as_str().to_owned(),
                            label: name.to_owned(),
                        };
                        (
                            commit_workbench_save_as(
                                document,
                                &pending.preview,
                                chart_id,
                                name.to_owned(),
                                calculation_id,
                                now.clone(),
                            ),
                            outcome,
                        )
                    }
                };
                let updated = updated.map_err(app_error)?;
                if let Err(commit_error) = self.commit_document(updated, now_millis, now).await {
                    if commit_error.code == PlatformErrorCode::Conflict {
                        self.pending_preview = None;
                        return Err(error(
                            PlatformErrorCode::StalePreview,
                            "the vault revision changed during persistence; the unsaved preview was invalidated",
                        ));
                    }
                    return Err(commit_error);
                }
                self.pending_preview = None;
                Ok(PlatformResponse::WorkbenchPreviewCommitted {
                    vaults: self.vault_summaries(),
                    workspace: self.workspace_summary(),
                    outcome,
                })
            }
            PlatformCommand::SaveWheelTemplate { template } => {
                template.validate()?;
                if template.is_protected() {
                    return Err(error(
                        PlatformErrorCode::InvalidInput,
                        "protected Oracle wheel templates are immutable; duplicate one to customize it",
                    ));
                }
                if let Some(existing) = self
                    .wheel_templates
                    .templates
                    .iter_mut()
                    .find(|existing| existing.id == template.id)
                {
                    *existing = template.clone();
                } else {
                    self.wheel_templates.templates.push(template.clone());
                }
                self.wheel_templates.last_selected_template_id = template.id;
                self.persist_wheel_templates().await?;
                Ok(PlatformResponse::WheelTemplates(
                    self.wheel_templates.clone(),
                ))
            }
            PlatformCommand::SelectWheelTemplate { template_id } => {
                if !self
                    .wheel_templates
                    .templates
                    .iter()
                    .any(|template| template.id == template_id)
                {
                    return Err(error(
                        PlatformErrorCode::NotFound,
                        "wheel template was not found",
                    ));
                }
                self.wheel_templates.last_selected_template_id = template_id;
                self.persist_wheel_templates().await?;
                Ok(PlatformResponse::WheelTemplates(
                    self.wheel_templates.clone(),
                ))
            }
            PlatformCommand::RemoveWheelTemplate { template_id } => {
                if self
                    .wheel_templates
                    .templates
                    .iter()
                    .any(|template| template.id == template_id && template.is_protected())
                {
                    return Err(error(
                        PlatformErrorCode::InvalidInput,
                        "protected Oracle wheel templates cannot be removed",
                    ));
                }
                if self.wheel_templates.templates.len() == 1 {
                    return Err(error(
                        PlatformErrorCode::InvalidInput,
                        "at least one wheel template must remain",
                    ));
                }
                let before = self.wheel_templates.templates.len();
                self.wheel_templates
                    .templates
                    .retain(|template| template.id != template_id);
                if before == self.wheel_templates.templates.len() {
                    return Err(error(
                        PlatformErrorCode::NotFound,
                        "wheel template was not found",
                    ));
                }
                if self.wheel_templates.last_selected_template_id == template_id {
                    self.wheel_templates.last_selected_template_id =
                        self.wheel_templates.templates[0].id.clone();
                }
                self.persist_wheel_templates().await?;
                Ok(PlatformResponse::WheelTemplates(
                    self.wheel_templates.clone(),
                ))
            }
            PlatformCommand::SetWorkspace { workspace } => {
                let document = self
                    .active_document()?
                    .clone()
                    .with_workspace(workspace)
                    .map_err(model_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::InstallCatalog { input } => {
                let catalog =
                    LocationCatalog::from_distribution(&input).map_err(|catalog_error| {
                        error(PlatformErrorCode::InvalidInput, catalog_error.to_string())
                    })?;
                let metadata = catalog.metadata().clone();
                self.store
                    .save_catalog(input, metadata.clone())
                    .await
                    .map_err(store_error)?;
                self.catalog = Some(catalog);
                self.request_persistence().await;
                Ok(PlatformResponse::CatalogInstalled(metadata))
            }
            PlatformCommand::InstallPinnedCatalog => Err(error(
                PlatformErrorCode::Internal,
                "same-origin catalog installation must be resolved by the browser worker",
            )),
            PlatformCommand::SearchCatalog { query, limit } => {
                let catalog = self.catalog.as_ref().ok_or_else(|| error(PlatformErrorCode::NotFound, "no GeoNames catalog is installed; upload files or use manual location entry"))?;
                Ok(PlatformResponse::CatalogResults(
                    catalog.search(&query, limit).map_err(|catalog_error| {
                        error(PlatformErrorCode::InvalidInput, catalog_error.to_string())
                    })?,
                ))
            }
            PlatformCommand::Touch => {
                self.touch_active(now_millis);
                Ok(self.updated())
            }
        }
    }

    async fn initialize(&mut self) -> Result<(), PlatformError> {
        if self.loaded {
            return Ok(());
        }
        let records = self.store.list_vaults().await.map_err(store_error)?;
        for record in records {
            record.validate().map_err(store_error)?;
            if self.records.insert(record.id.clone(), record).is_some() {
                return Err(error(
                    PlatformErrorCode::Storage,
                    "duplicate vault ID in IndexedDB",
                ));
            }
        }
        if let Some(input) = self.store.load_catalog().await.map_err(store_error)? {
            self.catalog = Some(LocationCatalog::from_distribution(&input).map_err(
                |catalog_error| error(PlatformErrorCode::Storage, catalog_error.to_string()),
            )?);
        }
        if let Some(raw) = self
            .store
            .load_wheel_template_settings()
            .await
            .map_err(store_error)?
        {
            if let Ok(settings) = serde_json::from_str::<WheelTemplateSettings>(&raw)
                && settings.validate().is_ok()
            {
                self.wheel_templates = settings;
            } else if let Ok(legacy) = serde_json::from_str::<WheelTemplateSettingsV1>(&raw)
                && let Ok(settings) = legacy.migrate()
            {
                let migrated = serde_json::to_string(&settings).map_err(|serialization_error| {
                    error(
                        PlatformErrorCode::Internal,
                        format!(
                            "could not serialize migrated wheel templates: {serialization_error}"
                        ),
                    )
                })?;
                self.store
                    .save_wheel_template_settings(migrated)
                    .await
                    .map_err(store_error)?;
                self.wheel_templates = settings;
            }
        }
        self.loaded = true;
        Ok(())
    }

    fn active_document(&self) -> Result<&VaultDocument, PlatformError> {
        match self.active.as_ref() {
            Some(ActiveWorkspace::Scratch) => self.scratch.as_ref().ok_or_else(|| {
                error(
                    PlatformErrorCode::Internal,
                    "active scratch workspace is missing",
                )
            }),
            Some(ActiveWorkspace::Vault(id)) => self
                .mounted
                .get(id)
                .map(|mounted| mounted.vault.document())
                .ok_or_else(|| error(PlatformErrorCode::Locked, "active vault is locked")),
            None => Err(error(
                PlatformErrorCode::NotFound,
                "no workspace is active; create scratch work or unlock a vault",
            )),
        }
    }

    fn preview_source(&self) -> Result<(Option<String>, String, Option<String>), PlatformError> {
        match self.active.as_ref() {
            Some(ActiveWorkspace::Scratch) => Ok((None, "Scratch workspace".into(), None)),
            Some(ActiveWorkspace::Vault(id)) => {
                if !self.mounted.contains_key(id) {
                    return Err(error(PlatformErrorCode::Locked, "active vault is locked"));
                }
                let record = self.records.get(id).ok_or_else(|| {
                    error(PlatformErrorCode::Storage, "active vault record is missing")
                })?;
                Ok((
                    Some(id.clone()),
                    record.title.clone(),
                    Some(record.revision.clone()),
                ))
            }
            None => Err(error(
                PlatformErrorCode::NotFound,
                "no workspace is active; create scratch work or unlock a vault",
            )),
        }
    }

    fn retain_pending_for_vault(&mut self, vault_id: &str) {
        let current_revision = self.records.get(vault_id).map(|record| &record.revision);
        let keep = self.pending_preview.as_ref().is_some_and(|pending| {
            pending.source_vault_id.as_deref() == Some(vault_id)
                && pending.source_vault_revision.as_ref() == current_revision
        });
        if !keep {
            self.pending_preview = None;
        }
    }

    async fn commit_document(
        &mut self,
        document: VaultDocument,
        now_millis: f64,
        now: String,
    ) -> Result<(), PlatformError> {
        match self.active.clone() {
            Some(ActiveWorkspace::Scratch) => {
                self.scratch = Some(document);
                self.scratch_dirty = true;
            }
            Some(ActiveWorkspace::Vault(id)) => {
                let mounted = self
                    .mounted
                    .get(&id)
                    .ok_or_else(|| error(PlatformErrorCode::Locked, "active vault is locked"))?;
                let envelope = mounted
                    .vault
                    .seal_document(&document)
                    .map_err(vault_error)?;
                let current = self.records.get(&id).ok_or_else(|| {
                    error(PlatformErrorCode::Storage, "active vault record is missing")
                })?;
                let expected = current.revision.clone();
                let record = VaultRecord {
                    id: current.id.clone(),
                    title: current.title.clone(),
                    revision: revision(&envelope),
                    envelope,
                    created_at: current.created_at.clone(),
                    modified_at: now,
                };
                self.store
                    .replace_vault(record.clone(), expected)
                    .await
                    .map_err(store_error)?;
                self.records.insert(id.clone(), record);
                let mounted = self
                    .mounted
                    .get_mut(&id)
                    .expect("mounted vault checked before persistence");
                mounted
                    .vault
                    .replace_document(document)
                    .map_err(vault_error)?;
                mounted.last_access_millis = now_millis;
            }
            None => return Err(error(PlatformErrorCode::NotFound, "no workspace is active")),
        }
        self.pending_preview = None;
        Ok(())
    }

    fn lock_idle(&mut self, now: f64) {
        let expired = self
            .mounted
            .iter()
            .filter(|(_, mounted)| now - mounted.last_access_millis >= IDLE_LOCK_MILLIS)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            self.mounted.remove(&id);
            if self
                .pending_preview
                .as_ref()
                .and_then(|pending| pending.source_vault_id.as_ref())
                == Some(&id)
            {
                self.pending_preview = None;
            }
            if self.active == Some(ActiveWorkspace::Vault(id)) {
                self.active = None;
            }
        }
    }

    fn touch_active(&mut self, now: f64) {
        if let Some(ActiveWorkspace::Vault(id)) = self.active.as_ref()
            && let Some(mounted) = self.mounted.get_mut(id)
        {
            mounted.last_access_millis = now;
        }
    }

    async fn request_persistence(&mut self) {
        if !self.persistence_requested {
            self.persistence_requested = true;
            self.persistence_granted = self.store.request_persistence().await.ok();
        }
    }

    async fn persist_wheel_templates(&self) -> Result<(), PlatformError> {
        self.wheel_templates.validate()?;
        let raw = serde_json::to_string(&self.wheel_templates).map_err(|serialization_error| {
            error(PlatformErrorCode::Internal, serialization_error.to_string())
        })?;
        self.store
            .save_wheel_template_settings(raw)
            .await
            .map_err(store_error)
    }

    fn updated(&self) -> PlatformResponse {
        PlatformResponse::Updated {
            vaults: self.vault_summaries(),
            workspace: self.workspace_summary(),
        }
    }

    fn vault_summaries(&self) -> Vec<VaultSummary> {
        self.records
            .values()
            .map(|record| VaultSummary {
                id: record.id.clone(),
                title: record.title.clone(),
                revision: record.revision.clone(),
                created_at: record.created_at.clone(),
                modified_at: record.modified_at.clone(),
                lock_state: if self.active == Some(ActiveWorkspace::Vault(record.id.clone())) {
                    VaultLockState::Active
                } else if self.mounted.contains_key(&record.id) {
                    VaultLockState::Mounted
                } else {
                    VaultLockState::Locked
                },
            })
            .collect()
    }

    fn workspace_summary(&self) -> WorkspaceSummary {
        let document = self.active_document().ok();
        WorkspaceSummary {
            active: self.active.clone(),
            scratch_dirty: self.scratch_dirty,
            people: document.map_or_else(Vec::new, |document| {
                document
                    .people()
                    .iter()
                    .map(|person| EntitySummary {
                        id: person.id().as_str().into(),
                        label: person.display_name().into(),
                    })
                    .collect()
            }),
            locations: document.map_or_else(Vec::new, |document| {
                document
                    .saved_locations()
                    .iter()
                    .map(|location| EntitySummary {
                        id: location.id().as_str().into(),
                        label: location.label().into(),
                    })
                    .collect()
            }),
            charts: document.map_or_else(Vec::new, |document| {
                document
                    .chart_definitions()
                    .iter()
                    .map(|chart| ChartSummary {
                        id: chart.id().as_str().into(),
                        label: chart.label().into(),
                        role: format!("{:?}", chart.role()).to_lowercase(),
                        local_input: format!(
                            "{} {} {}",
                            chart.local_input().local_date(),
                            chart.local_input().local_time(),
                            chart.local_input().time_zone()
                        ),
                        local_date: chart.local_input().local_date().into(),
                        local_time: chart.local_input().local_time().into(),
                        time_zone: chart.local_input().time_zone().into(),
                        current_calculation_id: chart
                            .current_calculation_id()
                            .map(|id| id.as_str().into()),
                    })
                    .collect()
            }),
            comparisons: document.map_or_else(Vec::new, |document| {
                document
                    .comparison_presets()
                    .iter()
                    .map(|preset| EntitySummary {
                        id: preset.id().as_str().into(),
                        label: preset.label().into(),
                    })
                    .collect()
            }),
        }
    }

    fn capabilities(&self) -> CapabilityStatus {
        CapabilityStatus {
            ephemeris: EphemerisStatus::Moshier,
            catalog: self.catalog.as_ref().map(|catalog| catalog.metadata().clone()),
            persistence_requested: self.persistence_requested,
            persistence_granted: self.persistence_granted,
            backup_warning: "Browser eviction or profile deletion can remove local vaults. Export portable .oracle-vault backups regularly.".into(),
        }
    }
}

fn workbench_presentation(
    generation: PreviewGeneration,
    preview: &PreparedWorkbenchPreview,
    adjustment_notice: Option<String>,
    source_vault_id: Option<String>,
    source_vault_title: String,
    source_vault_revision: Option<String>,
) -> WorkbenchPresentation {
    let summary = |preview: &oracle_studio_app::PreparedChartPreview| WorkbenchChartSummary {
        id: preview.definition.id().as_str().into(),
        label: preview.definition.label().into(),
        role: format!("{:?}", preview.definition.role()).to_lowercase(),
        local_input: preview.local_input.clone(),
        location_label: preview.location.label().into(),
        zodiac: format!("{:?}", preview.definition.calculation_options().zodiac()),
        house_system: format!(
            "{:?}",
            preview.definition.calculation_options().house_system()
        ),
        utc_offset_seconds: preview.resolved_time.utc_offset_seconds(),
    };
    WorkbenchPresentation {
        generation,
        source: Box::new(WorkbenchPreviewSource {
            vault_id: source_vault_id,
            vault_title: source_vault_title,
            vault_revision: source_vault_revision,
        }),
        inner: summary(&preview.inner),
        outer: summary(&preview.outer),
        scene: preview.scene.clone(),
        adjustment_notice,
    }
}

fn safe_filename(title: &str) -> String {
    let value = title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let value = value.trim_matches('-').chars().take(80).collect::<String>();
    let value = value
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if value.is_empty() {
        "oracle-vault".into()
    } else {
        value
    }
}

fn error(code: PlatformErrorCode, message: impl Into<String>) -> PlatformError {
    PlatformError::new(code, message)
}
fn model_error(error_: oracle_studio_core::ModelError) -> PlatformError {
    error(PlatformErrorCode::InvalidInput, error_.to_string())
}
fn app_error(error_: oracle_studio_app::AppError) -> PlatformError {
    let code = if matches!(error_, oracle_studio_app::AppError::ProviderUnavailable(_)) {
        PlatformErrorCode::ProviderUnavailable
    } else {
        PlatformErrorCode::InvalidInput
    };
    error(code, error_.to_string())
}
fn vault_error(error_: oracle_studio_vault::VaultError) -> PlatformError {
    use oracle_studio_vault::VaultError;
    let code = match error_ {
        VaultError::Authentication => PlatformErrorCode::Authentication,
        VaultError::UnsupportedVersion(_)
        | VaultError::UnsupportedAlgorithms
        | VaultError::UnsupportedKdfParameters => PlatformErrorCode::UnsupportedFormat,
        _ => PlatformErrorCode::InvalidInput,
    };
    error(code, error_.to_string())
}
fn store_error(error_: StoreError) -> PlatformError {
    let code = match error_ {
        StoreError::Conflict => PlatformErrorCode::Conflict,
        StoreError::Duplicate => PlatformErrorCode::DuplicateVault,
        _ => PlatformErrorCode::Storage,
    };
    error(code, error_.to_string())
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum StoreError {
    #[error("record already exists")]
    Duplicate,
    #[error("transaction revision conflict")]
    Conflict,
    #[error("record not found")]
    NotFound,
    #[error("stored data is corrupt: {0}")]
    Corrupt(String),
    #[error("browser storage failed: {0}")]
    Browser(String),
}

#[cfg(target_arch = "wasm32")]
mod indexed_db;
#[cfg(target_arch = "wasm32")]
pub use indexed_db::IndexedDbStore;

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        future::ready,
    };

    #[derive(Default)]
    pub struct MemoryStore {
        vaults: RefCell<BTreeMap<String, VaultRecord>>,
        catalog: RefCell<Option<CatalogInstallInput>>,
        wheel_template_settings: RefCell<Option<String>>,
        deny_persistence: Cell<bool>,
    }

    impl MemoryStore {
        pub fn deny_persistence(&self) {
            self.deny_persistence.set(true);
        }
        pub fn force_record(&self, record: VaultRecord) {
            self.vaults.borrow_mut().insert(record.id.clone(), record);
        }
        pub fn force_wheel_template_settings(&self, settings: impl Into<String>) {
            *self.wheel_template_settings.borrow_mut() = Some(settings.into());
        }
        pub fn wheel_template_settings(&self) -> Option<String> {
            self.wheel_template_settings.borrow().clone()
        }
    }

    impl BrowserStore for MemoryStore {
        fn list_vaults(&self) -> StoreFuture<'_, Vec<VaultRecord>> {
            Box::pin(ready(Ok(self.vaults.borrow().values().cloned().collect())))
        }
        fn insert_vault(&self, record: VaultRecord) -> StoreFuture<'_, ()> {
            Box::pin(ready(if self.vaults.borrow().contains_key(&record.id) {
                Err(StoreError::Duplicate)
            } else {
                self.vaults.borrow_mut().insert(record.id.clone(), record);
                Ok(())
            }))
        }
        fn replace_vault(&self, record: VaultRecord, expected: String) -> StoreFuture<'_, ()> {
            let existing_revision = self
                .vaults
                .borrow()
                .get(&record.id)
                .map(|existing| existing.revision.clone());
            Box::pin(ready(match existing_revision {
                Some(existing) if existing == expected => {
                    self.vaults.borrow_mut().insert(record.id.clone(), record);
                    Ok(())
                }
                Some(_) => Err(StoreError::Conflict),
                None => Err(StoreError::NotFound),
            }))
        }
        fn delete_vault(&self, id: String) -> StoreFuture<'_, ()> {
            Box::pin(ready(
                self.vaults
                    .borrow_mut()
                    .remove(&id)
                    .map(|_| ())
                    .ok_or(StoreError::NotFound),
            ))
        }
        fn load_catalog(&self) -> StoreFuture<'_, Option<CatalogInstallInput>> {
            Box::pin(ready(Ok(self.catalog.borrow().clone())))
        }
        fn save_catalog(
            &self,
            input: CatalogInstallInput,
            _metadata: CatalogMetadata,
        ) -> StoreFuture<'_, ()> {
            *self.catalog.borrow_mut() = Some(input);
            Box::pin(ready(Ok(())))
        }
        fn load_wheel_template_settings(&self) -> StoreFuture<'_, Option<String>> {
            Box::pin(ready(Ok(self.wheel_template_settings.borrow().clone())))
        }
        fn save_wheel_template_settings(&self, settings: String) -> StoreFuture<'_, ()> {
            *self.wheel_template_settings.borrow_mut() = Some(settings);
            Box::pin(ready(Ok(())))
        }
        fn request_persistence(&self) -> StoreFuture<'_, bool> {
            Box::pin(ready(Ok(!self.deny_persistence.get())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{testing::MemoryStore, *};
    use futures::executor::block_on;
    use oracle_studio_core::{
        ChartCalculationOptions, ChartDefinition, ChartRole, LocalDateTimeInput,
        LocationProvenance, SavedLocation, StableId, default_chart_points,
    };
    use oracle_studio_platform::{PreviewCommitOutcome, WorkbenchPreviewRequest};

    fn run(
        engine: &mut BrowserStudioEngine,
        command: PlatformCommand,
        millis: f64,
    ) -> Result<PlatformResponse, PlatformError> {
        block_on(engine.execute(command, millis, "2026-08-19T12:00:00Z".into()))
    }

    fn test_id(value: &str) -> StableId {
        StableId::new("browser-test.id", value).unwrap()
    }

    fn populated_vault() -> (
        Rc<MemoryStore>,
        BrowserStudioEngine,
        String,
        WorkbenchPreviewRequest,
    ) {
        let location = SavedLocation::new(
            test_id("fictional_harbor"),
            "Fictional Harbor",
            Vec::new(),
            "US",
            40.0,
            -75.0,
            None,
            "America/New_York",
            LocationProvenance::Manual,
        )
        .unwrap();
        let chart = |id: &str, label: &str, role, date: &str| {
            ChartDefinition::new(
                test_id(id),
                label,
                role,
                None,
                LocalDateTimeInput::new(date, "12:00:00", "America/New_York").unwrap(),
                ChartCalculationOptions::default(),
                default_chart_points(),
                false,
            )
            .unwrap()
        };
        let document = VaultDocument::empty()
            .with_location(location)
            .unwrap()
            .with_chart(chart(
                "fictional_natal",
                "Fictional Natal",
                ChartRole::Natal,
                "2000-01-15",
            ))
            .unwrap()
            .with_chart(chart(
                "fictional_transit",
                "Fictional Transit",
                ChartRole::Transit,
                "2026-08-17",
            ))
            .unwrap();
        let (_, envelope) = create("Chart Actions", b"fictional password", document).unwrap();
        let record = VaultRecord::from_envelope(
            envelope,
            "2026-08-19T12:00:00Z".into(),
            "2026-08-19T12:00:00Z".into(),
        )
        .unwrap();
        let vault_id = record.id.clone();
        let store = Rc::new(MemoryStore::default());
        store.force_record(record);
        let mut engine = BrowserStudioEngine::new(store.clone());
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        run(
            &mut engine,
            PlatformCommand::UnlockVault {
                vault_id: vault_id.clone(),
                password: b"fictional password".to_vec(),
            },
            1.0,
        )
        .unwrap();
        let request = WorkbenchPreviewRequest {
            generation: PreviewGeneration::new(7),
            inner_chart_definition_id: test_id("fictional_natal"),
            outer_chart_definition_id: test_id("fictional_transit"),
            inner_saved_location_id: test_id("fictional_harbor"),
            outer_saved_location_id: test_id("fictional_harbor"),
            outer_local_input: LocalDateTimeInput::new(
                "2026-08-18",
                "12:00:00",
                "America/New_York",
            )
            .unwrap(),
            outer_ambiguous_time_choice: None,
            adjustment_notice: None,
        };
        (store, engine, vault_id, request)
    }

    fn calculate_pending(engine: &mut BrowserStudioEngine, request: &WorkbenchPreviewRequest) {
        let response = run(
            engine,
            PlatformCommand::WorkbenchPreview {
                request: request.clone(),
            },
            2.0,
        )
        .unwrap();
        let PlatformResponse::WorkbenchPreview(presentation) = response else {
            panic!("workbench preview response")
        };
        assert_eq!(
            presentation.source.vault_id,
            engine.active.as_ref().and_then(|active| {
                match active {
                    ActiveWorkspace::Vault(id) => Some(id.clone()),
                    ActiveWorkspace::Scratch => None,
                }
            })
        );
        assert_eq!(
            presentation.source.vault_revision.as_deref(),
            presentation
                .source
                .vault_id
                .as_ref()
                .and_then(|id| engine.records.get(id))
                .map(|record| record.revision.as_str())
        );
    }

    #[test]
    fn files_update_requires_confirmation_and_preserves_chart_identity() {
        let (_store, mut engine, _vault_id, request) = populated_vault();
        calculate_pending(&mut engine, &request);

        let rejected = run(
            &mut engine,
            PlatformCommand::CommitWorkbenchPreview {
                generation: request.generation,
                save_mode: PreviewSaveMode::UpdateChart { confirmed: false },
            },
            3.0,
        );
        assert!(matches!(
            rejected,
            Err(PlatformError {
                code: PlatformErrorCode::ConfirmationRequired,
                ..
            })
        ));
        assert!(engine.pending_preview.is_some());

        let response = run(
            &mut engine,
            PlatformCommand::CommitWorkbenchPreview {
                generation: request.generation,
                save_mode: PreviewSaveMode::UpdateChart { confirmed: true },
            },
            4.0,
        )
        .unwrap();
        let PlatformResponse::WorkbenchPreviewCommitted {
            workspace, outcome, ..
        } = response
        else {
            panic!("preview commit response")
        };
        assert_eq!(
            outcome,
            PreviewCommitOutcome::Updated {
                chart_id: "fictional_transit".into(),
                label: "Fictional Transit".into(),
            }
        );
        assert_eq!(workspace.charts.len(), 2);
        let updated = workspace
            .charts
            .iter()
            .find(|chart| chart.id == "fictional_transit")
            .unwrap();
        assert_eq!(updated.local_date, "2026-08-18");
        assert!(updated.current_calculation_id.is_some());
        assert!(engine.pending_preview.is_none());
    }

    #[test]
    fn files_save_as_rejects_case_insensitive_collision_and_never_overwrites() {
        let (_store, mut engine, _vault_id, request) = populated_vault();
        calculate_pending(&mut engine, &request);

        let collision = run(
            &mut engine,
            PlatformCommand::CommitWorkbenchPreview {
                generation: request.generation,
                save_mode: PreviewSaveMode::SaveAs {
                    name: "  fictional transit  ".into(),
                },
            },
            3.0,
        );
        assert!(matches!(
            collision,
            Err(PlatformError {
                code: PlatformErrorCode::Conflict,
                ..
            })
        ));
        assert!(engine.pending_preview.is_some());
        assert_eq!(
            engine.active_document().unwrap().chart_definitions().len(),
            2
        );

        let response = run(
            &mut engine,
            PlatformCommand::CommitWorkbenchPreview {
                generation: request.generation,
                save_mode: PreviewSaveMode::SaveAs {
                    name: "  Saved Transit  ".into(),
                },
            },
            4.0,
        )
        .unwrap();
        let PlatformResponse::WorkbenchPreviewCommitted {
            workspace, outcome, ..
        } = response
        else {
            panic!("preview commit response")
        };
        assert_eq!(
            outcome,
            PreviewCommitOutcome::SavedAs {
                chart_id: "saved-transit".into(),
                label: "Saved Transit".into(),
            }
        );
        assert_eq!(workspace.charts.len(), 3);
        assert_eq!(
            workspace
                .charts
                .iter()
                .filter(|chart| chart.label == "Fictional Transit")
                .count(),
            1
        );
        let saved = workspace
            .charts
            .iter()
            .find(|chart| chart.label == "Saved Transit")
            .unwrap();
        assert_eq!(saved.local_date, "2026-08-18");
        assert!(saved.current_calculation_id.is_some());
    }

    #[test]
    fn locking_switching_and_reload_invalidate_pending_preview() {
        let (store, mut engine, vault_id, request) = populated_vault();
        calculate_pending(&mut engine, &request);
        run(&mut engine, PlatformCommand::CreateScratch, 3.0).unwrap();
        assert_eq!(engine.active, Some(ActiveWorkspace::Scratch));
        assert!(engine.pending_preview.is_none());
        assert!(matches!(
            run(
                &mut engine,
                PlatformCommand::CommitWorkbenchPreview {
                    generation: request.generation,
                    save_mode: PreviewSaveMode::UpdateChart { confirmed: true },
                },
                4.0,
            ),
            Err(PlatformError {
                code: PlatformErrorCode::StalePreview,
                ..
            })
        ));

        run(
            &mut engine,
            PlatformCommand::DiscardScratch { confirmed: true },
            5.0,
        )
        .unwrap();
        run(
            &mut engine,
            PlatformCommand::ActivateVault {
                vault_id: vault_id.clone(),
            },
            6.0,
        )
        .unwrap();
        calculate_pending(&mut engine, &request);
        run(
            &mut engine,
            PlatformCommand::LockVault {
                vault_id: vault_id.clone(),
            },
            7.0,
        )
        .unwrap();
        assert!(engine.pending_preview.is_none());
        assert!(matches!(
            run(
                &mut engine,
                PlatformCommand::CommitWorkbenchPreview {
                    generation: request.generation,
                    save_mode: PreviewSaveMode::UpdateChart { confirmed: true },
                },
                8.0,
            ),
            Err(PlatformError {
                code: PlatformErrorCode::StalePreview,
                ..
            })
        ));

        let mut reloaded = BrowserStudioEngine::new(store);
        run(&mut reloaded, PlatformCommand::Initialize, 9.0).unwrap();
        run(
            &mut reloaded,
            PlatformCommand::UnlockVault {
                vault_id,
                password: b"fictional password".to_vec(),
            },
            10.0,
        )
        .unwrap();
        assert!(matches!(
            run(
                &mut reloaded,
                PlatformCommand::CommitWorkbenchPreview {
                    generation: request.generation,
                    save_mode: PreviewSaveMode::SaveAs {
                        name: "Must Not Save".into(),
                    },
                },
                11.0,
            ),
            Err(PlatformError {
                code: PlatformErrorCode::StalePreview,
                ..
            })
        ));
    }

    #[test]
    fn transactional_revision_conflict_invalidates_preview_without_mutating_memory() {
        let (store, mut engine, vault_id, request) = populated_vault();
        calculate_pending(&mut engine, &request);
        let mut concurrent = engine.records[&vault_id].clone();
        concurrent.revision = "sha256:concurrent-writer".into();
        store.force_record(concurrent);

        let result = run(
            &mut engine,
            PlatformCommand::CommitWorkbenchPreview {
                generation: request.generation,
                save_mode: PreviewSaveMode::UpdateChart { confirmed: true },
            },
            3.0,
        );
        assert!(matches!(
            result,
            Err(PlatformError {
                code: PlatformErrorCode::StalePreview,
                ..
            })
        ));
        assert!(engine.pending_preview.is_none());
        let original = engine
            .active_document()
            .unwrap()
            .chart_definitions()
            .iter()
            .find(|chart| chart.id().as_str() == "fictional_transit")
            .unwrap();
        assert_eq!(original.local_input().local_date(), "2026-08-17");
        assert!(original.current_calculation_id().is_none());
    }

    #[test]
    fn scratch_is_volatile_dirty_and_converts_without_data_loss() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store);
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        run(&mut engine, PlatformCommand::CreateScratch, 1.0).unwrap();
        run(
            &mut engine,
            PlatformCommand::AddPerson {
                id: oracle_studio_core::StableId::new("person.id", "fictional").unwrap(),
                display_name: "Fictional Person".into(),
                kind: oracle_studio_core::PersonKind::Personal,
                notes: None,
            },
            2.0,
        )
        .unwrap();
        assert!(matches!(
            run(
                &mut engine,
                PlatformCommand::DiscardScratch { confirmed: false },
                3.0
            ),
            Err(PlatformError {
                code: PlatformErrorCode::ConfirmationRequired,
                ..
            })
        ));
        let response = run(
            &mut engine,
            PlatformCommand::SaveScratch {
                title: "Portable Test".into(),
                password: b"fictional password".to_vec(),
            },
            4.0,
        )
        .unwrap();
        let PlatformResponse::Updated { vaults, workspace } = response else {
            panic!("updated response")
        };
        assert_eq!(vaults.len(), 1);
        assert_eq!(workspace.people[0].label, "Fictional Person");
        assert!(!workspace.scratch_dirty);
    }

    #[test]
    fn independent_vaults_lock_after_idle_without_discarding_scratch() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store);
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        run(&mut engine, PlatformCommand::CreateScratch, 1.0).unwrap();
        run(
            &mut engine,
            PlatformCommand::SaveScratch {
                title: "One".into(),
                password: b"password one".to_vec(),
            },
            2.0,
        )
        .unwrap();
        let id = engine.records.keys().next().unwrap().clone();
        run(&mut engine, PlatformCommand::CreateScratch, 3.0).unwrap();
        run(&mut engine, PlatformCommand::Touch, IDLE_LOCK_MILLIS + 10.0).unwrap();
        assert!(!engine.mounted.contains_key(&id));
        assert!(engine.scratch.is_some());
        assert_eq!(engine.active, Some(ActiveWorkspace::Scratch));
    }

    #[test]
    fn duplicate_import_requires_explicit_replacement() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store);
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        run(&mut engine, PlatformCommand::CreateScratch, 1.0).unwrap();
        run(
            &mut engine,
            PlatformCommand::SaveScratch {
                title: "One".into(),
                password: b"password".to_vec(),
            },
            2.0,
        )
        .unwrap();
        let bytes = engine.records.values().next().unwrap().envelope.clone();
        assert!(matches!(
            run(
                &mut engine,
                PlatformCommand::ImportVault {
                    bytes: bytes.clone(),
                    replace_confirmed: false
                },
                3.0
            ),
            Err(PlatformError {
                code: PlatformErrorCode::DuplicateVault,
                ..
            })
        ));
        let response = run(
            &mut engine,
            PlatformCommand::ImportVault {
                bytes,
                replace_confirmed: true,
            },
            4.0,
        )
        .unwrap();
        let PlatformResponse::Updated { vaults, workspace } = response else {
            panic!("updated response")
        };
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].lock_state, VaultLockState::Locked);
        assert_eq!(workspace.active, None);
    }

    #[test]
    fn reload_export_remove_reimport_and_unlock_preserve_portable_bytes() {
        let store = Rc::new(MemoryStore::default());
        let mut first = BrowserStudioEngine::new(store.clone());
        run(&mut first, PlatformCommand::Initialize, 0.0).unwrap();
        run(&mut first, PlatformCommand::CreateScratch, 1.0).unwrap();
        run(
            &mut first,
            PlatformCommand::SaveScratch {
                title: "Reloadable".into(),
                password: b"portable password".to_vec(),
            },
            2.0,
        )
        .unwrap();
        let id = first.records.keys().next().unwrap().clone();
        let expected = first.records[&id].envelope.clone();

        let mut reloaded = BrowserStudioEngine::new(store);
        let ready = run(&mut reloaded, PlatformCommand::Initialize, 3.0).unwrap();
        let PlatformResponse::Ready { vaults, .. } = ready else {
            panic!("ready response")
        };
        assert_eq!(vaults[0].lock_state, VaultLockState::Locked);
        let exported = run(
            &mut reloaded,
            PlatformCommand::ExportVault {
                vault_id: id.clone(),
            },
            4.0,
        )
        .unwrap();
        let PlatformResponse::Export { bytes, .. } = exported else {
            panic!("export response")
        };
        assert_eq!(bytes, expected);
        run(
            &mut reloaded,
            PlatformCommand::RemoveVault {
                vault_id: id.clone(),
                confirmed: true,
            },
            5.0,
        )
        .unwrap();
        run(
            &mut reloaded,
            PlatformCommand::ImportVault {
                bytes,
                replace_confirmed: false,
            },
            6.0,
        )
        .unwrap();
        let unlocked = run(
            &mut reloaded,
            PlatformCommand::UnlockVault {
                vault_id: id,
                password: b"portable password".to_vec(),
            },
            7.0,
        )
        .unwrap();
        let PlatformResponse::Updated { workspace, .. } = unlocked else {
            panic!("updated response")
        };
        assert!(matches!(workspace.active, Some(ActiveWorkspace::Vault(_))));
    }

    #[test]
    fn compare_and_swap_conflict_does_not_change_decrypted_memory() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store.clone());
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        run(&mut engine, PlatformCommand::CreateScratch, 1.0).unwrap();
        run(
            &mut engine,
            PlatformCommand::SaveScratch {
                title: "Conflict".into(),
                password: b"conflict password".to_vec(),
            },
            2.0,
        )
        .unwrap();
        let id = engine.records.keys().next().unwrap().clone();
        let original_revision = engine.records[&id].revision.clone();
        let mut concurrent = engine.records[&id].clone();
        concurrent.revision = "sha256:concurrent-writer".into();
        store.force_record(concurrent);

        let result = run(
            &mut engine,
            PlatformCommand::AddPerson {
                id: oracle_studio_core::StableId::new("person.id", "must_not_commit").unwrap(),
                display_name: "Must Not Commit".into(),
                kind: oracle_studio_core::PersonKind::Personal,
                notes: None,
            },
            3.0,
        );
        assert!(matches!(
            result,
            Err(PlatformError {
                code: PlatformErrorCode::Conflict,
                ..
            })
        ));
        assert_eq!(engine.records[&id].revision, original_revision);
        assert!(engine.mounted[&id].vault.document().people().is_empty());
    }

    #[test]
    fn multiple_vaults_mount_independently_and_persistence_denial_is_reported() {
        let store = Rc::new(MemoryStore::default());
        store.deny_persistence();
        let mut engine = BrowserStudioEngine::new(store);
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        for (index, title) in ["First", "Second"].into_iter().enumerate() {
            run(
                &mut engine,
                PlatformCommand::CreateScratch,
                index as f64 * 10.0 + 1.0,
            )
            .unwrap();
            run(
                &mut engine,
                PlatformCommand::SaveScratch {
                    title: title.into(),
                    password: format!("password {title}").into_bytes(),
                },
                index as f64 * 10.0 + 2.0,
            )
            .unwrap();
        }
        assert_eq!(engine.records.len(), 2);
        assert_eq!(engine.mounted.len(), 2);
        assert_eq!(engine.persistence_granted, Some(false));
        let active = match engine.active.clone().unwrap() {
            ActiveWorkspace::Vault(id) => id,
            ActiveWorkspace::Scratch => panic!("expected active vault"),
        };
        let other = engine
            .records
            .keys()
            .find(|id| **id != active)
            .unwrap()
            .clone();
        run(
            &mut engine,
            PlatformCommand::LockVault {
                vault_id: other.clone(),
            },
            30.0,
        )
        .unwrap();
        assert!(!engine.mounted.contains_key(&other));
        assert!(engine.mounted.contains_key(&active));
        assert_eq!(engine.active, Some(ActiveWorkspace::Vault(active)));
    }

    #[test]
    fn export_filenames_never_collapse_to_an_empty_basename() {
        assert_eq!(safe_filename("星"), "oracle-vault");
        assert_eq!(safe_filename("  Portable / Studio  "), "portable-studio");
    }

    #[test]
    fn corrupt_global_wheel_settings_fall_back_without_blocking_startup() {
        let store = Rc::new(MemoryStore::default());
        store.force_wheel_template_settings(
            r#"{"schema_version":99,"templates":[],"last_selected_template_id":"missing"}"#,
        );
        let mut engine = BrowserStudioEngine::new(store);
        let response = run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        let PlatformResponse::Ready {
            wheel_templates, ..
        } = response
        else {
            panic!("ready response")
        };
        assert_eq!(wheel_templates, WheelTemplateSettings::default());
    }

    #[test]
    fn wheel_templates_persist_globally_and_restore_the_last_selection() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store.clone());
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        let template = oracle_studio_platform::WheelTemplate {
            id: "paper-compact".into(),
            name: "Paper Compact".into(),
            mode: oracle_studio_platform::WheelMode::Biwheel,
            orientation: oracle_studio_platform::WheelOrientation::ZodiacZeroTop,
            palette: oracle_studio_platform::WheelPaletteSelection::Explicit(
                oracle_studio_platform::WheelPalette::PaperLight,
            ),
            label_density: oracle_studio_platform::LabelDensity::Compact,
            layout: oracle_studio_platform::WheelLayout::Compact,
        };
        run(
            &mut engine,
            PlatformCommand::SaveWheelTemplate {
                template: template.clone(),
            },
            1.0,
        )
        .unwrap();

        let mut reloaded = BrowserStudioEngine::new(store);
        let response = run(&mut reloaded, PlatformCommand::Initialize, 2.0).unwrap();
        let PlatformResponse::Ready {
            wheel_templates, ..
        } = response
        else {
            panic!("ready response")
        };
        assert_eq!(wheel_templates.last_selected_template_id, template.id);
        assert!(wheel_templates.templates.contains(&template));
    }

    #[test]
    fn schema_v1_wheel_templates_migrate_without_losing_custom_ids_or_selection() {
        let store = Rc::new(MemoryStore::default());
        store.force_wheel_template_settings(
            r#"{"schema_version":1,"templates":[{"id":"my-paper","name":"My Paper","orientation":"zodiac-zero-top","palette":"paper-light","label_density":"compact"}],"last_selected_template_id":"my-paper"}"#,
        );
        let mut engine = BrowserStudioEngine::new(store.clone());
        let response = run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        let PlatformResponse::Ready {
            wheel_templates, ..
        } = response
        else {
            panic!("ready response")
        };
        assert_eq!(wheel_templates.schema_version, 2);
        assert_eq!(wheel_templates.last_selected_template_id, "my-paper");
        let migrated = wheel_templates
            .templates
            .iter()
            .find(|template| template.id == "my-paper")
            .unwrap();
        assert_eq!(migrated.mode, oracle_studio_platform::WheelMode::Biwheel);
        assert_eq!(
            migrated.palette,
            oracle_studio_platform::WheelPaletteSelection::Explicit(
                oracle_studio_platform::WheelPalette::PaperLight
            )
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&store.wheel_template_settings().unwrap())
                .unwrap()["schema_version"],
            2
        );
    }

    #[test]
    fn protected_wheel_templates_are_selectable_but_not_mutable_or_removable() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store);
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        let protected = engine.wheel_templates.templates[0].clone();
        assert!(protected.is_protected());
        let save_error = run(
            &mut engine,
            PlatformCommand::SaveWheelTemplate {
                template: protected.clone(),
            },
            1.0,
        )
        .unwrap_err();
        assert_eq!(save_error.code, PlatformErrorCode::InvalidInput);
        let remove_error = run(
            &mut engine,
            PlatformCommand::RemoveWheelTemplate {
                template_id: protected.id,
            },
            2.0,
        )
        .unwrap_err();
        assert_eq!(remove_error.code, PlatformErrorCode::InvalidInput);
    }
}
