//! Pure, bounded GeoNames parsing and deterministic local search.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const CITIES500_PATH: &str = "catalog/geonames/cities500.zip";
pub const ADMIN1_CODES_PATH: &str = "catalog/geonames/admin1CodesASCII.txt";
pub const ADMIN2_CODES_PATH: &str = "catalog/geonames/admin2Codes.txt";
pub const DISTRIBUTION_URL: &str = "https://download.geonames.org/export/dump/";
pub const CITIES500_URL: &str = "https://download.geonames.org/export/dump/cities500.zip";
pub const ADMIN1_CODES_URL: &str = "https://download.geonames.org/export/dump/admin1CodesASCII.txt";
pub const ADMIN2_CODES_URL: &str = "https://download.geonames.org/export/dump/admin2Codes.txt";
pub const LICENSE_NAME: &str = "CC BY 4.0";
pub const LICENSE_URL: &str = "https://creativecommons.org/licenses/by/4.0/";
pub const ATTRIBUTION: &str = "Contains GeoNames geographical data, available under CC BY 4.0.";

const CATALOG_SCHEMA_VERSION: u32 = 1;
pub const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
const MAX_CITIES_BYTES: usize = 512 * 1024 * 1024;
const MAX_ADMIN_BYTES: usize = 64 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 64 * 1024;
const MAX_ROWS: usize = 20_000_000;
const MAX_ALIASES_PER_PLACE: usize = 256;
const MAX_QUERY_CHARS: usize = 256;
const MAX_RESULTS: usize = 50;
const CITIES_FILE: &str = "cities500.txt";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogInstallInput {
    pub cities500_zip: Vec<u8>,
    pub admin1_codes: Vec<u8>,
    pub admin2_codes: Vec<u8>,
    pub retrieved_at: String,
    pub retrieval: CatalogRetrieval,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogRetrieval {
    SameOriginPinned { manifest_sha256: String },
    LocalFiles,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNamesSourceFile {
    pub source: String,
    pub sha256: String,
    pub byte_length: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMetadata {
    pub schema_version: u32,
    pub content_id: String,
    pub retrieved_at: String,
    pub retrieval: CatalogRetrieval,
    pub place_count: usize,
    pub cities_archive: GeoNamesSourceFile,
    pub admin1_codes: GeoNamesSourceFile,
    pub admin2_codes: GeoNamesSourceFile,
    pub distribution_url: String,
    pub license_name: String,
    pub license_url: String,
    pub attribution: String,
}

impl CatalogMetadata {
    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(self.schema_version));
        }
        validate_content_id(&self.content_id)?;
        for source in [&self.cities_archive, &self.admin1_codes, &self.admin2_codes] {
            validate_sha256(&source.sha256)?;
            if source.byte_length == 0 || source.source.is_empty() {
                return Err(CatalogError::InvalidMetadata);
            }
        }
        normalize_timestamp(&self.retrieved_at)?;
        if self.place_count == 0
            || self.distribution_url != DISTRIBUTION_URL
            || self.license_name != LICENSE_NAME
            || self.license_url != LICENSE_URL
            || self.attribution != ATTRIBUTION
        {
            return Err(CatalogError::InvalidMetadata);
        }
        if let CatalogRetrieval::SameOriginPinned { manifest_sha256 } = &self.retrieval {
            validate_sha256(manifest_sha256)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,
    Prefix,
    Substring,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPlace {
    geonames_id: u64,
    name: String,
    administrative_names: Vec<String>,
    country_code: String,
    latitude_degrees: f64,
    longitude_degrees: f64,
    elevation_meters: Option<f64>,
    time_zone: String,
    population: u64,
    normalized_names: Vec<String>,
}

impl CatalogPlace {
    pub const fn geonames_id(&self) -> u64 {
        self.geonames_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn administrative_names(&self) -> &[String] {
        &self.administrative_names
    }
    pub fn country_code(&self) -> &str {
        &self.country_code
    }
    pub fn latitude_degrees(&self) -> f64 {
        self.latitude_degrees
    }
    pub fn longitude_degrees(&self) -> f64 {
        self.longitude_degrees
    }
    pub fn elevation_meters(&self) -> Option<f64> {
        self.elevation_meters
    }
    pub fn time_zone(&self) -> &str {
        &self.time_zone
    }
    pub const fn population(&self) -> u64 {
        self.population
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSearchMatch {
    place: CatalogPlace,
    match_kind: MatchKind,
}

impl CatalogSearchMatch {
    pub fn place(&self) -> &CatalogPlace {
        &self.place
    }
    pub const fn match_kind(&self) -> MatchKind {
        self.match_kind
    }
}

#[derive(Clone, Debug)]
pub struct LocationCatalog {
    metadata: CatalogMetadata,
    places: Vec<CatalogPlace>,
}

impl LocationCatalog {
    pub fn from_distribution(input: &CatalogInstallInput) -> Result<Self, CatalogError> {
        if input.cities500_zip.is_empty()
            || input.admin1_codes.is_empty()
            || input.admin2_codes.is_empty()
            || input.cities500_zip.len() > MAX_ARCHIVE_BYTES
            || input.admin1_codes.len() > MAX_ADMIN_BYTES
            || input.admin2_codes.len() > MAX_ADMIN_BYTES
        {
            return Err(CatalogError::SizeLimit);
        }
        let cities = extract_cities500(&input.cities500_zip)?;
        let administrative = AdministrativeNames::parse(&input.admin1_codes, &input.admin2_codes)?;
        let places = parse_places(&cities, &administrative)?;
        if places.is_empty() {
            return Err(CatalogError::EmptyCatalog);
        }
        let metadata = CatalogMetadata {
            schema_version: CATALOG_SCHEMA_VERSION,
            content_id: catalog_content_id(&cities, &input.admin1_codes, &input.admin2_codes),
            retrieved_at: normalize_timestamp(&input.retrieved_at)?,
            retrieval: input.retrieval.clone(),
            place_count: places.len(),
            cities_archive: GeoNamesSourceFile {
                source: match input.retrieval {
                    CatalogRetrieval::SameOriginPinned { .. } => CITIES500_PATH.into(),
                    CatalogRetrieval::LocalFiles => "cities500.zip".into(),
                },
                sha256: sha256(&input.cities500_zip),
                byte_length: input.cities500_zip.len(),
            },
            admin1_codes: GeoNamesSourceFile {
                source: match input.retrieval {
                    CatalogRetrieval::SameOriginPinned { .. } => ADMIN1_CODES_PATH.into(),
                    CatalogRetrieval::LocalFiles => "admin1CodesASCII.txt".into(),
                },
                sha256: sha256(&input.admin1_codes),
                byte_length: input.admin1_codes.len(),
            },
            admin2_codes: GeoNamesSourceFile {
                source: match input.retrieval {
                    CatalogRetrieval::SameOriginPinned { .. } => ADMIN2_CODES_PATH.into(),
                    CatalogRetrieval::LocalFiles => "admin2Codes.txt".into(),
                },
                sha256: sha256(&input.admin2_codes),
                byte_length: input.admin2_codes.len(),
            },
            distribution_url: DISTRIBUTION_URL.into(),
            license_name: LICENSE_NAME.into(),
            license_url: LICENSE_URL.into(),
            attribution: ATTRIBUTION.into(),
        };
        metadata.validate()?;
        Ok(Self { metadata, places })
    }

    pub fn metadata(&self) -> &CatalogMetadata {
        &self.metadata
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CatalogSearchMatch>, CatalogError> {
        let query = normalize_query(query)?;
        let mut matches = self
            .places
            .iter()
            .filter_map(|place| {
                let match_kind = place
                    .normalized_names
                    .iter()
                    .filter_map(|name| match_kind(name, &query))
                    .min()?;
                Some(CatalogSearchMatch {
                    place: place.clone(),
                    match_kind,
                })
            })
            .collect::<Vec<_>>();
        matches.sort_by(|first, second| {
            first
                .match_kind
                .cmp(&second.match_kind)
                .then_with(|| second.place.population.cmp(&first.place.population))
                .then_with(|| first.place.geonames_id.cmp(&second.place.geonames_id))
        });
        matches.truncate(limit.clamp(1, MAX_RESULTS));
        Ok(matches)
    }
}

struct AdministrativeNames {
    admin1: BTreeMap<String, String>,
    admin2: BTreeMap<String, String>,
}

impl AdministrativeNames {
    fn parse(admin1: &[u8], admin2: &[u8]) -> Result<Self, CatalogError> {
        Ok(Self {
            admin1: parse_admin_codes(admin1)?,
            admin2: parse_admin_codes(admin2)?,
        })
    }
}

fn parse_admin_codes(input: &[u8]) -> Result<BTreeMap<String, String>, CatalogError> {
    let text = std::str::from_utf8(input).map_err(|_| CatalogError::InvalidUtf8)?;
    let mut result = BTreeMap::new();
    for line in text.lines() {
        if line.len() > MAX_LINE_BYTES {
            return Err(CatalogError::LineLimit);
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let code = fields.next().unwrap_or_default();
        let name = fields.next().unwrap_or_default();
        if code.is_empty() || name.trim().is_empty() {
            return Err(CatalogError::InvalidAdminRecord);
        }
        result.insert(code.to_owned(), name.to_owned());
        if result.len() > MAX_ROWS {
            return Err(CatalogError::RowLimit);
        }
    }
    Ok(result)
}

fn parse_places(
    input: &[u8],
    administrative: &AdministrativeNames,
) -> Result<Vec<CatalogPlace>, CatalogError> {
    let text = std::str::from_utf8(input).map_err(|_| CatalogError::InvalidUtf8)?;
    let mut places = Vec::new();
    let mut ids = BTreeSet::new();
    for line in text.lines() {
        if line.len() > MAX_LINE_BYTES {
            return Err(CatalogError::LineLimit);
        }
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 19 {
            return Err(CatalogError::InvalidPlaceRecord);
        }
        let geonames_id = parse_required::<u64>(fields[0])?;
        if geonames_id == 0 || !ids.insert(geonames_id) {
            return Err(CatalogError::InvalidPlaceRecord);
        }
        let name = fields[1].trim();
        let latitude_degrees = parse_required::<f64>(fields[4])?;
        let longitude_degrees = parse_required::<f64>(fields[5])?;
        if name.is_empty()
            || !latitude_degrees.is_finite()
            || !(-90.0..=90.0).contains(&latitude_degrees)
            || !longitude_degrees.is_finite()
            || !(-180.0..=180.0).contains(&longitude_degrees)
        {
            return Err(CatalogError::InvalidPlaceRecord);
        }
        let country_code = fields[8];
        if country_code.len() != 2 || !country_code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(CatalogError::InvalidPlaceRecord);
        }
        let population = if fields[14].is_empty() {
            0
        } else {
            parse_required(fields[14])?
        };
        let elevation_meters = if fields[15].is_empty() {
            None
        } else {
            let value = parse_required::<f64>(fields[15])?;
            if !value.is_finite() || !(-500.0..=10_000.0).contains(&value) {
                return Err(CatalogError::InvalidPlaceRecord);
            }
            Some(value)
        };
        let time_zone = fields[17];
        if time_zone.parse::<chrono_tz::Tz>().is_err() {
            return Err(CatalogError::InvalidPlaceRecord);
        }
        let admin1_key = format!("{country_code}.{}", fields[10]);
        let admin2_key = format!("{admin1_key}.{}", fields[11]);
        let mut administrative_names = Vec::new();
        if let Some(value) = administrative.admin2.get(&admin2_key) {
            administrative_names.push(value.clone());
        }
        if let Some(value) = administrative.admin1.get(&admin1_key)
            && !administrative_names.contains(value)
        {
            administrative_names.push(value.clone());
        }
        let mut normalized_names = Vec::new();
        for candidate in std::iter::once(name)
            .chain(std::iter::once(fields[2]).filter(|value| !value.is_empty()))
            .chain(fields[3].split(',').filter(|value| !value.is_empty()))
        {
            let normalized = candidate.trim().to_lowercase();
            if !normalized.is_empty()
                && normalized_names.len() < MAX_ALIASES_PER_PLACE
                && !normalized_names.contains(&normalized)
            {
                normalized_names.push(normalized);
            }
        }
        places.push(CatalogPlace {
            geonames_id,
            name: name.into(),
            administrative_names,
            country_code: country_code.into(),
            latitude_degrees,
            longitude_degrees,
            elevation_meters,
            time_zone: time_zone.into(),
            population,
            normalized_names,
        });
        if places.len() > MAX_ROWS {
            return Err(CatalogError::RowLimit);
        }
    }
    Ok(places)
}

fn parse_required<T: std::str::FromStr>(value: &str) -> Result<T, CatalogError> {
    value.parse().map_err(|_| CatalogError::InvalidPlaceRecord)
}

fn extract_cities500(archive: &[u8]) -> Result<Vec<u8>, CatalogError> {
    if archive.len() > MAX_ARCHIVE_BYTES {
        return Err(CatalogError::SizeLimit);
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(archive))?;
    let entry = archive
        .by_name(CITIES_FILE)
        .map_err(|_| CatalogError::MissingCitiesFile)?;
    if entry.is_dir() || entry.size() > MAX_CITIES_BYTES as u64 {
        return Err(CatalogError::SizeLimit);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry
        .take(MAX_CITIES_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CITIES_BYTES {
        return Err(CatalogError::SizeLimit);
    }
    Ok(bytes)
}

fn normalize_query(query: &str) -> Result<String, CatalogError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(CatalogError::EmptyQuery);
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(CatalogError::QueryLimit);
    }
    Ok(query.to_lowercase())
}

fn match_kind(name: &str, query: &str) -> Option<MatchKind> {
    if name == query {
        Some(MatchKind::Exact)
    } else if name.starts_with(query) {
        Some(MatchKind::Prefix)
    } else if name.contains(query) {
        Some(MatchKind::Substring)
    } else {
        None
    }
}

fn normalize_timestamp(value: &str) -> Result<String, CatalogError> {
    let normalized = DateTime::parse_from_rfc3339(value)
        .map_err(|_| CatalogError::InvalidTimestamp)?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    if normalized == value {
        Ok(normalized)
    } else {
        Err(CatalogError::InvalidTimestamp)
    }
}

fn catalog_content_id(cities: &[u8], admin1: &[u8], admin2: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"oracle-studio-geonames-cities500-v1\0");
    for bytes in [cities, admin1, admin2] {
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    format!("sha256:{:x}", digest.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_content_id(value: &str) -> Result<(), CatalogError> {
    validate_sha256(
        value
            .strip_prefix("sha256:")
            .ok_or(CatalogError::InvalidContentId)?,
    )
}

fn validate_sha256(value: &str) -> Result<(), CatalogError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(CatalogError::InvalidContentId)
    }
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog input exceeds its configured size bound")]
    SizeLimit,
    #[error("catalog input exceeds its configured row bound")]
    RowLimit,
    #[error("catalog line exceeds its configured size bound")]
    LineLimit,
    #[error("GeoNames input is not valid UTF-8")]
    InvalidUtf8,
    #[error("GeoNames administrative-code record is invalid")]
    InvalidAdminRecord,
    #[error("GeoNames place record is invalid")]
    InvalidPlaceRecord,
    #[error("the GeoNames archive does not contain cities500.txt")]
    MissingCitiesFile,
    #[error("the GeoNames catalog is empty")]
    EmptyCatalog,
    #[error("catalog retrieval timestamp is not canonical UTC RFC 3339")]
    InvalidTimestamp,
    #[error("catalog content ID is invalid")]
    InvalidContentId,
    #[error("catalog metadata is invalid")]
    InvalidMetadata,
    #[error("unsupported catalog schema version {0}")]
    UnsupportedSchema(u32),
    #[error("location search query must not be blank")]
    EmptyQuery,
    #[error("location search query is too long")]
    QueryLimit,
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn row(id: u64, name: &str, ascii: &str, population: u64) -> String {
        [
            id.to_string(),
            name.into(),
            ascii.into(),
            String::new(),
            "40".into(),
            "-74".into(),
            "P".into(),
            "PPL".into(),
            "US".into(),
            String::new(),
            "NY".into(),
            "001".into(),
            String::new(),
            String::new(),
            population.to_string(),
            String::new(),
            "10".into(),
            "America/New_York".into(),
            "2026-01-01".into(),
        ]
        .join("\t")
    }

    fn input() -> CatalogInstallInput {
        let cities = [
            row(1, "Springfield", "Springfield", 100),
            row(2, "São José", "Sao Jose", 200),
        ]
        .join("\n")
            + "\n";
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(
                CITIES_FILE,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(cities.as_bytes()).unwrap();
        CatalogInstallInput {
            cities500_zip: archive.finish().unwrap().into_inner(),
            admin1_codes: b"US.NY\tNew York\tNew York\t1\n".to_vec(),
            admin2_codes: b"US.NY.001\tExample County\tExample County\t1\n".to_vec(),
            retrieved_at: "2026-08-19T12:00:00Z".into(),
            retrieval: CatalogRetrieval::LocalFiles,
        }
    }

    #[test]
    fn unicode_and_ascii_aliases_rank_deterministically() {
        let catalog = LocationCatalog::from_distribution(&input()).unwrap();
        assert_eq!(
            catalog.search("São José", 10).unwrap()[0].match_kind(),
            MatchKind::Exact
        );
        assert_eq!(
            catalog.search("sao jose", 10).unwrap()[0]
                .place()
                .geonames_id(),
            2
        );
        assert_eq!(
            catalog.search("spring", 10).unwrap()[0].match_kind(),
            MatchKind::Prefix
        );
    }

    #[test]
    fn queries_and_archive_shape_are_bounded() {
        let catalog = LocationCatalog::from_distribution(&input()).unwrap();
        assert!(matches!(
            catalog.search(" ", 1),
            Err(CatalogError::EmptyQuery)
        ));
        assert!(matches!(
            catalog.search(&"x".repeat(MAX_QUERY_CHARS + 1), 1),
            Err(CatalogError::QueryLimit)
        ));
        let mut bad = input();
        bad.cities500_zip = vec![0; 16];
        assert!(LocationCatalog::from_distribution(&bad).is_err());
    }

    #[test]
    fn pinned_metadata_authenticates_sources_and_replacement_changes_content_id() {
        let local = LocationCatalog::from_distribution(&input()).unwrap();
        let mut pinned_input = input();
        pinned_input.retrieval = CatalogRetrieval::SameOriginPinned {
            manifest_sha256: "a".repeat(64),
        };
        let pinned = LocationCatalog::from_distribution(&pinned_input).unwrap();
        pinned.metadata().validate().unwrap();
        assert_eq!(pinned.metadata().cities_archive.source, CITIES500_PATH);
        assert_eq!(pinned.metadata().admin1_codes.source, ADMIN1_CODES_PATH);
        assert_eq!(pinned.metadata().admin2_codes.source, ADMIN2_CODES_PATH);
        assert_eq!(pinned.metadata().attribution, ATTRIBUTION);
        assert_eq!(pinned.metadata().content_id, local.metadata().content_id);

        let mut replacement = input();
        replacement.admin2_codes =
            b"US.NY.001\tReplacement County\tReplacement County\t1\n".to_vec();
        let replacement = LocationCatalog::from_distribution(&replacement).unwrap();
        assert_ne!(
            replacement.metadata().content_id,
            local.metadata().content_id
        );
        assert!(replacement.search("does not exist", 10).unwrap().is_empty());
        assert_eq!(
            replacement.search("field", 10).unwrap()[0].match_kind(),
            MatchKind::Substring
        );
    }
}
