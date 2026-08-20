//! The browser-local boundary between the Leptos presentation and its worker.
//!
//! This contract is deliberately versionless. Native and HTTP platforms may be
//! added later without changing the browser product into a protocol server.

use std::{future::Future, pin::Pin};

use oracle_studio_core::{
    AmbiguousTimeChoice, ChartDefinition, ComparisonPreset, LocalDateTimeInput,
    LocalTimeResolution, PersonKind, SavedLocation, StableId, WorkspaceState,
};
use oracle_studio_location_catalog::{CatalogInstallInput, CatalogMetadata, CatalogSearchMatch};
use serde::{Deserialize, Serialize};

pub type PlatformFuture =
    Pin<Box<dyn Future<Output = Result<PlatformResponse, PlatformError>> + 'static>>;

pub trait StudioPlatform {
    fn execute(&self, command: PlatformCommand) -> PlatformFuture;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlatformCommand {
    Initialize,
    CreateScratch,
    DiscardScratch {
        confirmed: bool,
    },
    SaveScratch {
        title: String,
        password: Vec<u8>,
    },
    ListVaults,
    ImportVault {
        bytes: Vec<u8>,
        replace_confirmed: bool,
    },
    ExportVault {
        vault_id: String,
    },
    UnlockVault {
        vault_id: String,
        password: Vec<u8>,
    },
    LockVault {
        vault_id: String,
    },
    ActivateVault {
        vault_id: String,
    },
    UnloadVault {
        vault_id: String,
    },
    RemoveVault {
        vault_id: String,
        confirmed: bool,
    },
    AddPerson {
        id: StableId,
        display_name: String,
        kind: PersonKind,
        notes: Option<String>,
    },
    SaveLocation {
        location: SavedLocation,
    },
    SaveChart {
        chart: ChartDefinition,
    },
    ResolveLocalTime {
        input: LocalDateTimeInput,
        choice: Option<AmbiguousTimeChoice>,
    },
    CalculateChart {
        id: StableId,
        chart_definition_id: StableId,
        saved_location_id: StableId,
        choice: Option<AmbiguousTimeChoice>,
        calculated_at: String,
    },
    SaveComparison {
        preset: ComparisonPreset,
    },
    CalculateComparison {
        id: StableId,
        comparison_preset_id: StableId,
        calculated_at: String,
    },
    SetWorkspace {
        workspace: WorkspaceState,
    },
    InstallPinnedCatalog,
    InstallCatalog {
        input: CatalogInstallInput,
    },
    SearchCatalog {
        query: String,
        limit: usize,
    },
    Touch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActiveWorkspace {
    Scratch,
    Vault(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VaultLockState {
    Locked,
    Mounted,
    Active,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VaultSummary {
    pub id: String,
    pub title: String,
    pub revision: String,
    pub created_at: String,
    pub modified_at: String,
    pub lock_state: VaultLockState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub active: Option<ActiveWorkspace>,
    pub scratch_dirty: bool,
    pub people: Vec<EntitySummary>,
    pub locations: Vec<EntitySummary>,
    pub charts: Vec<ChartSummary>,
    pub comparisons: Vec<EntitySummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntitySummary {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChartSummary {
    pub id: String,
    pub label: String,
    pub role: String,
    pub local_input: String,
    pub current_calculation_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EphemerisStatus {
    Unavailable,
    DeterministicTest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityStatus {
    pub ephemeris: EphemerisStatus,
    pub catalog: Option<CatalogMetadata>,
    pub persistence_requested: bool,
    pub persistence_granted: Option<bool>,
    pub backup_warning: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlatformResponse {
    Ready {
        vaults: Vec<VaultSummary>,
        workspace: WorkspaceSummary,
        capabilities: CapabilityStatus,
    },
    Vaults(Vec<VaultSummary>),
    Workspace(WorkspaceSummary),
    Export {
        filename: String,
        bytes: Vec<u8>,
    },
    LocalTime(LocalTimeResolution),
    CatalogInstalled(CatalogMetadata),
    CatalogResults(Vec<CatalogSearchMatch>),
    Updated {
        vaults: Vec<VaultSummary>,
        workspace: WorkspaceSummary,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformErrorCode {
    InvalidInput,
    NotFound,
    Locked,
    Conflict,
    DuplicateVault,
    ConfirmationRequired,
    Authentication,
    UnsupportedFormat,
    ProviderUnavailable,
    Storage,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlatformError {
    pub code: PlatformErrorCode,
    pub message: String,
}

impl PlatformError {
    pub fn new(code: PlatformErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PlatformError {}
