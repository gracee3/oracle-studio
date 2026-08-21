//! The browser-local boundary between the Leptos presentation and its worker.
//!
//! This contract is deliberately versionless. Native and HTTP platforms may be
//! added later without changing the browser product into a protocol server.

use std::{future::Future, pin::Pin};

pub use oracle_studio_aspect_sets::{
    AspectKind, AspectOrbValues, AspectSet, AspectSetRule, AspectSetSettings, AspectSetSnapshot,
    ChartPointId, MAX_IMPORT_BYTES,
};
use oracle_studio_chart_view::ChartScene;
pub use oracle_studio_chart_view::{
    LabelDensity, WheelLayout, WheelMode, WheelOrientation, WheelPalette,
};
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
    UpdateChartBasics {
        chart_id: StableId,
        label: String,
        role: oracle_studio_core::ChartRole,
        local_input: LocalDateTimeInput,
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
    WorkbenchPreview {
        request: WorkbenchPreviewRequest,
    },
    CommitWorkbenchPreview {
        generation: PreviewGeneration,
        save_mode: PreviewSaveMode,
    },
    SaveWheelTemplate {
        template: WheelTemplate,
    },
    SelectWheelTemplate {
        template_id: String,
    },
    RemoveWheelTemplate {
        template_id: String,
    },
    SaveAspectSet {
        set: AspectSet,
    },
    DuplicateAspectSet {
        source_id: String,
        id: String,
        name: String,
    },
    RenameAspectSet {
        id: String,
        name: String,
    },
    DeleteAspectSet {
        id: String,
    },
    SelectAspectSet {
        id: String,
    },
    ResetAspectSets,
    ImportAspectSet {
        bytes: Vec<u8>,
    },
    ExportAspectSet {
        id: String,
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
    pub local_date: String,
    pub local_time: String,
    pub time_zone: String,
    pub current_calculation_id: Option<String>,
    pub current_saved_location_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EphemerisStatus {
    Unavailable,
    DeterministicTest,
    Moshier,
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
#[allow(clippy::large_enum_variant)] // Browser-local messages favor a direct typed API over heap indirection.
pub enum PlatformResponse {
    Ready {
        vaults: Vec<VaultSummary>,
        workspace: WorkspaceSummary,
        capabilities: CapabilityStatus,
        wheel_templates: WheelTemplateSettings,
        aspect_sets: AspectSetSettings,
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
    WorkbenchPreview(WorkbenchPresentation),
    WorkbenchPreviewCommitted {
        vaults: Vec<VaultSummary>,
        workspace: WorkspaceSummary,
        outcome: PreviewCommitOutcome,
    },
    WheelTemplates(WheelTemplateSettings),
    AspectSets(AspectSetSettings),
    Updated {
        vaults: Vec<VaultSummary>,
        workspace: WorkspaceSummary,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PreviewGeneration(u64);

impl PreviewGeneration {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchPreviewRequest {
    pub generation: PreviewGeneration,
    pub inner_chart_definition_id: StableId,
    pub outer_chart_definition_id: StableId,
    pub inner_saved_location_id: StableId,
    pub outer_saved_location_id: StableId,
    pub outer_local_input: LocalDateTimeInput,
    pub outer_ambiguous_time_choice: Option<AmbiguousTimeChoice>,
    pub adjustment_notice: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewSaveMode {
    UpdateChart { confirmed: bool },
    SaveAs { name: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewCommitOutcome {
    Updated { chart_id: String, label: String },
    SavedAs { chart_id: String, label: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchChartSummary {
    pub id: String,
    pub label: String,
    pub role: String,
    pub local_input: LocalDateTimeInput,
    pub location_label: String,
    pub zodiac: String,
    pub house_system: String,
    pub utc_offset_seconds: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchPresentation {
    pub generation: PreviewGeneration,
    pub source: Box<WorkbenchPreviewSource>,
    pub inner: WorkbenchChartSummary,
    pub outer: WorkbenchChartSummary,
    pub scene: ChartScene,
    pub calculation: CalculationStatus,
    pub adjustment_notice: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalculationStatus {
    pub duration_micros: u64,
    pub provider: String,
    pub provider_version: String,
    pub ephemeris_mode: String,
    pub aspect_set_name: String,
    pub aspect_set_id: String,
    pub aspect_set_revision: u32,
    pub aspect_set_content_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchPreviewSource {
    pub vault_id: Option<String>,
    pub vault_title: String,
    pub vault_revision: Option<String>,
}

pub const WHEEL_TEMPLATE_SETTINGS_VERSION: u32 = 2;
pub const STUDIO_BIWHEEL_TEMPLATE_ID: &str = "oracle-studio-biwheel";
pub const COMPACT_BIWHEEL_TEMPLATE_ID: &str = "oracle-compact-biwheel";
pub const HIGH_CONTRAST_BIWHEEL_TEMPLATE_ID: &str = "oracle-high-contrast-biwheel";
pub const CLASSIC_SINGLE_TEMPLATE_ID: &str = "oracle-classic-single";
pub const DATA_FORWARD_SINGLE_TEMPLATE_ID: &str = "oracle-data-forward-single";

const BUILTIN_TEMPLATE_IDS: [&str; 5] = [
    STUDIO_BIWHEEL_TEMPLATE_ID,
    COMPACT_BIWHEEL_TEMPLATE_ID,
    HIGH_CONTRAST_BIWHEEL_TEMPLATE_ID,
    CLASSIC_SINGLE_TEMPLATE_ID,
    DATA_FORWARD_SINGLE_TEMPLATE_ID,
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "selection", content = "palette", rename_all = "kebab-case")]
pub enum WheelPaletteSelection {
    #[default]
    Auto,
    Explicit(WheelPalette),
}

impl WheelPaletteSelection {
    pub const fn resolve(self, dark_theme: bool) -> WheelPalette {
        match self {
            Self::Auto if dark_theme => WheelPalette::StudioDark,
            Self::Auto => WheelPalette::PaperLight,
            Self::Explicit(palette) => palette,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelTemplate {
    pub id: String,
    pub name: String,
    pub mode: WheelMode,
    pub orientation: WheelOrientation,
    pub palette: WheelPaletteSelection,
    pub label_density: LabelDensity,
    pub layout: WheelLayout,
}

impl WheelTemplate {
    pub fn validate(&self) -> Result<(), PlatformError> {
        let valid_id = !self.id.is_empty()
            && self.id.len() <= 128
            && self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !valid_id || self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidInput,
                "wheel template requires a bounded name and lowercase hyphenated ID",
            ));
        }
        Ok(())
    }

    pub fn is_protected(&self) -> bool {
        BUILTIN_TEMPLATE_IDS.contains(&self.id.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelTemplateSettings {
    pub schema_version: u32,
    pub templates: Vec<WheelTemplate>,
    pub last_selected_template_id: String,
}

impl Default for WheelTemplateSettings {
    fn default() -> Self {
        Self {
            schema_version: WHEEL_TEMPLATE_SETTINGS_VERSION,
            templates: builtin_wheel_templates(),
            last_selected_template_id: STUDIO_BIWHEEL_TEMPLATE_ID.into(),
        }
    }
}

impl WheelTemplateSettings {
    pub fn validate(&self) -> Result<(), PlatformError> {
        if self.schema_version != WHEEL_TEMPLATE_SETTINGS_VERSION
            || self.templates.is_empty()
            || self.templates.len() > 128
        {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidInput,
                "unsupported or empty wheel template settings",
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for template in &self.templates {
            template.validate()?;
            if !ids.insert(&template.id) {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidInput,
                    "wheel template IDs must be unique",
                ));
            }
        }
        for builtin in builtin_wheel_templates() {
            let Some(stored) = self
                .templates
                .iter()
                .find(|template| template.id == builtin.id)
            else {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidInput,
                    "all protected Oracle wheel templates must remain present",
                ));
            };
            if stored != &builtin {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidInput,
                    "protected Oracle wheel templates are immutable",
                ));
            }
        }
        if !ids.contains(&self.last_selected_template_id) {
            return Err(PlatformError::new(
                PlatformErrorCode::InvalidInput,
                "selected wheel template is missing",
            ));
        }
        Ok(())
    }

    pub fn selected(&self) -> &WheelTemplate {
        self.templates
            .iter()
            .find(|template| template.id == self.last_selected_template_id)
            .expect("validated template settings retain their selected record")
    }
}

pub fn builtin_wheel_templates() -> Vec<WheelTemplate> {
    vec![
        WheelTemplate {
            id: STUDIO_BIWHEEL_TEMPLATE_ID.into(),
            name: "Studio Biwheel".into(),
            mode: WheelMode::Biwheel,
            orientation: WheelOrientation::AscendantLeft,
            palette: WheelPaletteSelection::Auto,
            label_density: LabelDensity::Full,
            layout: WheelLayout::Balanced,
        },
        WheelTemplate {
            id: COMPACT_BIWHEEL_TEMPLATE_ID.into(),
            name: "Compact Biwheel".into(),
            mode: WheelMode::Biwheel,
            orientation: WheelOrientation::AscendantLeft,
            palette: WheelPaletteSelection::Auto,
            label_density: LabelDensity::Compact,
            layout: WheelLayout::Compact,
        },
        WheelTemplate {
            id: HIGH_CONTRAST_BIWHEEL_TEMPLATE_ID.into(),
            name: "High Contrast Biwheel".into(),
            mode: WheelMode::Biwheel,
            orientation: WheelOrientation::AscendantLeft,
            palette: WheelPaletteSelection::Explicit(WheelPalette::HighContrast),
            label_density: LabelDensity::Full,
            layout: WheelLayout::Balanced,
        },
        WheelTemplate {
            id: CLASSIC_SINGLE_TEMPLATE_ID.into(),
            name: "Classic Single".into(),
            mode: WheelMode::Single,
            orientation: WheelOrientation::AscendantLeft,
            palette: WheelPaletteSelection::Auto,
            label_density: LabelDensity::Full,
            layout: WheelLayout::Balanced,
        },
        WheelTemplate {
            id: DATA_FORWARD_SINGLE_TEMPLATE_ID.into(),
            name: "Data-forward Single".into(),
            mode: WheelMode::Single,
            orientation: WheelOrientation::ZodiacZeroTop,
            palette: WheelPaletteSelection::Auto,
            label_density: LabelDensity::Full,
            layout: WheelLayout::DataForward,
        },
    ]
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelTemplateV1 {
    pub id: String,
    pub name: String,
    pub orientation: WheelOrientation,
    pub palette: WheelPalette,
    pub label_density: LabelDensity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelTemplateSettingsV1 {
    pub schema_version: u32,
    pub templates: Vec<WheelTemplateV1>,
    pub last_selected_template_id: String,
}

impl WheelTemplateSettingsV1 {
    pub fn migrate(self) -> Result<WheelTemplateSettings, PlatformError> {
        if self.schema_version != 1 || self.templates.is_empty() {
            return Err(PlatformError::new(
                PlatformErrorCode::UnsupportedFormat,
                "wheel template settings are not schema v1",
            ));
        }
        let mut templates = builtin_wheel_templates();
        for legacy in self.templates {
            if BUILTIN_TEMPLATE_IDS.contains(&legacy.id.as_str()) {
                return Err(PlatformError::new(
                    PlatformErrorCode::InvalidInput,
                    "schema-v1 wheel template uses a reserved Oracle template ID",
                ));
            }
            templates.push(WheelTemplate {
                id: legacy.id,
                name: legacy.name,
                mode: WheelMode::Biwheel,
                orientation: legacy.orientation,
                palette: WheelPaletteSelection::Explicit(legacy.palette),
                label_density: legacy.label_density,
                layout: WheelLayout::Balanced,
            });
        }
        let migrated = WheelTemplateSettings {
            schema_version: WHEEL_TEMPLATE_SETTINGS_VERSION,
            templates,
            last_selected_template_id: self.last_selected_template_id,
        };
        migrated.validate()?;
        Ok(migrated)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformErrorCode {
    InvalidInput,
    NotFound,
    Locked,
    Conflict,
    StalePreview,
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
