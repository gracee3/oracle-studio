//! Validated chart-only document records for browser-local Oracle Studio vaults.

use std::collections::BTreeSet;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod studio;
mod workbench;

pub use studio::*;
pub use workbench::*;

pub const VAULT_DOCUMENT_SCHEMA_VERSION: u32 = 4;
pub const ASTRAEUS_REVISION: &str = "8637ceb64fa11a06c8680b46cb4b57c71d94d37f";
const MAX_TEXT_BYTES: usize = 32 * 1024;

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

impl<'de> Deserialize<'de> for StableId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new("stable_id", String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonKind {
    Personal,
    ProfessionalClient,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        let person = Self {
            id,
            display_name: display_name.into(),
            kind,
            notes,
        };
        person.validate()?;
        Ok(person)
    }

    fn validate(&self) -> Result<(), ModelError> {
        validate_text("person.display_name", &self.display_name)?;
        validate_optional_text("person.notes", self.notes.as_deref())
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

#[derive(Clone, Debug, PartialEq)]
pub struct VaultDocument {
    people: Vec<PersonProfile>,
    saved_locations: Vec<SavedLocation>,
    chart_definitions: Vec<ChartDefinition>,
    chart_calculations: Vec<ChartCalculation>,
    comparison_presets: Vec<ComparisonPreset>,
    comparison_calculations: Vec<ComparisonCalculation>,
    workspace_state: WorkspaceState,
}

#[derive(Serialize)]
struct VaultDocumentRef<'a> {
    schema_version: u32,
    people: &'a [PersonProfile],
    saved_locations: &'a [SavedLocation],
    chart_definitions: &'a [ChartDefinition],
    chart_calculations: &'a [ChartCalculation],
    comparison_presets: &'a [ComparisonPreset],
    comparison_calculations: &'a [ComparisonCalculation],
    workspace_state: &'a WorkspaceState,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultDocumentWire {
    schema_version: u32,
    people: Vec<PersonProfile>,
    saved_locations: Vec<SavedLocation>,
    chart_definitions: Vec<ChartDefinition>,
    chart_calculations: Vec<ChartCalculation>,
    comparison_presets: Vec<ComparisonPreset>,
    comparison_calculations: Vec<ComparisonCalculation>,
    workspace_state: WorkspaceState,
}

#[derive(Deserialize)]
struct VaultSchemaProbe {
    schema_version: u32,
}

impl VaultDocument {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        people: Vec<PersonProfile>,
        saved_locations: Vec<SavedLocation>,
        chart_definitions: Vec<ChartDefinition>,
        chart_calculations: Vec<ChartCalculation>,
        comparison_presets: Vec<ComparisonPreset>,
        comparison_calculations: Vec<ComparisonCalculation>,
        workspace_state: WorkspaceState,
    ) -> Result<Self, ModelError> {
        let document = Self {
            people,
            saved_locations,
            chart_definitions,
            chart_calculations,
            comparison_presets,
            comparison_calculations,
            workspace_state,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn empty() -> Self {
        Self {
            people: Vec::new(),
            saved_locations: Vec::new(),
            chart_definitions: Vec::new(),
            chart_calculations: Vec::new(),
            comparison_presets: Vec::new(),
            comparison_calculations: Vec::new(),
            workspace_state: WorkspaceState::default(),
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        validate_unique(self.people.iter().map(PersonProfile::id), "person")?;
        for person in &self.people {
            person.validate()?;
        }
        let people = self.people.iter().map(PersonProfile::id).collect();
        validate_studio_records(
            &people,
            &self.saved_locations,
            &self.chart_definitions,
            &self.chart_calculations,
            &self.comparison_presets,
            &self.comparison_calculations,
            &self.workspace_state,
        )
    }

    pub fn people(&self) -> &[PersonProfile] {
        &self.people
    }
    pub fn saved_locations(&self) -> &[SavedLocation] {
        &self.saved_locations
    }
    pub fn chart_definitions(&self) -> &[ChartDefinition] {
        &self.chart_definitions
    }
    pub fn chart_calculations(&self) -> &[ChartCalculation] {
        &self.chart_calculations
    }
    pub fn comparison_presets(&self) -> &[ComparisonPreset] {
        &self.comparison_presets
    }
    pub fn comparison_calculations(&self) -> &[ComparisonCalculation] {
        &self.comparison_calculations
    }
    pub const fn workspace_state(&self) -> &WorkspaceState {
        &self.workspace_state
    }

    pub fn with_person(mut self, person: PersonProfile) -> Result<Self, ModelError> {
        replace_by_id(&mut self.people, person, PersonProfile::id);
        self.validate()?;
        Ok(self)
    }

    pub fn with_location(mut self, location: SavedLocation) -> Result<Self, ModelError> {
        replace_by_id(&mut self.saved_locations, location, SavedLocation::id);
        self.validate()?;
        Ok(self)
    }

    pub fn with_chart(mut self, chart: ChartDefinition) -> Result<Self, ModelError> {
        replace_by_id(&mut self.chart_definitions, chart, ChartDefinition::id);
        self.validate()?;
        Ok(self)
    }

    pub fn with_chart_calculation(
        mut self,
        calculation: ChartCalculation,
    ) -> Result<Self, ModelError> {
        if self
            .chart_calculations
            .iter()
            .any(|existing| existing.id() == calculation.id())
        {
            return Err(ModelError::ImmutableRecord("chart calculation"));
        }
        let chart = self
            .chart_definitions
            .iter_mut()
            .find(|chart| chart.id() == calculation.chart_definition_id())
            .ok_or(ModelError::DanglingReference(
                "chart_calculation.chart_definition_id",
            ))?;
        chart.set_current_calculation(calculation.id().clone());
        self.chart_calculations.push(calculation);
        self.validate()?;
        Ok(self)
    }

    pub fn with_comparison(mut self, preset: ComparisonPreset) -> Result<Self, ModelError> {
        replace_by_id(&mut self.comparison_presets, preset, ComparisonPreset::id);
        self.validate()?;
        Ok(self)
    }

    pub fn with_comparison_calculation(
        mut self,
        calculation: ComparisonCalculation,
    ) -> Result<Self, ModelError> {
        if self
            .comparison_calculations
            .iter()
            .any(|existing| existing.id() == calculation.id())
        {
            return Err(ModelError::ImmutableRecord("comparison calculation"));
        }
        let preset = self
            .comparison_presets
            .iter_mut()
            .find(|preset| preset.id() == calculation.comparison_preset_id())
            .ok_or(ModelError::DanglingReference(
                "comparison_calculation.comparison_preset_id",
            ))?;
        preset.set_current_calculation(calculation.id().clone());
        self.comparison_calculations.push(calculation);
        self.validate()?;
        Ok(self)
    }

    pub fn with_workspace(mut self, workspace: WorkspaceState) -> Result<Self, ModelError> {
        self.workspace_state = workspace;
        self.validate()?;
        Ok(self)
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        self.validate()?;
        Ok(serde_json::to_string(&VaultDocumentRef {
            schema_version: VAULT_DOCUMENT_SCHEMA_VERSION,
            people: &self.people,
            saved_locations: &self.saved_locations,
            chart_definitions: &self.chart_definitions,
            chart_calculations: &self.chart_calculations,
            comparison_presets: &self.comparison_presets,
            comparison_calculations: &self.comparison_calculations,
            workspace_state: &self.workspace_state,
        })?)
    }

    pub fn from_json(input: &str) -> Result<Self, ModelError> {
        let probe: VaultSchemaProbe = serde_json::from_str(input)?;
        if probe.schema_version != VAULT_DOCUMENT_SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema(probe.schema_version));
        }
        let wire: VaultDocumentWire = serde_json::from_str(input)?;
        if wire.schema_version != VAULT_DOCUMENT_SCHEMA_VERSION {
            return Err(ModelError::UnsupportedSchema(wire.schema_version));
        }
        Self::new(
            wire.people,
            wire.saved_locations,
            wire.chart_definitions,
            wire.chart_calculations,
            wire.comparison_presets,
            wire.comparison_calculations,
            wire.workspace_state,
        )
    }
}

fn replace_by_id<T>(items: &mut Vec<T>, item: T, id: impl Fn(&T) -> &StableId) {
    if let Some(index) = items.iter().position(|existing| id(existing) == id(&item)) {
        items[index] = item;
    } else {
        items.push(item);
    }
}

fn validate_unique<'a>(
    ids: impl Iterator<Item = &'a StableId>,
    kind: &'static str,
) -> Result<(), ModelError> {
    let mut seen = BTreeSet::new();
    if ids.into_iter().any(|id| !seen.insert(id)) {
        Err(ModelError::DuplicateId(kind))
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("invalid stable ID in {field}")]
    InvalidId { field: &'static str },
    #[error("{0} must not be blank")]
    EmptyText(&'static str),
    #[error("{0} exceeds its text size bound")]
    TextTooLong(&'static str),
    #[error("invalid RFC 3339 timestamp in {0}")]
    InvalidTimestamp(&'static str),
    #[error("duplicate {0} ID")]
    DuplicateId(&'static str),
    #[error("dangling reference in {0}")]
    DanglingReference(&'static str),
    #[error("unsupported vault document schema version {0}")]
    UnsupportedSchema(u32),
    #[error("invalid value in {0}")]
    InvalidValue(&'static str),
    #[error("a person may have only one default natal chart")]
    DuplicateDefaultNatal,
    #[error("a default natal chart must have a person and natal role")]
    InvalidDefaultNatal,
    #[error("chart current calculation does not belong to that chart")]
    CalculationChartMismatch,
    #[error("comparison preset sources are inconsistent")]
    InvalidComparisonSources,
    #[error("ambiguous local time requires an explicit earlier or later choice")]
    AmbiguousLocalTime,
    #[error("local time does not exist in the selected time zone")]
    NonexistentLocalTime,
    #[error("an ambiguity choice was supplied for a unique local time")]
    UnexpectedAmbiguousTimeChoice,
    #[error("{0} records are immutable")]
    ImmutableRecord(&'static str),
    #[error("invalid vault document JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn validate_text(field: &'static str, value: &str) -> Result<(), ModelError> {
    if value.trim().is_empty() {
        Err(ModelError::EmptyText(field))
    } else if value.len() > MAX_TEXT_BYTES {
        Err(ModelError::TextTooLong(field))
    } else {
        Ok(())
    }
}

fn validate_optional_text(field: &'static str, value: Option<&str>) -> Result<(), ModelError> {
    value.map_or(Ok(()), |value| validate_text(field, value))
}

pub(crate) fn validate_content_id(value: &str) -> Result<(), ModelError> {
    let Some(hash) = value.strip_prefix("sha256:") else {
        return Err(ModelError::InvalidValue("content_id"));
    };
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ModelError::InvalidValue("content_id"));
    }
    Ok(())
}

pub(crate) fn normalize_timestamp(
    field: &'static str,
    value: String,
) -> Result<String, ModelError> {
    let timestamp = DateTime::parse_from_rfc3339(&value)
        .map_err(|_| ModelError::InvalidTimestamp(field))?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    if timestamp == value {
        Ok(timestamp)
    } else {
        Err(ModelError::InvalidTimestamp(field))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_schema_v4_round_trip_is_canonical_and_chart_only() {
        let document = VaultDocument::empty();
        let json = document.to_json().unwrap();
        assert_eq!(VaultDocument::from_json(&json).unwrap(), document);
        assert!(json.starts_with("{\"schema_version\":4,\"people\":"));
        for removed in [
            "sessions",
            "artifacts",
            "journal_entries",
            "deck_pack",
            "tarot",
        ] {
            assert!(!json.contains(removed));
        }
    }

    #[test]
    fn every_older_schema_is_rejected_without_migration() {
        for version in 0..VAULT_DOCUMENT_SCHEMA_VERSION {
            let json = format!("{{\"schema_version\":{version}}}");
            assert!(matches!(
                VaultDocument::from_json(&json),
                Err(ModelError::UnsupportedSchema(rejected)) if rejected == version
            ));
        }
    }

    #[test]
    fn hostile_but_valid_text_round_trips_and_is_bounded() {
        let person = PersonProfile::new(
            StableId::new("person.id", "hostile").unwrap(),
            "<script>\"&\\u{2028}",
            PersonKind::Personal,
            Some("line one\nline two\t\u{0}".into()),
        )
        .unwrap();
        let document = VaultDocument::empty().with_person(person).unwrap();
        assert_eq!(
            VaultDocument::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
        assert!(matches!(
            PersonProfile::new(
                StableId::new("person.id", "too_long").unwrap(),
                "x".repeat(MAX_TEXT_BYTES + 1),
                PersonKind::Personal,
                None,
            ),
            Err(ModelError::TextTooLong("person.display_name"))
        ));
    }

    #[test]
    fn default_natal_uniqueness_and_current_result_references_are_enforced() {
        let person_id = StableId::new("person.id", "fictional").unwrap();
        let person = PersonProfile::new(
            person_id.clone(),
            "Fictional Person",
            PersonKind::Personal,
            None,
        )
        .unwrap();
        let natal = |id: &str| {
            ChartDefinition::new(
                StableId::new("chart.id", id).unwrap(),
                format!("Natal {id}"),
                ChartRole::Natal,
                Some(person_id.clone()),
                LocalDateTimeInput::new("2000-01-15", "12:00", "America/New_York").unwrap(),
                ChartCalculationOptions::default(),
                default_chart_points(),
                true,
            )
            .unwrap()
        };
        let document = VaultDocument::empty()
            .with_person(person)
            .unwrap()
            .with_chart(natal("first"))
            .unwrap();
        assert!(matches!(
            document.clone().with_chart(natal("second")),
            Err(ModelError::DuplicateDefaultNatal)
        ));

        let mut dangling = ChartDefinition::new(
            StableId::new("chart.id", "transit").unwrap(),
            "Transit",
            ChartRole::Transit,
            None,
            LocalDateTimeInput::new("2026-08-19", "12:00", "America/New_York").unwrap(),
            ChartCalculationOptions::default(),
            default_chart_points(),
            false,
        )
        .unwrap();
        dangling.set_current_calculation(StableId::new("calculation.id", "missing").unwrap());
        assert!(matches!(
            document.with_chart(dangling),
            Err(ModelError::DanglingReference(
                "chart_definition.current_calculation_id"
            ))
        ));
    }
}
