//! Versioned JSON messages shared by the browser UI and native Studio host.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRequest {
    pub protocol_version: u16,
}

impl ProtocolRequest {
    pub const fn current() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiResponse<T> {
    pub protocol_version: u16,
    pub data: T,
}

impl<T> ApiResponse<T> {
    pub const fn current(data: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            data,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    BadRequest,
    Conflict,
    Locked,
    NotFound,
    ProtocolMismatch,
    Unauthorized,
    Unavailable,
    VaultAuthentication,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub protocol_version: u16,
    pub code: ApiErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn current(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultState {
    Locked,
    Unlocked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatus {
    pub state: VaultState,
    pub vault_name: Option<String>,
    pub revision: Option<String>,
    pub idle_timeout_seconds: u64,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateVaultRequest {
    pub protocol_version: u16,
    pub vault_path: String,
    password: String,
}

impl CreateVaultRequest {
    pub fn current(vault_path: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            vault_path: vault_path.into(),
            password: password.into(),
        }
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn into_parts(self) -> (String, String) {
        (self.vault_path, self.password)
    }
}

impl fmt::Debug for CreateVaultRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreateVaultRequest")
            .field("protocol_version", &self.protocol_version)
            .field("vault_path", &self.vault_path)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnlockVaultRequest {
    pub protocol_version: u16,
    pub vault_path: String,
    password: String,
}

impl UnlockVaultRequest {
    pub fn current(vault_path: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            vault_path: vault_path.into(),
            password: password.into(),
        }
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn into_parts(self) -> (String, String) {
        (self.vault_path, self.password)
    }
}

impl fmt::Debug for UnlockVaultRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UnlockVaultRequest")
            .field("protocol_version", &self.protocol_version)
            .field("vault_path", &self.vault_path)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonKindInput {
    Personal,
    ProfessionalClient,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePersonRequest {
    pub protocol_version: u16,
    pub id: String,
    pub display_name: String,
    pub kind: PersonKindInput,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocationProvenanceInput {
    Manual,
    GeoNames {
        geonames_id: u64,
        catalog_content_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveLocationRequest {
    pub protocol_version: u16,
    pub id: String,
    pub label: String,
    pub administrative_names: Vec<String>,
    pub country_code: String,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub elevation_meters: Option<f64>,
    pub time_zone: String,
    pub provenance: LocationProvenanceInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartRoleInput {
    Natal,
    Event,
    Transit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZodiacInput {
    Tropical,
    Sidereal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AyanamsaInput {
    FaganBradley,
    Lahiri,
    DeLuce,
    Raman,
    Krishnamurti,
    Yukteshwar,
    JnBhasin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HouseSystemInput {
    Placidus,
    Koch,
    Porphyry,
    Regiomontanus,
    Campanus,
    Equal,
    WholeSign,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CelestialObjectInput {
    Moon,
    Sun,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
    MeanNode,
    TrueNode,
    Chiron,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartPointInput {
    Moon,
    Sun,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
    MeanNode,
    TrueNode,
    Chiron,
    MeanSouthNode,
    TrueSouthNode,
    Ascendant,
    Midheaven,
    Descendant,
    ImumCoeli,
    Vertex,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveChartRequest {
    pub protocol_version: u16,
    pub id: String,
    pub label: String,
    pub role: ChartRoleInput,
    pub person_id: Option<String>,
    pub local_date: String,
    pub local_time: String,
    pub time_zone: String,
    pub zodiac: ZodiacInput,
    pub ayanamsa: Option<AyanamsaInput>,
    pub house_system: HouseSystemInput,
    pub ordered_objects: Vec<CelestialObjectInput>,
    pub ordered_points: Vec<ChartPointInput>,
    pub default_natal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguousTimeChoiceInput {
    Earlier,
    Later,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalculateChartRequest {
    pub protocol_version: u16,
    pub chart_calculation_id: String,
    pub calculation_artifact_id: String,
    pub chart_definition_id: String,
    pub saved_location_id: String,
    pub ambiguous_time_choice: Option<AmbiguousTimeChoiceInput>,
    pub calculated_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AspectKindInput {
    Conjunction,
    Opposition,
    Square,
    Trine,
    Sextile,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AspectDefinitionInput {
    pub kind: AspectKindInput,
    pub orb_degrees: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelOrientationInput {
    AscendantLeft,
    AriesTop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveComparisonRequest {
    pub protocol_version: u16,
    pub id: String,
    pub label: String,
    pub inner_chart_definition_id: String,
    pub outer_chart_definition_id: String,
    pub inner_points: Vec<ChartPointInput>,
    pub outer_points: Vec<ChartPointInput>,
    pub aspects: Vec<AspectDefinitionInput>,
    pub orientation: WheelOrientationInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalculateComparisonRequest {
    pub protocol_version: u16,
    pub comparison_artifact_id: String,
    pub comparison_preset_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetWorkspaceRequest {
    pub protocol_version: u16,
    pub active_person_id: Option<String>,
    pub active_comparison_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationResult {
    pub revision: String,
    pub record_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonSummary {
    pub id: String,
    pub display_name: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationSummary {
    pub id: String,
    pub label: String,
    pub country_code: String,
    pub time_zone: String,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartSummary {
    pub id: String,
    pub label: String,
    pub role: String,
    pub person_id: Option<String>,
    pub default_natal: bool,
    pub current_calculation_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonSummary {
    pub id: String,
    pub label: String,
    pub inner_chart_id: String,
    pub outer_chart_id: String,
    pub current_comparison_artifact_id: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSummary {
    pub active_person_id: Option<String>,
    pub active_comparison_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_debug_output_is_redacted() {
        let request = CreateVaultRequest::current("/tmp/example.oracle", "very secret");
        let output = format!("{request:?}");
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("very secret"));
    }

    #[test]
    fn requests_reject_unknown_fields() {
        let input = r#"{"protocol_version":1,"unexpected":true}"#;
        assert!(serde_json::from_str::<ProtocolRequest>(input).is_err());
    }
}
