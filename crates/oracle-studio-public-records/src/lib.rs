//! Strict, content-addressed public records approved for Oracle Studio fixtures.

use std::collections::BTreeSet;

use astraeus_core::{
    CalculationRequest, CelestialObject, GeographicLocation, HouseSystem, UtcInstant, Zodiac,
};
use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;
const CONTENT_ID_PREFIX: &str = "sha256:";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PublicRecordCatalog {
    schema_version: u32,
    catalog_id: String,
    catalog_content_id: String,
    records: Vec<PublicRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogWire {
    schema_version: u32,
    catalog_id: String,
    catalog_content_id: String,
    records: Vec<PublicRecord>,
}

#[derive(Serialize)]
struct CanonicalCatalogRef<'a> {
    schema_version: u32,
    catalog_id: &'a str,
    records: &'a [PublicRecord],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRecord {
    record_id: String,
    content_id: String,
    title: String,
    provenance: Provenance,
    rights: Rights,
    reliability: Reliability,
    ethical_notes: String,
    chart_readiness: ChartReadiness,
    data: RecordData,
}

#[derive(Serialize)]
struct CanonicalRecordRef<'a> {
    record_id: &'a str,
    title: &'a str,
    provenance: &'a Provenance,
    rights: &'a Rights,
    reliability: &'a Reliability,
    ethical_notes: &'a str,
    chart_readiness: &'a ChartReadiness,
    data: &'a RecordData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "record_kind", rename_all = "snake_case")]
pub enum RecordData {
    PublicEvent {
        category: EventCategory,
        temporal: TemporalEvidence,
        location: LocationEvidence,
        event_metadata: EventMetadata,
    },
    DeceasedPerson {
        aliases: Vec<String>,
        birth: TemporalEvidence,
        death: TemporalEvidence,
        birthplaces: Vec<BirthplaceStatement>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    AstronomicalEvent,
    NaturalEvent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalEvidence {
    value: String,
    precision: TemporalPrecision,
    time_scale: TimeScale,
    exact_time_known: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalPrecision {
    Day,
    Second,
    Millisecond,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeScale {
    Utc,
    CivilDate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationEvidence {
    label: String,
    latitude_degrees: f64,
    longitude_degrees: f64,
    coordinate_precision_degrees: f64,
    elevation_meters: f64,
    elevation_policy: ElevationPolicy,
    iana_time_zone: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElevationPolicy {
    SurfaceForChart,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventMetadata {
    event_type: String,
    source_magnitude: Option<f64>,
    source_depth_km: Option<f64>,
    notes: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BirthplaceStatement {
    place_entity_id: String,
    label: String,
    catalog_role: BirthplaceRole,
    source_rank: SourceRank,
    statement_id: String,
    place_source_revision: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BirthplaceRole {
    Sole,
    Preferred,
    Alternate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRank {
    Preferred,
    Normal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    source_name: String,
    canonical_url: String,
    source_revision: String,
    retrieved_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rights {
    license_or_status: String,
    license_url: String,
    attribution: String,
    redistribution_notes: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reliability {
    classification: ReliabilityClass,
    source_precision: String,
    notes: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityClass {
    SourceStatedExact,
    DateOnlyExactTimeUnknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartReadiness {
    status: ChartReadinessStatus,
    reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartReadinessStatus {
    ChartReady,
    ResearchOnly,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("invalid public-record catalog JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported public-record schema version {0}")]
    UnsupportedSchema(u32),
    #[error("invalid public-record field {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },
    #[error("duplicate public record ID {0}")]
    DuplicateRecordId(String),
    #[error("public record {record_id} content ID mismatch: expected {expected}, got {actual}")]
    RecordContentId {
        record_id: String,
        expected: String,
        actual: String,
    },
    #[error("catalog content ID mismatch: expected {expected}, got {actual}")]
    CatalogContentId { expected: String, actual: String },
}

impl PublicRecordCatalog {
    pub fn from_json(input: &str) -> Result<Self, CatalogError> {
        let wire: CatalogWire = serde_json::from_str(input)?;
        let catalog = Self {
            schema_version: wire.schema_version,
            catalog_id: wire.catalog_id,
            catalog_content_id: wire.catalog_content_id,
            records: wire.records,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn to_json(&self) -> Result<String, CatalogError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn catalog_id(&self) -> &str {
        &self.catalog_id
    }

    pub fn catalog_content_id(&self) -> &str {
        &self.catalog_content_id
    }

    pub fn records(&self) -> &[PublicRecord] {
        &self.records
    }

    pub fn computed_content_id(&self) -> Result<String, CatalogError> {
        content_id(&CanonicalCatalogRef {
            schema_version: self.schema_version,
            catalog_id: &self.catalog_id,
            records: &self.records,
        })
    }

    fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(self.schema_version));
        }
        validate_stable_id("catalog_id", &self.catalog_id, "public-record-catalog.")?;
        if self.records.is_empty() {
            return invalid("records", "catalog must contain at least one record");
        }
        let mut ids = BTreeSet::new();
        for record in &self.records {
            if !ids.insert(record.record_id.clone()) {
                return Err(CatalogError::DuplicateRecordId(record.record_id.clone()));
            }
            record.validate()?;
        }
        let actual = self.computed_content_id()?;
        if self.catalog_content_id != actual {
            return Err(CatalogError::CatalogContentId {
                expected: self.catalog_content_id.clone(),
                actual,
            });
        }
        Ok(())
    }
}

impl PublicRecord {
    pub fn record_id(&self) -> &str {
        &self.record_id
    }

    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn data(&self) -> &RecordData {
        &self.data
    }

    pub fn chart_readiness(&self) -> ChartReadinessStatus {
        self.chart_readiness.status
    }

    pub fn computed_content_id(&self) -> Result<String, CatalogError> {
        content_id(&CanonicalRecordRef {
            record_id: &self.record_id,
            title: &self.title,
            provenance: &self.provenance,
            rights: &self.rights,
            reliability: &self.reliability,
            ethical_notes: &self.ethical_notes,
            chart_readiness: &self.chart_readiness,
            data: &self.data,
        })
    }

    /// Build the deterministic production-style Moshier request for approved events.
    /// Date-only people deliberately return `None`; no synthetic noon is invented.
    pub fn chart_request(&self) -> Result<Option<CalculationRequest>, CatalogError> {
        let RecordData::PublicEvent {
            temporal, location, ..
        } = &self.data
        else {
            return Ok(None);
        };
        if self.chart_readiness.status != ChartReadinessStatus::ChartReady {
            return Ok(None);
        }
        let request = CalculationRequest::new(
            UtcInstant::parse_rfc3339(&temporal.value)
                .map_err(|error| field_error("data.temporal.value", error))?,
            GeographicLocation::new(
                location.latitude_degrees,
                location.longitude_degrees,
                location.elevation_meters,
            )
            .map_err(|error| field_error("data.location", error))?,
            vec![
                CelestialObject::Sun,
                CelestialObject::Moon,
                CelestialObject::Mercury,
                CelestialObject::Venus,
                CelestialObject::Mars,
                CelestialObject::Jupiter,
                CelestialObject::Saturn,
                CelestialObject::Uranus,
                CelestialObject::Neptune,
                CelestialObject::Pluto,
                CelestialObject::MeanNode,
                CelestialObject::TrueNode,
            ],
            Zodiac::Tropical,
            None,
            HouseSystem::Placidus,
        )
        .map_err(|error| field_error("data", error))?;
        Ok(Some(request))
    }

    fn validate(&self) -> Result<(), CatalogError> {
        validate_stable_id("record_id", &self.record_id, "public-record.")?;
        validate_text("title", &self.title)?;
        self.provenance.validate()?;
        self.rights.validate()?;
        self.reliability.validate()?;
        validate_text("ethical_notes", &self.ethical_notes)?;
        validate_text("chart_readiness.reason", &self.chart_readiness.reason)?;
        match &self.data {
            RecordData::PublicEvent {
                temporal,
                location,
                event_metadata,
                ..
            } => {
                temporal.validate()?;
                location.validate()?;
                event_metadata.validate()?;
                if self.chart_readiness.status == ChartReadinessStatus::ChartReady
                    && (!temporal.exact_time_known || temporal.time_scale != TimeScale::Utc)
                {
                    return invalid(
                        "chart_readiness",
                        "chart-ready events require an exact UTC instant",
                    );
                }
            }
            RecordData::DeceasedPerson {
                aliases,
                birth,
                death,
                birthplaces,
            } => {
                if self.chart_readiness.status != ChartReadinessStatus::ResearchOnly {
                    return invalid("chart_readiness", "date-only people are research-only");
                }
                validate_unique_text("data.aliases", aliases)?;
                birth.validate_date_only("data.birth")?;
                death.validate_date_only("data.death")?;
                validate_birthplaces(birthplaces)?;
            }
        }
        let actual = self.computed_content_id()?;
        if self.content_id != actual {
            return Err(CatalogError::RecordContentId {
                record_id: self.record_id.clone(),
                expected: self.content_id.clone(),
                actual,
            });
        }
        Ok(())
    }
}

impl TemporalEvidence {
    fn validate(&self) -> Result<(), CatalogError> {
        match (self.precision, self.time_scale, self.exact_time_known) {
            (TemporalPrecision::Day, TimeScale::CivilDate, false) => {
                NaiveDate::parse_from_str(&self.value, "%Y-%m-%d")
                    .map_err(|error| field_error("data.temporal.value", error))?;
            }
            (TemporalPrecision::Second, TimeScale::Utc, true) => {
                validate_utc(&self.value, false)?;
            }
            (TemporalPrecision::Millisecond, TimeScale::Utc, true) => {
                validate_utc(&self.value, true)?;
            }
            _ => {
                return invalid(
                    "data.temporal",
                    "precision, time scale, and exact-time flag are inconsistent",
                );
            }
        }
        Ok(())
    }

    fn validate_date_only(&self, field: &'static str) -> Result<(), CatalogError> {
        self.validate()?;
        if self.precision != TemporalPrecision::Day
            || self.time_scale != TimeScale::CivilDate
            || self.exact_time_known
        {
            return invalid(
                field,
                "person dates must be civil dates with unknown exact time",
            );
        }
        Ok(())
    }
}

impl LocationEvidence {
    fn validate(&self) -> Result<(), CatalogError> {
        validate_text("data.location.label", &self.label)?;
        GeographicLocation::new(
            self.latitude_degrees,
            self.longitude_degrees,
            self.elevation_meters,
        )
        .map_err(|error| field_error("data.location", error))?;
        if !self.coordinate_precision_degrees.is_finite()
            || !(0.0..=10.0).contains(&self.coordinate_precision_degrees)
            || self.coordinate_precision_degrees == 0.0
        {
            return invalid(
                "data.location.coordinate_precision_degrees",
                "must be finite and in (0, 10]",
            );
        }
        if let Some(zone) = &self.iana_time_zone {
            validate_text("data.location.iana_time_zone", zone)?;
        }
        Ok(())
    }
}

impl EventMetadata {
    fn validate(&self) -> Result<(), CatalogError> {
        validate_text("data.event_metadata.event_type", &self.event_type)?;
        validate_optional_nonnegative(
            "data.event_metadata.source_magnitude",
            self.source_magnitude,
        )?;
        validate_optional_nonnegative("data.event_metadata.source_depth_km", self.source_depth_km)?;
        validate_text("data.event_metadata.notes", &self.notes)
    }
}

impl Provenance {
    fn validate(&self) -> Result<(), CatalogError> {
        validate_text("provenance.source_name", &self.source_name)?;
        validate_https_url("provenance.canonical_url", &self.canonical_url)?;
        validate_text("provenance.source_revision", &self.source_revision)?;
        validate_utc(&self.retrieved_at, false)
    }
}

impl Rights {
    fn validate(&self) -> Result<(), CatalogError> {
        validate_text("rights.license_or_status", &self.license_or_status)?;
        validate_https_url("rights.license_url", &self.license_url)?;
        validate_text("rights.attribution", &self.attribution)?;
        validate_text("rights.redistribution_notes", &self.redistribution_notes)
    }
}

impl Reliability {
    fn validate(&self) -> Result<(), CatalogError> {
        validate_text("reliability.source_precision", &self.source_precision)?;
        validate_text("reliability.notes", &self.notes)
    }
}

fn validate_birthplaces(statements: &[BirthplaceStatement]) -> Result<(), CatalogError> {
    if statements.is_empty() {
        return invalid(
            "data.birthplaces",
            "at least one source statement is required",
        );
    }
    let mut ids = BTreeSet::new();
    let mut primary = 0;
    for statement in statements {
        let entity_number = statement.place_entity_id.strip_prefix('Q');
        if entity_number.is_none_or(|number| {
            number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            return invalid(
                "data.birthplaces.place_entity_id",
                "must be a Wikidata Q identifier",
            );
        }
        validate_text("data.birthplaces.label", &statement.label)?;
        validate_text("data.birthplaces.statement_id", &statement.statement_id)?;
        validate_text(
            "data.birthplaces.place_source_revision",
            &statement.place_source_revision,
        )?;
        if !ids.insert(&statement.statement_id) {
            return invalid("data.birthplaces", "statement IDs must be unique");
        }
        if statement.catalog_role != BirthplaceRole::Alternate {
            primary += 1;
        }
    }
    if primary != 1 {
        return invalid(
            "data.birthplaces",
            "exactly one sole or preferred birthplace is required",
        );
    }
    Ok(())
}

fn validate_unique_text(field: &'static str, values: &[String]) -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_text(field, value)?;
        if !seen.insert(value) {
            return invalid(field, "values must be unique");
        }
    }
    Ok(())
}

fn validate_stable_id(field: &'static str, value: &str, prefix: &str) -> Result<(), CatalogError> {
    if !value.starts_with(prefix)
        || value.len() == prefix.len()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return invalid(field, format!("must use the {prefix} stable-ID namespace"));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), CatalogError> {
    if value.trim().is_empty() || value.trim() != value {
        invalid(field, "must be non-empty without surrounding whitespace")
    } else {
        Ok(())
    }
}

fn validate_https_url(field: &'static str, value: &str) -> Result<(), CatalogError> {
    validate_text(field, value)?;
    if value.starts_with("https://") && !value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        Ok(())
    } else {
        invalid(field, "must be an HTTPS URL")
    }
}

fn validate_utc(value: &str, milliseconds: bool) -> Result<(), CatalogError> {
    if !value.ends_with('Z') {
        return invalid("timestamp", "must use canonical UTC Z notation");
    }
    let fraction = value
        .strip_suffix('Z')
        .and_then(|without_zone| without_zone.rsplit_once('.'))
        .map(|(_, fraction)| fraction);
    if (milliseconds && fraction.is_none_or(|part| part.len() != 3))
        || (!milliseconds && fraction.is_some())
    {
        return invalid(
            "timestamp",
            "fractional precision does not match the schema",
        );
    }
    DateTime::parse_from_rfc3339(value).map_err(|error| field_error("timestamp", error))?;
    Ok(())
}

fn validate_optional_nonnegative(
    field: &'static str,
    value: Option<f64>,
) -> Result<(), CatalogError> {
    if value.is_some_and(|number| !number.is_finite() || number < 0.0) {
        invalid(field, "must be finite and non-negative")
    } else {
        Ok(())
    }
}

fn content_id(value: &impl Serialize) -> Result<String, CatalogError> {
    Ok(format!(
        "{CONTENT_ID_PREFIX}{:x}",
        Sha256::digest(serde_json::to_vec(value)?)
    ))
}

fn invalid<T>(field: &'static str, reason: impl Into<String>) -> Result<T, CatalogError> {
    Err(CatalogError::InvalidField {
        field,
        reason: reason.into(),
    })
}

fn field_error(field: &'static str, error: impl std::fmt::Display) -> CatalogError {
    CatalogError::InvalidField {
        field,
        reason: error.to_string(),
    }
}
