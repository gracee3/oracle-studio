//! Browser-only state ownership and transactional persistence.

use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc};

use astraeus_moshier::MoshierEphemerisAdapter;
use oracle_studio_app::{
    ChartCalculationRequest, ComparisonCalculationRequest, PreparedWorkbenchPreview,
    WorkbenchCalculationRequest, calculate_chart, calculate_comparison_with_aspect_set,
    calculate_workbench_preview_with_aspect_set, commit_workbench_save_as, commit_workbench_update,
};
use oracle_studio_aspect_sets::{AspectSetError, AspectSetSettings};
use oracle_studio_core::{
    PersonProfile, VaultDocument, generate_unique_id, resolve_local_time, select_local_time,
};
use oracle_studio_location_catalog::{CatalogInstallInput, CatalogMetadata, LocationCatalog};
use oracle_studio_platform::{
    ActiveWorkspace, CapabilityStatus, ChartSummary, EntitySummary, EphemerisStatus,
    PlatformCommand, PlatformError, PlatformErrorCode, PlatformResponse, PreviewSaveMode,
    VaultLockState, VaultSummary, WheelTemplateSettings, WorkbenchChartSummary,
    WorkbenchPresentation, WorkspaceSummary,
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
    fn load_aspect_set_settings(&self) -> StoreFuture<'_, Option<String>>;
    fn save_aspect_set_settings(&self, settings: String) -> StoreFuture<'_, ()>;
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
    aspect_sets: AspectSetSettings,
    pending_preview: Option<(
        oracle_studio_platform::PreviewGeneration,
        PreparedWorkbenchPreview,
    )>,
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
            aspect_sets: AspectSetSettings::default(),
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
                    aspect_sets: self.aspect_sets.clone(),
                })
            }
            PlatformCommand::CreateScratch => {
                if self.scratch.is_none() {
                    self.scratch = Some(VaultDocument::empty());
                }
                self.active = Some(ActiveWorkspace::Scratch);
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
                let document = calculate_comparison_with_aspect_set(
                    self.active_document()?,
                    ComparisonCalculationRequest {
                        id,
                        comparison_preset_id,
                        calculated_at,
                    },
                    self.aspect_sets.selected().snapshot(),
                )
                .map_err(app_error)?;
                self.commit_document(document, now_millis, now).await?;
                Ok(self.updated())
            }
            PlatformCommand::WorkbenchPreview { request } => {
                let prepared = calculate_workbench_preview_with_aspect_set(
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
                    self.aspect_sets.selected().snapshot(),
                )
                .map_err(app_error)?;
                let presentation = workbench_presentation(
                    request.generation,
                    &prepared,
                    request.adjustment_notice,
                );
                self.pending_preview = Some((request.generation, prepared));
                Ok(PlatformResponse::WorkbenchPreview(presentation))
            }
            PlatformCommand::CommitWorkbenchPreview {
                generation,
                save_mode,
            } => {
                let (pending_generation, preview) =
                    self.pending_preview.as_ref().ok_or_else(|| {
                        error(
                            PlatformErrorCode::NotFound,
                            "no workbench preview is available",
                        )
                    })?;
                if *pending_generation != generation {
                    return Err(error(
                        PlatformErrorCode::Conflict,
                        "the workbench preview is stale; wait for the newest calculation",
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
                    &format!("{} calculation", preview.outer.definition.label()),
                    &calculation_ids,
                )
                .map_err(model_error)?;
                let updated = match save_mode {
                    PreviewSaveMode::UpdateChart => {
                        commit_workbench_update(document, preview, calculation_id, now.clone())
                    }
                    PreviewSaveMode::SaveAs { name } => {
                        if name.trim().is_empty() {
                            return Err(error(
                                PlatformErrorCode::InvalidInput,
                                "Save As requires a new chart name",
                            ));
                        }
                        let chart_ids = document
                            .chart_definitions()
                            .iter()
                            .map(|chart| chart.id().as_str().to_owned())
                            .collect();
                        let chart_id =
                            generate_unique_id("chart", &name, &chart_ids).map_err(model_error)?;
                        commit_workbench_save_as(
                            document,
                            preview,
                            chart_id,
                            name,
                            calculation_id,
                            now.clone(),
                        )
                    }
                }
                .map_err(app_error)?;
                self.commit_document(updated, now_millis, now).await?;
                self.pending_preview = None;
                Ok(self.updated())
            }
            PlatformCommand::SaveWheelTemplate { template } => {
                template.validate()?;
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
            PlatformCommand::SaveAspectSet { set } => {
                self.aspect_sets.save_user(set).map_err(aspect_error)?;
                self.pending_preview = None;
                self.persist_aspect_sets().await?;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::DuplicateAspectSet {
                source_id,
                id,
                name,
            } => {
                let source = self
                    .aspect_sets
                    .sets()
                    .iter()
                    .find(|set| set.id() == source_id)
                    .cloned()
                    .ok_or_else(|| {
                        error(PlatformErrorCode::NotFound, "aspect set was not found")
                    })?;
                let duplicate = source.duplicate(id, name).map_err(aspect_error)?;
                let duplicate_id = duplicate.id().to_owned();
                self.aspect_sets
                    .save_user(duplicate)
                    .map_err(aspect_error)?;
                self.aspect_sets
                    .select(&duplicate_id)
                    .map_err(aspect_error)?;
                self.pending_preview = None;
                self.persist_aspect_sets().await?;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::RenameAspectSet { id, name } => {
                self.aspect_sets.select(&id).map_err(aspect_error)?;
                self.aspect_sets
                    .rename_selected(name)
                    .map_err(aspect_error)?;
                self.pending_preview = None;
                self.persist_aspect_sets().await?;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::DeleteAspectSet { id } => {
                self.aspect_sets.select(&id).map_err(aspect_error)?;
                self.aspect_sets.delete_selected().map_err(aspect_error)?;
                self.pending_preview = None;
                self.persist_aspect_sets().await?;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::SelectAspectSet { id } => {
                self.aspect_sets.select(&id).map_err(aspect_error)?;
                self.persist_aspect_sets().await?;
                self.pending_preview = None;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::ResetAspectSets => {
                self.aspect_sets.reset_builtins().map_err(aspect_error)?;
                self.persist_aspect_sets().await?;
                self.pending_preview = None;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::ImportAspectSet { bytes } => {
                self.aspect_sets.import(&bytes).map_err(aspect_error)?;
                self.persist_aspect_sets().await?;
                self.pending_preview = None;
                Ok(PlatformResponse::AspectSets(self.aspect_sets.clone()))
            }
            PlatformCommand::ExportAspectSet { id } => {
                let set = self
                    .aspect_sets
                    .sets()
                    .iter()
                    .find(|set| set.id() == id)
                    .ok_or_else(|| {
                        error(PlatformErrorCode::NotFound, "aspect set was not found")
                    })?;
                Ok(PlatformResponse::Export {
                    filename: format!("{}.oracle-aspects.json", safe_filename(set.name())),
                    bytes: set.to_pretty_json().map_err(aspect_error)?,
                })
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
            && let Ok(settings) = serde_json::from_str::<WheelTemplateSettings>(&raw)
            && settings.validate().is_ok()
        {
            self.wheel_templates = settings;
        }
        if let Some(raw) = self
            .store
            .load_aspect_set_settings()
            .await
            .map_err(store_error)?
            && let Ok(settings) = serde_json::from_str::<AspectSetSettings>(&raw)
            && settings.validate().is_ok()
        {
            self.aspect_sets = settings;
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

    async fn persist_aspect_sets(&self) -> Result<(), PlatformError> {
        self.aspect_sets.validate().map_err(aspect_error)?;
        let raw = serde_json::to_string(&self.aspect_sets).map_err(|serialization_error| {
            error(PlatformErrorCode::Internal, serialization_error.to_string())
        })?;
        self.store
            .save_aspect_set_settings(raw)
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
    generation: oracle_studio_platform::PreviewGeneration,
    preview: &PreparedWorkbenchPreview,
    adjustment_notice: Option<String>,
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
fn aspect_error(error_: AspectSetError) -> PlatformError {
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
        aspect_set_settings: RefCell<Option<String>>,
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
        pub fn force_aspect_set_settings(&self, settings: impl Into<String>) {
            *self.aspect_set_settings.borrow_mut() = Some(settings.into());
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
        fn load_aspect_set_settings(&self) -> StoreFuture<'_, Option<String>> {
            Box::pin(ready(Ok(self.aspect_set_settings.borrow().clone())))
        }
        fn save_aspect_set_settings(&self, settings: String) -> StoreFuture<'_, ()> {
            *self.aspect_set_settings.borrow_mut() = Some(settings);
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

    fn run(
        engine: &mut BrowserStudioEngine,
        command: PlatformCommand,
        millis: f64,
    ) -> Result<PlatformResponse, PlatformError> {
        block_on(engine.execute(command, millis, "2026-08-19T12:00:00Z".into()))
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
    fn corrupt_global_aspect_settings_fall_back_without_blocking_startup() {
        let store = Rc::new(MemoryStore::default());
        store.force_aspect_set_settings(
            r#"{"schema_version":99,"sets":[],"selected_aspect_set_id":"missing"}"#,
        );
        let mut engine = BrowserStudioEngine::new(store);
        let response = run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        let PlatformResponse::Ready { aspect_sets, .. } = response else {
            panic!("ready response")
        };
        assert_eq!(aspect_sets, AspectSetSettings::default());
    }

    #[test]
    fn aspect_set_crud_import_export_and_selection_persist_globally() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store.clone());
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        run(
            &mut engine,
            PlatformCommand::DuplicateAspectSet {
                source_id: "builtin.standard".into(),
                id: "user.reviewed".into(),
                name: "Reviewed".into(),
            },
            1.0,
        )
        .unwrap();
        run(
            &mut engine,
            PlatformCommand::RenameAspectSet {
                id: "user.reviewed".into(),
                name: "Reviewed v2".into(),
            },
            2.0,
        )
        .unwrap();
        let exported = run(
            &mut engine,
            PlatformCommand::ExportAspectSet {
                id: "user.reviewed".into(),
            },
            3.0,
        )
        .unwrap();
        let PlatformResponse::Export { filename, bytes } = exported else {
            panic!("export response")
        };
        assert_eq!(filename, "reviewed-v2.oracle-aspects.json");
        assert!(matches!(
            run(
                &mut engine,
                PlatformCommand::ImportAspectSet {
                    bytes: bytes.clone()
                },
                4.0
            ),
            Err(PlatformError {
                code: PlatformErrorCode::InvalidInput,
                ..
            })
        ));
        run(
            &mut engine,
            PlatformCommand::DeleteAspectSet {
                id: "user.reviewed".into(),
            },
            5.0,
        )
        .unwrap();
        run(&mut engine, PlatformCommand::ImportAspectSet { bytes }, 6.0).unwrap();
        assert_eq!(engine.aspect_sets.selected_aspect_set_id(), "user.reviewed");
        run(
            &mut engine,
            PlatformCommand::SelectAspectSet {
                id: "builtin.tight".into(),
            },
            7.0,
        )
        .unwrap();
        assert!(matches!(
            run(
                &mut engine,
                PlatformCommand::DeleteAspectSet {
                    id: "builtin.tight".into()
                },
                8.0
            ),
            Err(PlatformError {
                code: PlatformErrorCode::InvalidInput,
                ..
            })
        ));

        let mut reloaded = BrowserStudioEngine::new(store);
        let response = run(&mut reloaded, PlatformCommand::Initialize, 9.0).unwrap();
        let PlatformResponse::Ready { aspect_sets, .. } = response else {
            panic!("ready response")
        };
        assert_eq!(aspect_sets.selected_aspect_set_id(), "builtin.tight");
        assert!(
            aspect_sets
                .sets()
                .iter()
                .any(|set| set.id() == "user.reviewed" && set.revision() == 2)
        );
    }

    #[test]
    fn wheel_templates_persist_globally_and_restore_the_last_selection() {
        let store = Rc::new(MemoryStore::default());
        let mut engine = BrowserStudioEngine::new(store.clone());
        run(&mut engine, PlatformCommand::Initialize, 0.0).unwrap();
        let template = oracle_studio_platform::WheelTemplate {
            id: "paper-compact".into(),
            name: "Paper Compact".into(),
            orientation: oracle_studio_platform::WheelOrientation::ZodiacZeroTop,
            palette: oracle_studio_platform::WheelPalette::PaperLight,
            label_density: oracle_studio_platform::LabelDensity::Compact,
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
}
