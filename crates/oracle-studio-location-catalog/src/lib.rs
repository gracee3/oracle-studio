//! Content-addressed, local-only GeoNames catalog parsing and search.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const CITIES500_URL: &str = "https://download.geonames.org/export/dump/cities500.zip";
pub const ADMIN1_CODES_URL: &str = "https://download.geonames.org/export/dump/admin1CodesASCII.txt";
pub const ADMIN2_CODES_URL: &str = "https://download.geonames.org/export/dump/admin2Codes.txt";
pub const DISTRIBUTION_URL: &str = "https://download.geonames.org/export/dump/";
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
const ADMIN1_FILE: &str = "admin1CodesASCII.txt";
const ADMIN2_FILE: &str = "admin2Codes.txt";
const METADATA_FILE: &str = "metadata.json";
const ACTIVE_FILE: &str = "active.json";

#[derive(Clone, Debug)]
pub struct CatalogInstallInput {
    pub cities500_zip: Vec<u8>,
    pub admin1_codes: Vec<u8>,
    pub admin2_codes: Vec<u8>,
    pub retrieved_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoNamesSourceFile {
    pub url: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMetadata {
    pub schema_version: u32,
    pub content_id: String,
    pub retrieved_at: String,
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
    fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(self.schema_version));
        }
        validate_content_id(&self.content_id)?;
        validate_sha256(&self.cities_archive.sha256)?;
        validate_sha256(&self.admin1_codes.sha256)?;
        validate_sha256(&self.admin2_codes.sha256)?;
        let timestamp = DateTime::parse_from_rfc3339(&self.retrieved_at)
            .map_err(|_| CatalogError::InvalidTimestamp)?
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        if timestamp != self.retrieved_at
            || self.place_count == 0
            || self.cities_archive.url != CITIES500_URL
            || self.admin1_codes.url != ADMIN1_CODES_URL
            || self.admin2_codes.url != ADMIN2_CODES_URL
            || self.distribution_url != DISTRIBUTION_URL
            || self.license_name != LICENSE_NAME
            || self.license_url != LICENSE_URL
            || self.attribution != ATTRIBUTION
        {
            return Err(CatalogError::InvalidMetadata);
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

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
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
        if input.cities500_zip.len() > MAX_ARCHIVE_BYTES
            || input.admin1_codes.len() > MAX_ADMIN_BYTES
            || input.admin2_codes.len() > MAX_ADMIN_BYTES
        {
            return Err(CatalogError::SizeLimit);
        }
        let cities = extract_cities500(&input.cities500_zip)?;
        Self::from_extracted(
            &cities,
            &input.admin1_codes,
            &input.admin2_codes,
            &input.cities500_zip,
            &input.retrieved_at,
        )
    }

    fn from_extracted(
        cities: &[u8],
        admin1: &[u8],
        admin2: &[u8],
        archive: &[u8],
        retrieved_at: &str,
    ) -> Result<Self, CatalogError> {
        if cities.len() > MAX_CITIES_BYTES
            || admin1.len() > MAX_ADMIN_BYTES
            || admin2.len() > MAX_ADMIN_BYTES
        {
            return Err(CatalogError::SizeLimit);
        }
        let retrieved_at = normalize_timestamp(retrieved_at)?;
        let administrative = AdministrativeNames::parse(admin1, admin2)?;
        let places = parse_places(cities, &administrative)?;
        if places.is_empty() {
            return Err(CatalogError::EmptyCatalog);
        }
        let content_id = catalog_content_id(cities, admin1, admin2);
        let metadata = CatalogMetadata {
            schema_version: CATALOG_SCHEMA_VERSION,
            content_id,
            retrieved_at,
            place_count: places.len(),
            cities_archive: GeoNamesSourceFile {
                url: CITIES500_URL.into(),
                sha256: sha256(archive),
            },
            admin1_codes: GeoNamesSourceFile {
                url: ADMIN1_CODES_URL.into(),
                sha256: sha256(admin1),
            },
            admin2_codes: GeoNamesSourceFile {
                url: ADMIN2_CODES_URL.into(),
                sha256: sha256(admin2),
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
        let limit = limit.clamp(1, MAX_RESULTS);
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
        matches.truncate(limit);
        Ok(matches)
    }
}

#[derive(Clone, Debug)]
pub struct CatalogStore {
    root: PathBuf,
}

impl CatalogStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn install(&self, input: CatalogInstallInput) -> Result<LocationCatalog, CatalogError> {
        let cities = extract_cities500(&input.cities500_zip)?;
        let catalog = LocationCatalog::from_extracted(
            &cities,
            &input.admin1_codes,
            &input.admin2_codes,
            &input.cities500_zip,
            &input.retrieved_at,
        )?;
        ensure_private_directory(&self.root)?;
        let objects = self.root.join("objects");
        ensure_private_directory(&objects)?;
        let digest = catalog
            .metadata
            .content_id
            .strip_prefix("sha256:")
            .expect("validated content ID");
        let object = objects.join(digest);
        ensure_private_directory(&object)?;
        atomic_write(&object.join(CITIES_FILE), &cities)?;
        atomic_write(&object.join(ADMIN1_FILE), &input.admin1_codes)?;
        atomic_write(&object.join(ADMIN2_FILE), &input.admin2_codes)?;
        let metadata = serde_json::to_vec(&catalog.metadata)?;
        atomic_write(&object.join(METADATA_FILE), &metadata)?;
        let active = serde_json::to_vec(&ActiveCatalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            content_id: catalog.metadata.content_id.clone(),
        })?;
        atomic_write(&self.root.join(ACTIVE_FILE), &active)?;
        Ok(catalog)
    }

    pub fn load_active(&self) -> Result<Option<LocationCatalog>, CatalogError> {
        self.validate_read_paths()?;
        let active_bytes = match read_bounded(&self.root.join(ACTIVE_FILE), 16 * 1024) {
            Ok(bytes) => bytes,
            Err(CatalogError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let active: ActiveCatalog = serde_json::from_slice(&active_bytes)?;
        if active.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(active.schema_version));
        }
        validate_content_id(&active.content_id)?;
        let digest = active
            .content_id
            .strip_prefix("sha256:")
            .expect("validated content ID");
        let object = self.root.join("objects").join(digest);
        reject_symlink(&object)?;
        let cities = read_bounded(&object.join(CITIES_FILE), MAX_CITIES_BYTES)?;
        let admin1 = read_bounded(&object.join(ADMIN1_FILE), MAX_ADMIN_BYTES)?;
        let admin2 = read_bounded(&object.join(ADMIN2_FILE), MAX_ADMIN_BYTES)?;
        let metadata_bytes = read_bounded(&object.join(METADATA_FILE), 128 * 1024)?;
        let metadata: CatalogMetadata = serde_json::from_slice(&metadata_bytes)?;
        metadata.validate()?;
        if metadata.content_id != active.content_id
            || catalog_content_id(&cities, &admin1, &admin2) != active.content_id
        {
            return Err(CatalogError::ContentMismatch);
        }
        let administrative = AdministrativeNames::parse(&admin1, &admin2)?;
        let places = parse_places(&cities, &administrative)?;
        if places.len() != metadata.place_count {
            return Err(CatalogError::ContentMismatch);
        }
        Ok(Some(LocationCatalog { metadata, places }))
    }

    pub fn active_metadata(&self) -> Result<Option<CatalogMetadata>, CatalogError> {
        self.validate_read_paths()?;
        let active_bytes = match read_bounded(&self.root.join(ACTIVE_FILE), 16 * 1024) {
            Ok(bytes) => bytes,
            Err(CatalogError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let active: ActiveCatalog = serde_json::from_slice(&active_bytes)?;
        if active.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(CatalogError::UnsupportedSchema(active.schema_version));
        }
        validate_content_id(&active.content_id)?;
        let digest = active
            .content_id
            .strip_prefix("sha256:")
            .expect("validated content ID");
        let object = self.root.join("objects").join(digest);
        reject_symlink(&object)?;
        let metadata_bytes = read_bounded(&object.join(METADATA_FILE), 128 * 1024)?;
        let metadata: CatalogMetadata = serde_json::from_slice(&metadata_bytes)?;
        metadata.validate()?;
        if metadata.content_id != active.content_id {
            return Err(CatalogError::ContentMismatch);
        }
        Ok(Some(metadata))
    }

    fn validate_read_paths(&self) -> Result<(), CatalogError> {
        reject_symlink(&self.root)?;
        reject_symlink(&self.root.join("objects"))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActiveCatalog {
    schema_version: u32,
    content_id: String,
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
        if name.is_empty() {
            return Err(CatalogError::InvalidPlaceRecord);
        }
        let latitude_degrees = parse_required::<f64>(fields[4])?;
        let longitude_degrees = parse_required::<f64>(fields[5])?;
        if !latitude_degrees.is_finite()
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
            parse_required::<u64>(fields[14])?
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
        let mut administrative_names = Vec::new();
        let admin1_key = format!("{country_code}.{}", fields[10]);
        let admin2_key = format!("{admin1_key}.{}", fields[11]);
        if let Some(name) = administrative.admin2.get(&admin2_key) {
            administrative_names.push(name.clone());
        }
        if let Some(name) = administrative.admin1.get(&admin1_key)
            && !administrative_names.contains(name)
        {
            administrative_names.push(name.clone());
        }
        let mut normalized_names = Vec::new();
        for candidate in std::iter::once(name)
            .chain(std::iter::once(fields[2]).filter(|value| !value.is_empty()))
            .chain(fields[3].split(',').filter(|value| !value.is_empty()))
        {
            let normalized = candidate.trim().to_lowercase();
            if !normalized.is_empty() && !normalized_names.contains(&normalized) {
                normalized_names.push(normalized);
            }
            if normalized_names.len() > MAX_ALIASES_PER_PLACE {
                return Err(CatalogError::AliasLimit);
            }
        }
        places.push(CatalogPlace {
            geonames_id,
            name: name.to_owned(),
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
    value
        .parse::<T>()
        .map_err(|_| CatalogError::InvalidPlaceRecord)
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
    let digest = value
        .strip_prefix("sha256:")
        .ok_or(CatalogError::InvalidContentId)?;
    validate_sha256(digest)
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

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, CatalogError> {
    reject_symlink(path)?;
    let file = File::open(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(CatalogError::SizeLimit);
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(CatalogError::SizeLimit);
    }
    Ok(bytes)
}

fn ensure_private_directory(path: &Path) -> Result<(), CatalogError> {
    reject_symlink(path)?;
    fs::create_dir_all(path)?;
    set_private_directory_permissions(path)?;
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CatalogError> {
    reject_symlink(path)?;
    let parent = path.parent().ok_or(CatalogError::InvalidPath)?;
    ensure_private_directory(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(CatalogError::InvalidPath)?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| CatalogError::Randomness(error.to_string()))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let temporary = parent.join(format!(".{name}.tmp-{suffix}"));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        set_private_open_mode(&mut options);
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        set_private_file_permissions(path)?;
        File::open(parent)?.sync_all()?;
        Ok::<(), CatalogError>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn reject_symlink(path: &Path) -> Result<(), CatalogError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CatalogError::Symlink),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn set_private_open_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_open_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), CatalogError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), CatalogError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), CatalogError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), CatalogError> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog input exceeds its configured size bound")]
    SizeLimit,
    #[error("catalog input exceeds its configured row bound")]
    RowLimit,
    #[error("catalog line exceeds its configured size bound")]
    LineLimit,
    #[error("place contains too many alternate names")]
    AliasLimit,
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
    #[error("catalog content does not match its recorded identity")]
    ContentMismatch,
    #[error("unsupported catalog schema version {0}")]
    UnsupportedSchema(u32),
    #[error("location search query must not be blank")]
    EmptyQuery,
    #[error("location search query is too long")]
    QueryLimit,
    #[error("catalog path is invalid")]
    InvalidPath,
    #[error("symbolic links are not accepted in the catalog store")]
    Symlink,
    #[error("operating-system randomness failed: {0}")]
    Randomness(String),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    const ADMIN1: &str =
        "US.NY\tNew York\tNew York\t5128638\nBR.27\tSão Paulo\tSao Paulo\t3448433\n";
    const ADMIN2: &str = "US.NY.001\tExample County\tExample County\t1\n";

    #[allow(clippy::too_many_arguments)]
    fn row(
        id: u64,
        name: &str,
        ascii: &str,
        alternates: &str,
        latitude: &str,
        longitude: &str,
        country: &str,
        admin1: &str,
        admin2: &str,
        population: u64,
        elevation: &str,
        time_zone: &str,
    ) -> String {
        [
            id.to_string(),
            name.into(),
            ascii.into(),
            alternates.into(),
            latitude.into(),
            longitude.into(),
            "P".into(),
            "PPL".into(),
            country.into(),
            String::new(),
            admin1.into(),
            admin2.into(),
            String::new(),
            String::new(),
            population.to_string(),
            elevation.into(),
            "10".into(),
            time_zone.into(),
            "2026-01-01".into(),
        ]
        .join("\t")
    }

    fn cities() -> String {
        [
            row(
                1,
                "Springfield",
                "Springfield",
                "",
                "42.0",
                "-73.0",
                "US",
                "NY",
                "001",
                1_000,
                "20",
                "America/New_York",
            ),
            row(
                2,
                "Springfield",
                "Springfield",
                "",
                "42.1",
                "-73.1",
                "US",
                "NY",
                "001",
                5_000,
                "",
                "America/New_York",
            ),
            row(
                3,
                "Springfield Heights",
                "Springfield Heights",
                "",
                "42.2",
                "-73.2",
                "US",
                "NY",
                "001",
                10_000,
                "",
                "America/New_York",
            ),
            row(
                4,
                "São José",
                "Sao Jose",
                "Sao Jose,São José dos Campos",
                "-23.2",
                "-45.8",
                "BR",
                "27",
                "",
                9_000,
                "",
                "America/Sao_Paulo",
            ),
        ]
        .join("\n")
            + "\n"
    }

    fn archive(cities: &str) -> Vec<u8> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(
                CITIES_FILE,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(cities.as_bytes()).unwrap();
        archive.finish().unwrap().into_inner()
    }

    fn install_input(cities: &str, timestamp: &str) -> CatalogInstallInput {
        CatalogInstallInput {
            cities500_zip: archive(cities),
            admin1_codes: ADMIN1.as_bytes().to_vec(),
            admin2_codes: ADMIN2.as_bytes().to_vec(),
            retrieved_at: timestamp.into(),
        }
    }

    #[test]
    fn exact_prefix_and_substring_ranking_is_deterministic_and_unicode_aware() {
        let input = install_input(&cities(), "2026-08-18T12:00:00Z");
        let catalog = LocationCatalog::from_distribution(&input).unwrap();
        let matches = catalog.search("springfield", 10).unwrap();
        assert_eq!(
            matches
                .iter()
                .map(|result| (result.place().geonames_id(), result.match_kind()))
                .collect::<Vec<_>>(),
            vec![
                (2, MatchKind::Exact),
                (1, MatchKind::Exact),
                (3, MatchKind::Prefix),
            ]
        );
        assert_eq!(
            catalog.search("São José", 10).unwrap()[0]
                .place()
                .geonames_id(),
            4
        );
        assert_eq!(
            catalog.search("sao jose", 10).unwrap()[0].match_kind(),
            MatchKind::Exact
        );
        assert_eq!(
            catalog.search("field heights", 10).unwrap()[0].match_kind(),
            MatchKind::Substring
        );
        assert_eq!(
            matches[0].place().administrative_names(),
            ["Example County", "New York"]
        );
    }

    #[test]
    fn malformed_records_bounds_and_queries_fail_closed() {
        let administrative =
            AdministrativeNames::parse(ADMIN1.as_bytes(), ADMIN2.as_bytes()).unwrap();
        let invalid = row(
            1,
            "Invalid",
            "Invalid",
            "",
            "91.0",
            "0.0",
            "US",
            "NY",
            "001",
            1,
            "",
            "America/New_York",
        );
        assert!(matches!(
            parse_places(invalid.as_bytes(), &administrative),
            Err(CatalogError::InvalidPlaceRecord)
        ));
        let long_line = "x".repeat(MAX_LINE_BYTES + 1);
        assert!(matches!(
            parse_places(long_line.as_bytes(), &administrative),
            Err(CatalogError::LineLimit)
        ));
        let catalog =
            LocationCatalog::from_distribution(&install_input(&cities(), "2026-08-18T12:00:00Z"))
                .unwrap();
        assert!(matches!(
            catalog.search("   ", 10),
            Err(CatalogError::EmptyQuery)
        ));
        assert!(matches!(
            catalog.search(&"a".repeat(MAX_QUERY_CHARS + 1), 10),
            Err(CatalogError::QueryLimit)
        ));
        assert!(matches!(
            LocationCatalog::from_distribution(&CatalogInstallInput {
                cities500_zip: archive("not\ta\tvalid\n"),
                admin1_codes: vec![],
                admin2_codes: vec![],
                retrieved_at: "not-a-time".into(),
            }),
            Err(CatalogError::InvalidTimestamp | CatalogError::InvalidPlaceRecord)
        ));
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random).unwrap();
            let name = random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            Self(std::env::temp_dir().join(format!("oracle-catalog-{name}")))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if self.0.exists() {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }

    #[test]
    fn store_replacement_keeps_content_addressing_attribution_and_prior_snapshots() {
        let directory = TestDirectory::new();
        let store = CatalogStore::new(&directory.0);
        let first = store
            .install(install_input(&cities(), "2026-08-18T12:00:00Z"))
            .unwrap();
        let first_snapshot = first.search("São José", 1).unwrap()[0].place().clone();
        let replacement_cities = cities().replace("Springfield Heights", "Replacement City");
        let second = store
            .install(install_input(&replacement_cities, "2026-08-18T13:00:00Z"))
            .unwrap();
        assert_ne!(first.metadata().content_id, second.metadata().content_id);
        assert_eq!(second.metadata().attribution, ATTRIBUTION);
        assert_eq!(second.metadata().license_name, LICENSE_NAME);
        assert_eq!(first_snapshot.name(), "São José");
        let reopened = store.load_active().unwrap().unwrap();
        assert_eq!(reopened.metadata(), second.metadata());
        assert_eq!(
            reopened.search("replacement", 1).unwrap()[0]
                .place()
                .geonames_id(),
            3
        );
        let old_digest = first.metadata().content_id.strip_prefix("sha256:").unwrap();
        assert!(directory.0.join("objects").join(old_digest).is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn store_reads_reject_symlinked_content_objects() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let store = CatalogStore::new(&directory.0);
        let installed = store
            .install(install_input(&cities(), "2026-08-18T12:00:00Z"))
            .unwrap();
        let digest = installed
            .metadata()
            .content_id
            .strip_prefix("sha256:")
            .unwrap();
        let object = directory.0.join("objects").join(digest);
        let moved_object = directory.0.join("moved-object");
        fs::rename(&object, &moved_object).unwrap();
        symlink(&moved_object, &object).unwrap();

        assert!(matches!(store.load_active(), Err(CatalogError::Symlink)));
        assert!(matches!(
            store.active_metadata(),
            Err(CatalogError::Symlink)
        ));
    }
}
