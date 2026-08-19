//! Native loopback host and in-memory unlocked-vault session.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{SecondsFormat, Utc};
use oracle_studio_app::{ChartCalculationRequest, ComparisonCalculationRequest, StudioService};
use oracle_studio_chart_view::{ChartPoint, ChartScene};
use oracle_studio_core::{
    AmbiguousTimeChoice, ArtifactKind, AspectDefinition, AspectKindId, AyanamsaId,
    CelestialObjectId, ChartCalculationOptions, ChartDefinition, ChartPointId, ChartRole,
    ComparisonPreset, HouseSystemId, LocalDateTimeInput, LocalTimeResolution, LocationProvenance,
    PersonKind, PersonProfile, SavedLocation, StableId, VaultDocument, WheelOrientation,
    WorkspaceState, ZodiacId, resolve_local_time,
};
use oracle_studio_location_catalog::{
    ADMIN1_CODES_URL, ADMIN2_CODES_URL, ATTRIBUTION, CITIES500_URL, CatalogInstallInput,
    CatalogMetadata, CatalogStore, DISTRIBUTION_URL, LICENSE_NAME, LICENSE_URL, LocationCatalog,
    MAX_ARCHIVE_BYTES, MatchKind,
};
use oracle_studio_protocol::{
    AmbiguousTimeChoiceInput, ApiError, ApiErrorCode, ApiResponse, AspectKindInput, AyanamsaInput,
    BiwheelAspect, BiwheelPoint, BiwheelRing, BiwheelScene, CalculateChartRequest,
    CalculateComparisonRequest, CatalogMatchKind, CatalogPlaceSummary, CatalogStatus,
    CelestialObjectInput, ChartCalculationSummary, ChartInformation, ChartPointInput,
    ChartRoleInput, ChartSummary, ComparisonSummary, CreateVaultRequest, HouseSystemInput,
    InstallCatalogRequest, LocalTimeResolutionSummary, LocationProvenanceInput, LocationSummary,
    MutationResult, PROTOCOL_VERSION, PersonKindInput, PersonSummary, ProtocolRequest,
    ResolveChartTimeRequest, ResolvedTimeSummary, SaveChartRequest, SaveComparisonRequest,
    SaveLocationRequest, SavePersonRequest, SearchCatalogRequest, SessionStatus,
    SetWorkspaceRequest, UnlockVaultRequest, VaultState, WheelOrientationInput,
    WorkspacePresentation, WorkspaceSummary, ZodiacInput,
};
use oracle_studio_storage::{ExpectedState, FileVault, StorageError, VaultRevision};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::Mutex};
use tower_http::services::{ServeDir, ServeFile};
use zeroize::Zeroizing;

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const CSP: &str = "default-src 'self'; connect-src 'self'; font-src 'self' data:; img-src 'self' data:; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
const CSP_SCRIPT_PREFIX: &str = "default-src 'self'; connect-src 'self'; font-src 'self' data:; img-src 'self' data:; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'";
const CSP_SCRIPT_SUFFIX: &str =
    "; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

#[derive(Clone)]
struct SecurityHeaders {
    content_security_policy: HeaderValue,
}

impl SecurityHeaders {
    fn for_distribution(distribution: &Path) -> Self {
        let content_security_policy = std::fs::read_to_string(distribution.join("index.html"))
            .ok()
            .map(|index| content_security_policy(&index))
            .unwrap_or_else(|| HeaderValue::from_static(CSP));
        Self {
            content_security_policy,
        }
    }
}

#[derive(Clone)]
pub struct AppState(Arc<AppStateInner>);

struct AppStateInner {
    expected_origin: String,
    expected_host: String,
    bearer_token: Zeroizing<String>,
    session: Mutex<SessionStore>,
    catalog: Mutex<CatalogRuntime>,
}

impl AppState {
    pub fn new(
        expected_origin: impl Into<String>,
        bearer_token: impl Into<String>,
        idle_timeout: Duration,
    ) -> Result<Self, HostError> {
        Self::with_catalog_root(
            expected_origin,
            bearer_token,
            idle_timeout,
            default_catalog_root()?,
        )
    }

    pub fn with_catalog_root(
        expected_origin: impl Into<String>,
        bearer_token: impl Into<String>,
        idle_timeout: Duration,
        catalog_root: impl Into<PathBuf>,
    ) -> Result<Self, HostError> {
        let expected_origin = expected_origin.into();
        let expected_host = expected_origin
            .strip_prefix("http://")
            .ok_or(HostError::InvalidOrigin)?
            .to_owned();
        let port = expected_host
            .strip_prefix("127.0.0.1:")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port != 0)
            .ok_or(HostError::InvalidOrigin)?;
        if expected_origin != format!("http://127.0.0.1:{port}") {
            return Err(HostError::InvalidOrigin);
        }
        Ok(Self(Arc::new(AppStateInner {
            expected_origin,
            expected_host,
            bearer_token: Zeroizing::new(bearer_token.into()),
            session: Mutex::new(SessionStore::new(idle_timeout)),
            catalog: Mutex::new(CatalogRuntime {
                store: CatalogStore::new(catalog_root),
                loaded: None,
            }),
        })))
    }
}

struct CatalogRuntime {
    store: CatalogStore,
    loaded: Option<Arc<LocationCatalog>>,
}

struct VaultSession {
    vault: FileVault,
    document: VaultDocument,
    revision: VaultRevision,
    password: Zeroizing<Vec<u8>>,
    last_activity: Instant,
}

struct SessionStore {
    current: Option<VaultSession>,
    idle_timeout: Duration,
}

impl SessionStore {
    const fn new(idle_timeout: Duration) -> Self {
        Self {
            current: None,
            idle_timeout,
        }
    }

    fn replace(
        &mut self,
        vault: FileVault,
        document: VaultDocument,
        revision: VaultRevision,
        password: Zeroizing<Vec<u8>>,
        now: Instant,
    ) {
        self.current = Some(VaultSession {
            vault,
            document,
            revision,
            password,
            last_activity: now,
        });
    }

    fn expire_and_touch(&mut self, now: Instant) {
        if self.current.as_ref().is_some_and(|session| {
            now.saturating_duration_since(session.last_activity) >= self.idle_timeout
        }) {
            self.current = None;
        } else if let Some(session) = &mut self.current {
            session.last_activity = now;
        }
    }

    fn status(&self) -> SessionStatus {
        match &self.current {
            Some(session) => SessionStatus {
                state: VaultState::Unlocked,
                vault_name: session
                    .vault
                    .path()
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned()),
                revision: Some(session.revision.as_str().to_owned()),
                idle_timeout_seconds: self.idle_timeout.as_secs(),
            },
            None => SessionStatus {
                state: VaultState::Locked,
                vault_name: None,
                revision: None,
                idle_timeout_seconds: self.idle_timeout.as_secs(),
            },
        }
    }
}

pub fn app(state: AppState, distribution: impl AsRef<Path>) -> Router {
    let distribution = distribution.as_ref().to_owned();
    let security_headers = SecurityHeaders::for_distribution(&distribution);
    let api = Router::new()
        .route("/session/status", post(session_status))
        .route("/vault/create", post(create_vault))
        .route("/vault/unlock", post(unlock_vault))
        .route("/vault/lock", post(lock_vault))
        .route("/people/list", post(list_people))
        .route("/people/save", post(save_person))
        .route("/locations/list", post(list_locations))
        .route("/locations/save", post(save_location))
        .route("/catalog/status", post(catalog_status))
        .route("/catalog/install", post(install_catalog))
        .route("/catalog/search", post(search_catalog))
        .route("/charts/list", post(list_charts))
        .route("/charts/save", post(save_chart))
        .route("/charts/time-resolution", post(resolve_chart_time))
        .route("/charts/calculate", post(calculate_chart))
        .route("/comparisons/list", post(list_comparisons))
        .route("/comparisons/save", post(save_comparison))
        .route("/comparisons/calculate", post(calculate_comparison))
        .route("/workspace/get", post(get_workspace))
        .route("/workspace/view", post(get_workspace_presentation))
        .route("/workspace/set", post(set_workspace))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize_api))
        .with_state(state);
    let index = distribution.join("index.html");
    Router::new()
        .nest("/api/v1", api)
        .fallback_service(ServeDir::new(distribution).not_found_service(ServeFile::new(index)))
        .layer(middleware::from_fn_with_state(
            security_headers,
            apply_security_headers,
        ))
}

fn content_security_policy(index: &str) -> HeaderValue {
    let hashes = inline_script_hash_sources(index);
    if hashes.is_empty() {
        return HeaderValue::from_static(CSP);
    }
    HeaderValue::from_str(&format!(
        "{CSP_SCRIPT_PREFIX} {}{CSP_SCRIPT_SUFFIX}",
        hashes.join(" ")
    ))
    .expect("SHA-256 CSP sources contain only valid header bytes")
}

fn inline_script_hash_sources(index: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let mut remaining = index;
    while let Some(script_start) = remaining.find("<script") {
        let after_name = &remaining[script_start + "<script".len()..];
        let Some(tag_end) = after_name.find('>') else {
            break;
        };
        let opening_tag = &after_name[..tag_end];
        let content_and_rest = &after_name[tag_end + 1..];
        let Some(script_end) = content_and_rest.find("</script>") else {
            break;
        };
        if !opening_tag
            .split_ascii_whitespace()
            .any(|attribute| attribute.starts_with("src="))
        {
            let digest = Sha256::digest(&content_and_rest.as_bytes()[..script_end]);
            sources.push(format!("'sha256-{}'", BASE64_STANDARD.encode(digest)));
        }
        remaining = &content_and_rest[script_end + "</script>".len()..];
    }
    sources
}

pub async fn bind_loopback(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)).await
}

fn default_catalog_root() -> Result<PathBuf, HostError> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path.join("oracle-studio").join("geonames"));
        }
    }
    if let Some(path) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path
                .join(".local")
                .join("share")
                .join("oracle-studio")
                .join("geonames"));
        }
    }
    Err(HostError::CatalogRootUnavailable)
}

pub fn validate_loopback(address: SocketAddr) -> Result<(), HostError> {
    if address.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST) {
        Ok(())
    } else {
        Err(HostError::NonLoopbackBind(address))
    }
}

pub fn launch_token() -> Result<Zeroizing<String>, HostError> {
    let mut bytes = Zeroizing::new([0_u8; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|error| HostError::Randomness(error.to_string()))?;
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(Zeroizing::new(token))
}

async fn authorize_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let valid_host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.0.expected_host);
    let valid_origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.0.expected_origin);
    let valid_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| {
            value.len() == state.0.bearer_token.len()
                && bool::from(value.as_bytes().ct_eq(state.0.bearer_token.as_bytes()))
        });
    if valid_host && valid_origin && valid_token {
        next.run(request).await
    } else {
        api_error(
            StatusCode::UNAUTHORIZED,
            ApiErrorCode::Unauthorized,
            "the local Studio session could not authorize this request",
        )
    }
}

async fn apply_security_headers(
    State(security): State<SecurityHeaders>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        security.content_security_policy,
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    response
}

async fn session_status(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    session.expire_and_touch(Instant::now());
    Json(ApiResponse::current(session.status())).into_response()
}

async fn create_vault(
    State(state): State<AppState>,
    Json(request): Json<CreateVaultRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let (path, password) = request.into_parts();
    let result = tokio::task::spawn_blocking(move || {
        let password = Zeroizing::new(password.into_bytes());
        let vault = FileVault::new(PathBuf::from(path))?;
        let document = VaultDocument::empty();
        let revision = vault.save(&document, &password, &ExpectedState::Missing)?;
        Ok::<_, StorageError>((vault, document, revision, password))
    })
    .await;
    let (vault, document, revision, password) = match result {
        Ok(Ok(created)) => created,
        Ok(Err(error)) => return storage_error(error),
        Err(_) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Unavailable,
                "the vault worker stopped unexpectedly",
            );
        }
    };
    let mut session = state.0.session.lock().await;
    session.replace(vault, document, revision, password, Instant::now());
    Json(ApiResponse::current(session.status())).into_response()
}

async fn unlock_vault(
    State(state): State<AppState>,
    Json(request): Json<UnlockVaultRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let (path, password) = request.into_parts();
    let result = tokio::task::spawn_blocking(move || {
        let password = Zeroizing::new(password.into_bytes());
        let vault = FileVault::new(PathBuf::from(path))?;
        let loaded = vault.load(&password)?;
        let revision = loaded.revision().clone();
        Ok::<_, StorageError>((vault, loaded.into_document(), revision, password))
    })
    .await;
    let (vault, document, revision, password) = match result {
        Ok(Ok(unlocked)) => unlocked,
        Ok(Err(error)) => return storage_error(error),
        Err(_) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Unavailable,
                "the vault worker stopped unexpectedly",
            );
        }
    };
    let mut session = state.0.session.lock().await;
    session.replace(vault, document, revision, password, Instant::now());
    Json(ApiResponse::current(session.status())).into_response()
}

async fn lock_vault(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    session.current = None;
    Json(ApiResponse::current(session.status())).into_response()
}

async fn list_people(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let people = current
        .document
        .people()
        .iter()
        .map(|person| PersonSummary {
            id: person.id().as_str().to_owned(),
            display_name: person.display_name().to_owned(),
            kind: match person.kind() {
                PersonKind::Personal => "personal",
                PersonKind::ProfessionalClient => "professional_client",
            }
            .to_owned(),
            notes: person.notes().map(str::to_owned),
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::current(people)).into_response()
}

async fn list_locations(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let locations = current
        .document
        .saved_locations()
        .iter()
        .map(|location| LocationSummary {
            id: location.id().as_str().to_owned(),
            label: location.label().to_owned(),
            administrative_names: location.administrative_names().to_vec(),
            country_code: location.country_code().to_owned(),
            time_zone: location.time_zone().to_owned(),
            latitude_degrees: location.latitude_degrees(),
            longitude_degrees: location.longitude_degrees(),
            elevation_meters: location.elevation_meters(),
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::current(locations)).into_response()
}

async fn catalog_status(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let catalog = state.0.catalog.lock().await;
    let metadata = match &catalog.loaded {
        Some(loaded) => Some(loaded.metadata().clone()),
        None => match catalog.store.active_metadata() {
            Ok(metadata) => metadata,
            Err(error) => return catalog_error(error),
        },
    };
    Json(ApiResponse::current(catalog_status_summary(
        metadata.as_ref(),
    )))
    .into_response()
}

async fn install_catalog(
    State(state): State<AppState>,
    Json(request): Json<InstallCatalogRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let store = state.0.catalog.lock().await.store.clone();
    let installed = tokio::task::spawn_blocking(move || {
        let input = download_geonames_catalog()?;
        store
            .install(input)
            .map_err(|error| CatalogOperationError::Catalog(error.to_string()))
    })
    .await;
    let catalog = match installed {
        Ok(Ok(catalog)) => Arc::new(catalog),
        Ok(Err(error)) => return catalog_error(error),
        Err(_) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Unavailable,
                "the catalog installer stopped unexpectedly",
            );
        }
    };
    let status = catalog_status_summary(Some(catalog.metadata()));
    state.0.catalog.lock().await.loaded = Some(catalog);
    Json(ApiResponse::current(status)).into_response()
}

async fn search_catalog(
    State(state): State<AppState>,
    Json(request): Json<SearchCatalogRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let loaded = state.0.catalog.lock().await.loaded.clone();
    let catalog = match loaded {
        Some(catalog) => catalog,
        None => {
            let store = state.0.catalog.lock().await.store.clone();
            let loaded = tokio::task::spawn_blocking(move || store.load_active()).await;
            let catalog = match loaded {
                Ok(Ok(Some(catalog))) => Arc::new(catalog),
                Ok(Ok(None)) => {
                    return api_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        ApiErrorCode::Unavailable,
                        "install the offline GeoNames catalog before searching",
                    );
                }
                Ok(Err(error)) => return catalog_error(error),
                Err(_) => {
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ApiErrorCode::Unavailable,
                        "the catalog loader stopped unexpectedly",
                    );
                }
            };
            let mut runtime = state.0.catalog.lock().await;
            let catalog = runtime.loaded.get_or_insert(catalog);
            Arc::clone(catalog)
        }
    };
    let matches = match catalog.search(&request.query, request.limit) {
        Ok(matches) => matches,
        Err(error) => return app_error(error),
    };
    let content_id = catalog.metadata().content_id.clone();
    let results = matches
        .into_iter()
        .map(|result| {
            let place = result.place();
            CatalogPlaceSummary {
                geonames_id: place.geonames_id(),
                name: place.name().to_owned(),
                administrative_names: place.administrative_names().to_vec(),
                country_code: place.country_code().to_owned(),
                latitude_degrees: place.latitude_degrees(),
                longitude_degrees: place.longitude_degrees(),
                elevation_meters: place.elevation_meters(),
                time_zone: place.time_zone().to_owned(),
                population: place.population(),
                match_kind: match result.match_kind() {
                    MatchKind::Exact => CatalogMatchKind::Exact,
                    MatchKind::Prefix => CatalogMatchKind::Prefix,
                    MatchKind::Substring => CatalogMatchKind::Substring,
                },
                catalog_content_id: content_id.clone(),
            }
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::current(results)).into_response()
}

async fn list_charts(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let charts = current
        .document
        .chart_definitions()
        .iter()
        .map(|chart| chart_summary(&current.document, chart))
        .collect::<Vec<_>>();
    Json(ApiResponse::current(charts)).into_response()
}

async fn list_comparisons(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let comparisons = current
        .document
        .comparison_presets()
        .iter()
        .map(|comparison| ComparisonSummary {
            id: comparison.id().as_str().to_owned(),
            label: comparison.label().to_owned(),
            inner_chart_id: comparison.inner_chart_definition_id().as_str().to_owned(),
            outer_chart_id: comparison.outer_chart_definition_id().as_str().to_owned(),
            inner_points: comparison
                .inner_points()
                .iter()
                .copied()
                .map(protocol_point)
                .collect(),
            outer_points: comparison
                .outer_points()
                .iter()
                .copied()
                .map(protocol_point)
                .collect(),
            aspects: comparison
                .aspects()
                .iter()
                .map(|aspect| oracle_studio_protocol::AspectDefinitionInput {
                    kind: protocol_aspect(aspect.kind()),
                    orb_degrees: aspect.orb_degrees(),
                })
                .collect(),
            orientation: protocol_orientation(comparison.orientation()),
            current_comparison_artifact_id: comparison
                .current_comparison_artifact_id()
                .map(|id| id.as_str().to_owned()),
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::current(comparisons)).into_response()
}

async fn get_workspace(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let workspace = current.document.workspace_state();
    Json(ApiResponse::current(WorkspaceSummary {
        active_person_id: workspace
            .active_person_id()
            .map(|id| id.as_str().to_owned()),
        active_comparison_id: workspace
            .active_comparison_preset_id()
            .map(|id| id.as_str().to_owned()),
    }))
    .into_response()
}

async fn get_workspace_presentation(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    match workspace_presentation(&current.document) {
        Ok(presentation) => Json(ApiResponse::current(presentation)).into_response(),
        Err(error) => app_error(error),
    }
}

async fn save_person(
    State(state): State<AppState>,
    Json(request): Json<SavePersonRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let id = match StableId::new("person.id", request.id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let person = match PersonProfile::new(
        id.clone(),
        request.display_name,
        match request.kind {
            PersonKindInput::Personal => PersonKind::Personal,
            PersonKindInput::ProfessionalClient => PersonKind::ProfessionalClient,
        },
        request.notes,
    ) {
        Ok(person) => person,
        Err(error) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = if current
        .document
        .people()
        .iter()
        .any(|item| item.id() == &id)
    {
        StudioService::replace_person(&current.document, person)
    } else {
        StudioService::add_person(&current.document, person)
    };
    persist_result(current, next, id.as_str())
}

async fn save_location(
    State(state): State<AppState>,
    Json(request): Json<SaveLocationRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let id = match StableId::new("saved_location.id", request.id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let provenance = match request.provenance {
        LocationProvenanceInput::Manual => LocationProvenance::Manual,
        LocationProvenanceInput::GeoNames {
            geonames_id,
            catalog_content_id,
        } => LocationProvenance::GeoNames {
            geonames_id,
            catalog_content_id,
        },
    };
    let location = match SavedLocation::new(
        id.clone(),
        request.label,
        request.administrative_names,
        request.country_code,
        request.latitude_degrees,
        request.longitude_degrees,
        request.elevation_meters,
        request.time_zone,
        provenance,
    ) {
        Ok(location) => location,
        Err(error) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = if current
        .document
        .saved_locations()
        .iter()
        .any(|item| item.id() == &id)
    {
        StudioService::replace_saved_location(&current.document, location)
    } else {
        StudioService::add_saved_location(&current.document, location)
    };
    persist_result(current, next, id.as_str())
}

async fn save_chart(
    State(state): State<AppState>,
    Json(request): Json<SaveChartRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let id = match StableId::new("chart_definition.id", request.id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let person_id = match optional_id("chart_definition.person_id", request.person_id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let local_input =
        match LocalDateTimeInput::new(request.local_date, request.local_time, request.time_zone) {
            Ok(input) => input,
            Err(error) => return app_error(error),
        };
    let options = match ChartCalculationOptions::new(
        match request.zodiac {
            ZodiacInput::Tropical => ZodiacId::Tropical,
            ZodiacInput::Sidereal => ZodiacId::Sidereal,
        },
        request.ayanamsa.map(map_ayanamsa),
        match request.house_system {
            HouseSystemInput::Placidus => HouseSystemId::Placidus,
            HouseSystemInput::Koch => HouseSystemId::Koch,
            HouseSystemInput::Porphyry => HouseSystemId::Porphyry,
            HouseSystemInput::Regiomontanus => HouseSystemId::Regiomontanus,
            HouseSystemInput::Campanus => HouseSystemId::Campanus,
            HouseSystemInput::Equal => HouseSystemId::Equal,
            HouseSystemInput::WholeSign => HouseSystemId::WholeSign,
        },
        request
            .ordered_objects
            .into_iter()
            .map(map_object)
            .collect(),
    ) {
        Ok(options) => options,
        Err(error) => return app_error(error),
    };
    let chart = match ChartDefinition::new(
        id.clone(),
        request.label,
        match request.role {
            ChartRoleInput::Natal => ChartRole::Natal,
            ChartRoleInput::Event => ChartRole::Event,
            ChartRoleInput::Transit => ChartRole::Transit,
        },
        person_id,
        local_input,
        options,
        request.ordered_points.into_iter().map(map_point).collect(),
        request.default_natal,
    ) {
        Ok(chart) => chart,
        Err(error) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = if current
        .document
        .chart_definitions()
        .iter()
        .any(|item| item.id() == &id)
    {
        StudioService::replace_chart_definition(&current.document, chart)
    } else {
        StudioService::add_chart_definition(&current.document, chart)
    };
    persist_result(current, next, id.as_str())
}

async fn calculate_chart(
    State(state): State<AppState>,
    Json(request): Json<CalculateChartRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let calculation_id =
        match StableId::new("chart_calculation.id", request.chart_calculation_id.clone()) {
            Ok(id) => id,
            Err(error) => return app_error(error),
        };
    let service_request = match chart_calculation_request(request, calculation_id.clone()) {
        Ok(request) => request,
        Err(error) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = StudioService::calculate_chart_definition(&current.document, service_request);
    persist_result(current, next, calculation_id.as_str())
}

async fn resolve_chart_time(
    State(state): State<AppState>,
    Json(request): Json<ResolveChartTimeRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let chart_id = match StableId::new("chart_definition.id", request.chart_definition_id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let chart = match current
        .document
        .chart_definitions()
        .iter()
        .find(|chart| chart.id() == &chart_id)
    {
        Some(chart) => chart,
        None => {
            return api_error(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "the chart definition does not exist",
            );
        }
    };
    let resolution = match resolve_local_time(chart.local_input()) {
        Ok(LocalTimeResolution::Unique(value)) => LocalTimeResolutionSummary::Unique {
            value: resolved_time_summary(&value),
        },
        Ok(LocalTimeResolution::Ambiguous { earlier, later }) => {
            LocalTimeResolutionSummary::Ambiguous {
                earlier: resolved_time_summary(&earlier),
                later: resolved_time_summary(&later),
            }
        }
        Ok(LocalTimeResolution::Nonexistent) => LocalTimeResolutionSummary::Nonexistent,
        Err(error) => return app_error(error),
    };
    Json(ApiResponse::current(resolution)).into_response()
}

async fn save_comparison(
    State(state): State<AppState>,
    Json(request): Json<SaveComparisonRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let id = match StableId::new("comparison_preset.id", request.id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let preset = ComparisonPreset::new(
        id.clone(),
        request.label,
        match StableId::new(
            "comparison_preset.inner_chart_definition_id",
            request.inner_chart_definition_id,
        ) {
            Ok(id) => id,
            Err(error) => return app_error(error),
        },
        match StableId::new(
            "comparison_preset.outer_chart_definition_id",
            request.outer_chart_definition_id,
        ) {
            Ok(id) => id,
            Err(error) => return app_error(error),
        },
        request.inner_points.into_iter().map(map_point).collect(),
        request.outer_points.into_iter().map(map_point).collect(),
        match request
            .aspects
            .into_iter()
            .map(|aspect| {
                AspectDefinition::new(
                    match aspect.kind {
                        AspectKindInput::Conjunction => AspectKindId::Conjunction,
                        AspectKindInput::Opposition => AspectKindId::Opposition,
                        AspectKindInput::Square => AspectKindId::Square,
                        AspectKindInput::Trine => AspectKindId::Trine,
                        AspectKindInput::Sextile => AspectKindId::Sextile,
                    },
                    aspect.orb_degrees,
                )
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(aspects) => aspects,
            Err(error) => return app_error(error),
        },
        match request.orientation {
            WheelOrientationInput::AscendantLeft => WheelOrientation::AscendantLeft,
            WheelOrientationInput::AriesTop => WheelOrientation::AriesTop,
        },
    );
    let preset = match preset {
        Ok(preset) => preset,
        Err(error) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = if current
        .document
        .comparison_presets()
        .iter()
        .any(|item| item.id() == &id)
    {
        StudioService::replace_comparison_preset(&current.document, preset)
    } else {
        StudioService::add_comparison_preset(&current.document, preset)
    };
    persist_result(current, next, id.as_str())
}

async fn calculate_comparison(
    State(state): State<AppState>,
    Json(request): Json<CalculateComparisonRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let artifact_id = match StableId::new("artifact.id", request.comparison_artifact_id) {
        Ok(id) => id,
        Err(error) => return app_error(error),
    };
    let service_request = ComparisonCalculationRequest {
        comparison_artifact_id: artifact_id.clone(),
        comparison_preset_id: match StableId::new(
            "comparison_preset.id",
            request.comparison_preset_id,
        ) {
            Ok(id) => id,
            Err(error) => return app_error(error),
        },
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = StudioService::calculate_comparison(&current.document, service_request);
    persist_result(current, next, artifact_id.as_str())
}

async fn set_workspace(
    State(state): State<AppState>,
    Json(request): Json<SetWorkspaceRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let workspace = match (
        optional_id("workspace.active_person_id", request.active_person_id),
        optional_id(
            "workspace.active_comparison_preset_id",
            request.active_comparison_id,
        ),
    ) {
        (Ok(person), Ok(comparison)) => WorkspaceState::new(person, comparison),
        (Err(error), _) | (_, Err(error)) => return app_error(error),
    };
    let mut session = state.0.session.lock().await;
    let current = match active_session_mut(&mut session) {
        Ok(current) => current,
        Err(response) => return response.into_response(),
    };
    let next = StudioService::set_workspace_state(&current.document, workspace);
    persist_result(current, next, "workspace")
}

fn chart_summary(document: &VaultDocument, chart: &ChartDefinition) -> ChartSummary {
    ChartSummary {
        id: chart.id().as_str().to_owned(),
        label: chart.label().to_owned(),
        role: protocol_role(chart.role()),
        person_id: chart.person_id().map(|id| id.as_str().to_owned()),
        local_date: chart.local_input().local_date().to_owned(),
        local_time: chart.local_input().local_time().to_owned(),
        time_zone: chart.local_input().time_zone().to_owned(),
        zodiac: protocol_zodiac(chart.calculation_options().zodiac()),
        ayanamsa: chart
            .calculation_options()
            .ayanamsa()
            .map(protocol_ayanamsa),
        house_system: protocol_house_system(chart.calculation_options().house_system()),
        ordered_objects: chart
            .calculation_options()
            .ordered_objects()
            .iter()
            .copied()
            .map(protocol_object)
            .collect(),
        ordered_points: chart
            .ordered_points()
            .iter()
            .copied()
            .map(protocol_point)
            .collect(),
        default_natal: chart.default_natal(),
        current_calculation_id: chart
            .current_calculation_id()
            .map(|id| id.as_str().to_owned()),
        calculation_history: document
            .chart_calculations()
            .iter()
            .filter(|calculation| calculation.chart_definition_id() == chart.id())
            .map(|calculation| ChartCalculationSummary {
                id: calculation.id().as_str().to_owned(),
                abbreviation: calculation.resolved_time().abbreviation().to_owned(),
                utc_offset_display: calculation.resolved_time().utc_offset_display(),
                utc_instant: calculation.resolved_time().utc_instant().to_owned(),
                location_label: calculation.location_snapshot().label().to_owned(),
                calculated_at: calculation.calculated_at().to_owned(),
            })
            .collect(),
    }
}

fn workspace_presentation(
    document: &VaultDocument,
) -> Result<Option<WorkspacePresentation>, String> {
    let Some(comparison_id) = document.workspace_state().active_comparison_preset_id() else {
        return Ok(None);
    };
    let preset = document
        .comparison_presets()
        .iter()
        .find(|preset| preset.id() == comparison_id)
        .ok_or_else(|| "the active comparison preset is missing".to_owned())?;
    let Some(artifact_id) = preset.current_comparison_artifact_id() else {
        return Ok(None);
    };
    let artifact = document
        .artifacts()
        .iter()
        .find(|artifact| artifact.id() == artifact_id)
        .ok_or_else(|| "the active comparison artifact is missing".to_owned())?;
    if artifact.kind() != ArtifactKind::AstraeusComparison {
        return Err("the active workspace artifact is not an Astraeus comparison".to_owned());
    }
    let scene = ChartScene::from_comparison_json(artifact.canonical_json())
        .map_err(|error| error.to_string())?;
    let inner_calculation_id = preset
        .current_inner_calculation_id()
        .ok_or_else(|| "the active comparison has no inner calculation".to_owned())?;
    let outer_calculation_id = preset
        .current_outer_calculation_id()
        .ok_or_else(|| "the active comparison has no outer calculation".to_owned())?;
    let inner = chart_information(
        document,
        preset.inner_chart_definition_id(),
        inner_calculation_id,
        &scene.natal.zodiac,
        &scene.natal.house_system,
    )?;
    let outer = chart_information(
        document,
        preset.outer_chart_definition_id(),
        outer_calculation_id,
        &scene.transit_zodiac,
        &scene.transit_house_system,
    )?;
    Ok(Some(WorkspacePresentation {
        comparison_id: preset.id().as_str().to_owned(),
        comparison_label: preset.label().to_owned(),
        inner,
        outer,
        orientation: protocol_orientation(preset.orientation()),
        scene: protocol_scene(scene),
    }))
}

fn chart_information(
    document: &VaultDocument,
    chart_id: &StableId,
    calculation_id: &StableId,
    zodiac: &str,
    house_system: &str,
) -> Result<ChartInformation, String> {
    let chart = document
        .chart_definitions()
        .iter()
        .find(|chart| chart.id() == chart_id)
        .ok_or_else(|| "a workspace chart definition is missing".to_owned())?;
    let calculation = document
        .chart_calculations()
        .iter()
        .find(|calculation| calculation.id() == calculation_id)
        .ok_or_else(|| "a workspace chart calculation is missing".to_owned())?;
    let person_label = chart.person_id().and_then(|person_id| {
        document
            .people()
            .iter()
            .find(|person| person.id() == person_id)
            .map(|person| person.display_name().to_owned())
    });
    let local = calculation.local_input_snapshot();
    let resolved = calculation.resolved_time();
    let location = calculation.location_snapshot();
    Ok(ChartInformation {
        chart_label: chart.label().to_owned(),
        person_label,
        role: protocol_role(chart.role()),
        local_date: local.local_date().to_owned(),
        local_time: local.local_time().to_owned(),
        abbreviation: resolved.abbreviation().to_owned(),
        utc_offset_display: resolved.utc_offset_display(),
        utc_instant: resolved.utc_instant().to_owned(),
        location_label: location.label().to_owned(),
        administrative_names: location.administrative_names().to_vec(),
        country_code: location.country_code().to_owned(),
        zodiac: zodiac.to_owned(),
        house_system: house_system.to_owned(),
    })
}

fn protocol_scene(scene: ChartScene) -> BiwheelScene {
    BiwheelScene {
        timestamp: scene.timestamp,
        natal: BiwheelRing {
            timestamp: scene.natal.timestamp,
            zodiac: scene.natal.zodiac,
            house_system: scene.natal.house_system,
            points: scene
                .natal
                .points
                .into_iter()
                .map(protocol_biwheel_point)
                .collect(),
            houses: scene.natal.houses,
            ascendant_degrees: scene.natal.ascendant_degrees,
        },
        transit_zodiac: scene.transit_zodiac,
        transit_house_system: scene.transit_house_system,
        transit: scene
            .transit
            .into_iter()
            .map(protocol_biwheel_point)
            .collect(),
        aspects: scene
            .aspects
            .into_iter()
            .map(|aspect| BiwheelAspect {
                id: aspect.id,
                natal_point_id: aspect.natal_point_id,
                transit_point_id: aspect.transit_point_id,
                kind: aspect.kind,
                orb_degrees: aspect.orb_degrees,
                phase: aspect.phase,
            })
            .collect(),
    }
}

fn protocol_biwheel_point(point: ChartPoint) -> BiwheelPoint {
    BiwheelPoint {
        id: point.id,
        longitude_degrees: point.longitude_degrees,
        longitude_speed_degrees_per_day: point.longitude_speed_degrees_per_day,
        retrograde: point.retrograde,
    }
}

fn resolved_time_summary(value: &oracle_studio_core::ResolvedLocalTime) -> ResolvedTimeSummary {
    ResolvedTimeSummary {
        abbreviation: value.abbreviation().to_owned(),
        utc_offset_display: value.utc_offset_display(),
        utc_instant: value.utc_instant().to_owned(),
    }
}

const fn protocol_role(value: ChartRole) -> ChartRoleInput {
    match value {
        ChartRole::Natal => ChartRoleInput::Natal,
        ChartRole::Event => ChartRoleInput::Event,
        ChartRole::Transit => ChartRoleInput::Transit,
    }
}

const fn protocol_zodiac(value: ZodiacId) -> ZodiacInput {
    match value {
        ZodiacId::Tropical => ZodiacInput::Tropical,
        ZodiacId::Sidereal => ZodiacInput::Sidereal,
    }
}

const fn protocol_ayanamsa(value: AyanamsaId) -> AyanamsaInput {
    match value {
        AyanamsaId::FaganBradley => AyanamsaInput::FaganBradley,
        AyanamsaId::Lahiri => AyanamsaInput::Lahiri,
        AyanamsaId::DeLuce => AyanamsaInput::DeLuce,
        AyanamsaId::Raman => AyanamsaInput::Raman,
        AyanamsaId::Krishnamurti => AyanamsaInput::Krishnamurti,
        AyanamsaId::Yukteshwar => AyanamsaInput::Yukteshwar,
        AyanamsaId::JnBhasin => AyanamsaInput::JnBhasin,
    }
}

const fn protocol_house_system(value: HouseSystemId) -> HouseSystemInput {
    match value {
        HouseSystemId::Placidus => HouseSystemInput::Placidus,
        HouseSystemId::Koch => HouseSystemInput::Koch,
        HouseSystemId::Porphyry => HouseSystemInput::Porphyry,
        HouseSystemId::Regiomontanus => HouseSystemInput::Regiomontanus,
        HouseSystemId::Campanus => HouseSystemInput::Campanus,
        HouseSystemId::Equal => HouseSystemInput::Equal,
        HouseSystemId::WholeSign => HouseSystemInput::WholeSign,
    }
}

const fn protocol_object(value: CelestialObjectId) -> CelestialObjectInput {
    match value {
        CelestialObjectId::Moon => CelestialObjectInput::Moon,
        CelestialObjectId::Sun => CelestialObjectInput::Sun,
        CelestialObjectId::Mercury => CelestialObjectInput::Mercury,
        CelestialObjectId::Venus => CelestialObjectInput::Venus,
        CelestialObjectId::Mars => CelestialObjectInput::Mars,
        CelestialObjectId::Jupiter => CelestialObjectInput::Jupiter,
        CelestialObjectId::Saturn => CelestialObjectInput::Saturn,
        CelestialObjectId::Uranus => CelestialObjectInput::Uranus,
        CelestialObjectId::Neptune => CelestialObjectInput::Neptune,
        CelestialObjectId::Pluto => CelestialObjectInput::Pluto,
        CelestialObjectId::MeanNode => CelestialObjectInput::MeanNode,
        CelestialObjectId::TrueNode => CelestialObjectInput::TrueNode,
        CelestialObjectId::Chiron => CelestialObjectInput::Chiron,
    }
}

const fn protocol_point(value: ChartPointId) -> ChartPointInput {
    match value {
        ChartPointId::Moon => ChartPointInput::Moon,
        ChartPointId::Sun => ChartPointInput::Sun,
        ChartPointId::Mercury => ChartPointInput::Mercury,
        ChartPointId::Venus => ChartPointInput::Venus,
        ChartPointId::Mars => ChartPointInput::Mars,
        ChartPointId::Jupiter => ChartPointInput::Jupiter,
        ChartPointId::Saturn => ChartPointInput::Saturn,
        ChartPointId::Uranus => ChartPointInput::Uranus,
        ChartPointId::Neptune => ChartPointInput::Neptune,
        ChartPointId::Pluto => ChartPointInput::Pluto,
        ChartPointId::MeanNode => ChartPointInput::MeanNode,
        ChartPointId::TrueNode => ChartPointInput::TrueNode,
        ChartPointId::Chiron => ChartPointInput::Chiron,
        ChartPointId::MeanSouthNode => ChartPointInput::MeanSouthNode,
        ChartPointId::TrueSouthNode => ChartPointInput::TrueSouthNode,
        ChartPointId::Ascendant => ChartPointInput::Ascendant,
        ChartPointId::Midheaven => ChartPointInput::Midheaven,
        ChartPointId::Descendant => ChartPointInput::Descendant,
        ChartPointId::ImumCoeli => ChartPointInput::ImumCoeli,
        ChartPointId::Vertex => ChartPointInput::Vertex,
    }
}

const fn protocol_aspect(value: AspectKindId) -> AspectKindInput {
    match value {
        AspectKindId::Conjunction => AspectKindInput::Conjunction,
        AspectKindId::Opposition => AspectKindInput::Opposition,
        AspectKindId::Square => AspectKindInput::Square,
        AspectKindId::Trine => AspectKindInput::Trine,
        AspectKindId::Sextile => AspectKindInput::Sextile,
    }
}

const fn protocol_orientation(value: WheelOrientation) -> WheelOrientationInput {
    match value {
        WheelOrientation::AscendantLeft => WheelOrientationInput::AscendantLeft,
        WheelOrientation::AriesTop => WheelOrientationInput::AriesTop,
    }
}

fn chart_calculation_request(
    request: CalculateChartRequest,
    chart_calculation_id: StableId,
) -> Result<ChartCalculationRequest, oracle_studio_core::ModelError> {
    Ok(ChartCalculationRequest {
        chart_calculation_id,
        calculation_artifact_id: StableId::new("artifact.id", request.calculation_artifact_id)?,
        chart_definition_id: StableId::new("chart_definition.id", request.chart_definition_id)?,
        saved_location_id: StableId::new("saved_location.id", request.saved_location_id)?,
        ambiguous_time_choice: request.ambiguous_time_choice.map(|choice| match choice {
            AmbiguousTimeChoiceInput::Earlier => AmbiguousTimeChoice::Earlier,
            AmbiguousTimeChoiceInput::Later => AmbiguousTimeChoice::Later,
        }),
        calculated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

fn optional_id(
    field: &'static str,
    value: Option<String>,
) -> Result<Option<StableId>, oracle_studio_core::ModelError> {
    value.map(|value| StableId::new(field, value)).transpose()
}

fn map_object(value: CelestialObjectInput) -> CelestialObjectId {
    match value {
        CelestialObjectInput::Moon => CelestialObjectId::Moon,
        CelestialObjectInput::Sun => CelestialObjectId::Sun,
        CelestialObjectInput::Mercury => CelestialObjectId::Mercury,
        CelestialObjectInput::Venus => CelestialObjectId::Venus,
        CelestialObjectInput::Mars => CelestialObjectId::Mars,
        CelestialObjectInput::Jupiter => CelestialObjectId::Jupiter,
        CelestialObjectInput::Saturn => CelestialObjectId::Saturn,
        CelestialObjectInput::Uranus => CelestialObjectId::Uranus,
        CelestialObjectInput::Neptune => CelestialObjectId::Neptune,
        CelestialObjectInput::Pluto => CelestialObjectId::Pluto,
        CelestialObjectInput::MeanNode => CelestialObjectId::MeanNode,
        CelestialObjectInput::TrueNode => CelestialObjectId::TrueNode,
        CelestialObjectInput::Chiron => CelestialObjectId::Chiron,
    }
}

fn map_point(value: ChartPointInput) -> ChartPointId {
    match value {
        ChartPointInput::Moon => ChartPointId::Moon,
        ChartPointInput::Sun => ChartPointId::Sun,
        ChartPointInput::Mercury => ChartPointId::Mercury,
        ChartPointInput::Venus => ChartPointId::Venus,
        ChartPointInput::Mars => ChartPointId::Mars,
        ChartPointInput::Jupiter => ChartPointId::Jupiter,
        ChartPointInput::Saturn => ChartPointId::Saturn,
        ChartPointInput::Uranus => ChartPointId::Uranus,
        ChartPointInput::Neptune => ChartPointId::Neptune,
        ChartPointInput::Pluto => ChartPointId::Pluto,
        ChartPointInput::MeanNode => ChartPointId::MeanNode,
        ChartPointInput::TrueNode => ChartPointId::TrueNode,
        ChartPointInput::Chiron => ChartPointId::Chiron,
        ChartPointInput::MeanSouthNode => ChartPointId::MeanSouthNode,
        ChartPointInput::TrueSouthNode => ChartPointId::TrueSouthNode,
        ChartPointInput::Ascendant => ChartPointId::Ascendant,
        ChartPointInput::Midheaven => ChartPointId::Midheaven,
        ChartPointInput::Descendant => ChartPointId::Descendant,
        ChartPointInput::ImumCoeli => ChartPointId::ImumCoeli,
        ChartPointInput::Vertex => ChartPointId::Vertex,
    }
}

fn map_ayanamsa(value: AyanamsaInput) -> AyanamsaId {
    match value {
        AyanamsaInput::FaganBradley => AyanamsaId::FaganBradley,
        AyanamsaInput::Lahiri => AyanamsaId::Lahiri,
        AyanamsaInput::DeLuce => AyanamsaId::DeLuce,
        AyanamsaInput::Raman => AyanamsaId::Raman,
        AyanamsaInput::Krishnamurti => AyanamsaId::Krishnamurti,
        AyanamsaInput::Yukteshwar => AyanamsaId::Yukteshwar,
        AyanamsaInput::JnBhasin => AyanamsaId::JnBhasin,
    }
}

fn persist_result(
    current: &mut VaultSession,
    next: Result<VaultDocument, oracle_studio_app::AppError>,
    record_id: &str,
) -> Response {
    let next = match next {
        Ok(next) => next,
        Err(error) => return app_error(error),
    };
    let revision = match current.vault.save(
        &next,
        &current.password,
        &ExpectedState::Revision(current.revision.clone()),
    ) {
        Ok(revision) => revision,
        Err(error) => return storage_error(error),
    };
    current.document = next;
    current.revision = revision;
    current.last_activity = Instant::now();
    Json(ApiResponse::current(MutationResult {
        revision: current.revision.as_str().to_owned(),
        record_id: record_id.to_owned(),
    }))
    .into_response()
}

fn app_error(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError::current(
            ApiErrorCode::BadRequest,
            error.to_string(),
        )),
    )
        .into_response()
}

fn catalog_status_summary(metadata: Option<&CatalogMetadata>) -> CatalogStatus {
    CatalogStatus {
        installed: metadata.is_some(),
        content_id: metadata.map(|metadata| metadata.content_id.clone()),
        retrieved_at: metadata.map(|metadata| metadata.retrieved_at.clone()),
        place_count: metadata.map(|metadata| metadata.place_count),
        attribution: ATTRIBUTION.into(),
        license_name: LICENSE_NAME.into(),
        license_url: LICENSE_URL.into(),
        distribution_url: DISTRIBUTION_URL.into(),
    }
}

fn download_geonames_catalog() -> Result<CatalogInstallInput, CatalogOperationError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(180)))
        .build();
    let agent: ureq::Agent = config.into();
    let cities500_zip = download_catalog_file(&agent, CITIES500_URL, MAX_ARCHIVE_BYTES)?;
    let admin1_codes = download_catalog_file(&agent, ADMIN1_CODES_URL, 64 * 1024 * 1024)?;
    let admin2_codes = download_catalog_file(&agent, ADMIN2_CODES_URL, 64 * 1024 * 1024)?;
    Ok(CatalogInstallInput {
        cities500_zip,
        admin1_codes,
        admin2_codes,
        retrieved_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

fn download_catalog_file(
    agent: &ureq::Agent,
    url: &'static str,
    limit: usize,
) -> Result<Vec<u8>, CatalogOperationError> {
    agent
        .get(url)
        .header(
            "User-Agent",
            "Oracle-Studio/0.1 (offline GeoNames catalog installer)",
        )
        .call()
        .map_err(|error| CatalogOperationError::Download(error.to_string()))?
        .body_mut()
        .with_config()
        .limit(limit as u64)
        .read_to_vec()
        .map_err(|error| CatalogOperationError::Download(error.to_string()))
}

fn catalog_error(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError::current(
            ApiErrorCode::Unavailable,
            error.to_string(),
        )),
    )
        .into_response()
}

fn active_session(session: &mut SessionStore) -> Result<&VaultSession, ApiFailure> {
    session.expire_and_touch(Instant::now());
    session.current.as_ref().ok_or_else(|| {
        ApiFailure::new(
            StatusCode::LOCKED,
            ApiErrorCode::Locked,
            "unlock a vault before using this operation",
        )
    })
}

fn active_session_mut(session: &mut SessionStore) -> Result<&mut VaultSession, ApiFailure> {
    session.expire_and_touch(Instant::now());
    session.current.as_mut().ok_or_else(|| {
        ApiFailure::new(
            StatusCode::LOCKED,
            ApiErrorCode::Locked,
            "unlock a vault before using this operation",
        )
    })
}

fn require_protocol(version: u16) -> Result<(), ApiFailure> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ApiFailure::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ProtocolMismatch,
            "the UI and native host use different protocol versions",
        ))
    }
}

fn storage_error(error: StorageError) -> Response {
    match error {
        StorageError::Conflict => api_error(
            StatusCode::CONFLICT,
            ApiErrorCode::Conflict,
            "the vault changed since it was last read",
        ),
        StorageError::Vault(_) => api_error(
            StatusCode::UNAUTHORIZED,
            ApiErrorCode::VaultAuthentication,
            "the vault could not be authenticated with that password",
        ),
        StorageError::Io(ref error) if error.kind() == io::ErrorKind::NotFound => api_error(
            StatusCode::NOT_FOUND,
            ApiErrorCode::NotFound,
            "the requested vault does not exist",
        ),
        StorageError::Busy => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiErrorCode::Unavailable,
            "the vault is busy in another operation",
        ),
        _ => api_error(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::BadRequest,
            "the vault path or storage operation was not accepted",
        ),
    }
}

fn api_error(status: StatusCode, code: ApiErrorCode, message: &'static str) -> Response {
    (status, Json(ApiError::current(code, message))).into_response()
}

#[derive(Clone, Copy)]
struct ApiFailure {
    status: StatusCode,
    code: ApiErrorCode,
    message: &'static str,
}

impl ApiFailure {
    const fn new(status: StatusCode, code: ApiErrorCode, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        api_error(self.status, self.code, self.message)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("Studio may bind only to 127.0.0.1, not {0}")]
    NonLoopbackBind(SocketAddr),
    #[error("the Studio origin must be an HTTP loopback origin")]
    InvalidOrigin,
    #[error("operating-system randomness failed: {0}")]
    Randomness(String),
    #[error("a catalog root requires an absolute XDG_DATA_HOME or HOME directory")]
    CatalogRootUnavailable,
}

#[derive(Debug, thiserror::Error)]
enum CatalogOperationError {
    #[error("GeoNames download failed: {0}")]
    Download(String),
    #[error("GeoNames catalog installation failed: {0}")]
    Catalog(String),
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::*;

    const ORIGIN: &str = "http://127.0.0.1:4567";
    const HOST: &str = "127.0.0.1:4567";
    const TOKEN: &str = "0707070707070707070707070707070707070707070707070707070707070707";

    fn test_app(timeout: Duration) -> Router {
        app(
            AppState::new(ORIGIN, TOKEN, timeout).unwrap(),
            Path::new("missing-test-distribution"),
        )
    }

    fn test_app_with_catalog(timeout: Duration, catalog_root: &Path) -> Router {
        app(
            AppState::with_catalog_root(ORIGIN, TOKEN, timeout, catalog_root).unwrap(),
            Path::new("missing-test-distribution"),
        )
    }

    fn api_request(path: &str, token: &str, origin: &str, body: impl Into<Body>) -> Request<Body> {
        Request::post(path)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, origin)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.into())
            .unwrap()
    }

    #[test]
    fn refuses_non_loopback_bind_addresses() {
        let public = SocketAddr::from(([0, 0, 0, 0], 8080));
        assert!(matches!(
            validate_loopback(public),
            Err(HostError::NonLoopbackBind(address)) if address == public
        ));
        assert!(validate_loopback(SocketAddr::from(([127, 0, 0, 1], 0))).is_ok());
    }

    #[test]
    fn launch_token_has_256_bits_of_hex_material() {
        let token = launch_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn application_state_accepts_only_exact_loopback_origins() {
        assert!(AppState::new(ORIGIN, TOKEN, DEFAULT_IDLE_TIMEOUT).is_ok());
        assert!(AppState::new("http://0.0.0.0:4567", TOKEN, DEFAULT_IDLE_TIMEOUT).is_err());
        assert!(AppState::new("https://127.0.0.1:4567", TOKEN, DEFAULT_IDLE_TIMEOUT).is_err());
        assert!(AppState::new("http://127.0.0.1:0", TOKEN, DEFAULT_IDLE_TIMEOUT).is_err());
    }

    #[test]
    fn csp_hashes_generated_inline_bootstrap_without_allowing_all_inline_code() {
        let index = "<script src=\"/external.js\"></script><script>console.log('ok');</script>";
        assert_eq!(
            inline_script_hash_sources(index),
            vec!["'sha256-FrqULMBzC5wUFutTLAFbXSa/hBlhjjFaviVEuHrmOhY='"]
        );
        let header = content_security_policy(index);
        let policy = header.to_str().unwrap();
        assert!(policy.contains("'wasm-unsafe-eval' 'sha256-"));
        assert!(!policy.contains("'unsafe-inline'"));
    }

    #[tokio::test]
    async fn every_api_operation_requires_origin_host_and_token() {
        let body = r#"{"protocol_version":1}"#;
        let denied_token = test_app(DEFAULT_IDLE_TIMEOUT)
            .oneshot(api_request("/api/v1/session/status", "wrong", ORIGIN, body))
            .await
            .unwrap();
        assert_eq!(denied_token.status(), StatusCode::UNAUTHORIZED);

        let denied_origin = test_app(DEFAULT_IDLE_TIMEOUT)
            .oneshot(api_request(
                "/api/v1/session/status",
                TOKEN,
                "http://example.test",
                body,
            ))
            .await
            .unwrap();
        assert_eq!(denied_origin.status(), StatusCode::UNAUTHORIZED);

        let accepted = test_app(DEFAULT_IDLE_TIMEOUT)
            .oneshot(api_request("/api/v1/session/status", TOKEN, ORIGIN, body))
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        assert_eq!(
            accepted.headers().get(header::CONTENT_SECURITY_POLICY),
            Some(&HeaderValue::from_static(CSP))
        );
    }

    #[tokio::test]
    async fn strict_json_rejects_unknown_fields() {
        let response = test_app(DEFAULT_IDLE_TIMEOUT)
            .oneshot(api_request(
                "/api/v1/session/status",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1,"unexpected":true}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn protocol_errors_do_not_echo_request_data() {
        let response = test_app(DEFAULT_IDLE_TIMEOUT)
            .oneshot(api_request(
                "/api/v1/vault/unlock",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":99,"vault_path":"/secret/path","password":"secret"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("/secret/path"));
        assert!(!body.contains("secret"));
    }

    #[tokio::test]
    async fn accepted_mutations_are_atomically_persisted_in_the_encrypted_vault() {
        let suffix = launch_token().unwrap();
        let test_directory =
            std::env::temp_dir().join(format!("oracle-studio-{}", suffix.as_str()));
        std::fs::create_dir(&test_directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&test_directory, std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let vault_path = test_directory.join("test.oracle");
        let file_name = vault_path.file_name().unwrap().to_string_lossy();
        let lock_path = vault_path.with_file_name(format!(".{file_name}.lock"));
        let router = test_app(DEFAULT_IDLE_TIMEOUT);
        let create = serde_json::json!({
            "protocol_version": 1,
            "vault_path": vault_path.to_string_lossy(),
            "password": "test-only-password"
        })
        .to_string();
        let response = router
            .clone()
            .oneshot(api_request("/api/v1/vault/create", TOKEN, ORIGIN, create))
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));

        let save_person = serde_json::json!({
            "protocol_version": 1,
            "id": "fictional_person",
            "display_name": "Fictional <script>person</script>",
            "kind": "personal",
            "notes": null
        })
        .to_string();
        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/people/save",
                TOKEN,
                ORIGIN,
                save_person,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let save_location = serde_json::json!({
            "protocol_version": 1,
            "id": "fictional_location",
            "label": "Fictional City",
            "administrative_names": ["Example County"],
            "country_code": "US",
            "latitude_degrees": 42.65,
            "longitude_degrees": -73.75,
            "elevation_meters": 84.0,
            "time_zone": "America/New_York",
            "provenance": {"kind": "manual"}
        })
        .to_string();
        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/locations/save",
                TOKEN,
                ORIGIN,
                save_location,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let loaded = FileVault::new(&vault_path)
            .unwrap()
            .load(b"test-only-password")
            .unwrap();
        assert_eq!(loaded.document().people().len(), 1);
        assert_eq!(loaded.document().saved_locations().len(), 1);
        assert_eq!(
            loaded.document().people()[0].display_name(),
            "Fictional <script>person</script>"
        );

        std::fs::remove_file(&vault_path).unwrap();
        std::fs::remove_file(&lock_path).unwrap();
        std::fs::remove_dir(&test_directory).unwrap();
    }

    #[tokio::test]
    async fn catalog_status_and_search_use_only_the_installed_local_pack() {
        let suffix = launch_token().unwrap();
        let test_directory =
            std::env::temp_dir().join(format!("oracle-catalog-api-{}", suffix.as_str()));
        let fields = [
            "99",
            "Fictional City",
            "Fictional City",
            "Example Place",
            "42.6500",
            "-73.7500",
            "P",
            "PPL",
            "US",
            "",
            "NY",
            "001",
            "",
            "",
            "12345",
            "84",
            "80",
            "America/New_York",
            "2026-01-01",
        ];
        let mut archive = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file(
                "cities500.txt",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(fields.join("\t").as_bytes()).unwrap();
        archive.write_all(b"\n").unwrap();
        let cities500_zip = archive.finish().unwrap().into_inner();
        CatalogStore::new(&test_directory)
            .install(CatalogInstallInput {
                cities500_zip,
                admin1_codes: b"US.NY\tNew York\tNew York\t1\n".to_vec(),
                admin2_codes: b"US.NY.001\tExample County\tExample County\t2\n".to_vec(),
                retrieved_at: "2026-08-18T12:00:00Z".into(),
            })
            .unwrap();
        let router = test_app_with_catalog(DEFAULT_IDLE_TIMEOUT, &test_directory);

        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/catalog/status",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let status: ApiResponse<CatalogStatus> = serde_json::from_slice(&body).unwrap();
        assert!(status.data.installed);
        assert_eq!(status.data.place_count, Some(1));
        assert_eq!(status.data.attribution, ATTRIBUTION);

        let response = router
            .oneshot(api_request(
                "/api/v1/catalog/search",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1,"query":"example place","limit":10}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let results: ApiResponse<Vec<CatalogPlaceSummary>> = serde_json::from_slice(&body).unwrap();
        assert_eq!(results.data.len(), 1);
        assert_eq!(results.data[0].geonames_id, 99);
        assert_eq!(
            results.data[0].administrative_names,
            ["Example County", "New York"]
        );
        assert_eq!(results.data[0].match_kind, CatalogMatchKind::Exact);

        std::fs::remove_dir_all(&test_directory).unwrap();
    }

    #[tokio::test]
    async fn encrypted_chart_workflow_presents_exact_selected_biwheel() {
        let suffix = launch_token().unwrap();
        let test_directory =
            std::env::temp_dir().join(format!("oracle-workspace-api-{}", suffix.as_str()));
        std::fs::create_dir(&test_directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&test_directory, std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let vault_path = test_directory.join("workspace.oracle");
        let file_name = vault_path.file_name().unwrap().to_string_lossy();
        let lock_path = vault_path.with_file_name(format!(".{file_name}.lock"));
        let router = test_app(DEFAULT_IDLE_TIMEOUT);

        let requests = [
            (
                "/api/v1/vault/create",
                serde_json::json!({
                    "protocol_version": 1,
                    "vault_path": vault_path.to_string_lossy(),
                    "password": "test-only-password"
                }),
            ),
            (
                "/api/v1/people/save",
                serde_json::json!({
                    "protocol_version": 1,
                    "id": "fictional_person",
                    "display_name": "Fictional Person",
                    "kind": "personal",
                    "notes": null
                }),
            ),
            (
                "/api/v1/locations/save",
                serde_json::json!({
                    "protocol_version": 1,
                    "id": "fictional_location",
                    "label": "Fictional City",
                    "administrative_names": ["Example State"],
                    "country_code": "US",
                    "latitude_degrees": 40.7128,
                    "longitude_degrees": -74.0060,
                    "elevation_meters": 10.0,
                    "time_zone": "America/New_York",
                    "provenance": {"kind": "manual"}
                }),
            ),
            (
                "/api/v1/charts/save",
                serde_json::json!({
                    "protocol_version": 1,
                    "id": "fictional_natal",
                    "label": "Fictional Natal",
                    "role": "natal",
                    "person_id": "fictional_person",
                    "local_date": "2000-01-15",
                    "local_time": "12:00:00",
                    "time_zone": "America/New_York",
                    "zodiac": "tropical",
                    "ayanamsa": null,
                    "house_system": "placidus",
                    "ordered_objects": ["moon", "sun"],
                    "ordered_points": ["moon", "sun", "ascendant", "midheaven"],
                    "default_natal": true
                }),
            ),
            (
                "/api/v1/charts/save",
                serde_json::json!({
                    "protocol_version": 1,
                    "id": "fictional_transit",
                    "label": "Fictional Transit",
                    "role": "transit",
                    "person_id": null,
                    "local_date": "2026-08-17",
                    "local_time": "16:20:00",
                    "time_zone": "America/New_York",
                    "zodiac": "tropical",
                    "ayanamsa": null,
                    "house_system": "placidus",
                    "ordered_objects": ["moon", "sun"],
                    "ordered_points": ["moon", "sun", "ascendant", "midheaven"],
                    "default_natal": false
                }),
            ),
        ];
        for (path, body) in requests {
            let response = router
                .clone()
                .oneshot(api_request(path, TOKEN, ORIGIN, body.to_string()))
                .await
                .unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
            assert_eq!(
                status,
                StatusCode::OK,
                "{path}: {}",
                String::from_utf8_lossy(&body)
            );
        }

        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/charts/time-resolution",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1,"chart_definition_id":"fictional_transit"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        let resolved: ApiResponse<LocalTimeResolutionSummary> =
            serde_json::from_slice(&body).unwrap();
        assert!(matches!(
            resolved.data,
            LocalTimeResolutionSummary::Unique { value }
                if value.abbreviation == "EDT"
                    && value.utc_offset_display == "UTC-04:00"
                    && value.utc_instant == "2026-08-17T20:20:00Z"
        ));

        for (calculation_id, artifact_id, chart_id) in [
            (
                "fictional_natal_calc",
                "fictional_natal_artifact",
                "fictional_natal",
            ),
            (
                "fictional_transit_calc",
                "fictional_transit_artifact",
                "fictional_transit",
            ),
        ] {
            let body = serde_json::json!({
                "protocol_version": 1,
                "chart_calculation_id": calculation_id,
                "calculation_artifact_id": artifact_id,
                "chart_definition_id": chart_id,
                "saved_location_id": "fictional_location",
                "ambiguous_time_choice": null
            })
            .to_string();
            let response = router
                .clone()
                .oneshot(api_request("/api/v1/charts/calculate", TOKEN, ORIGIN, body))
                .await
                .unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
            assert_eq!(
                status,
                StatusCode::OK,
                "{chart_id}: {}",
                String::from_utf8_lossy(&body)
            );
        }

        let comparison = serde_json::json!({
            "protocol_version": 1,
            "id": "fictional_comparison",
            "label": "Fictional Natal + Transit",
            "inner_chart_definition_id": "fictional_natal",
            "outer_chart_definition_id": "fictional_transit",
            "inner_points": ["moon", "sun", "ascendant", "midheaven"],
            "outer_points": ["moon", "sun", "ascendant", "midheaven"],
            "aspects": [
                {"kind": "conjunction", "orb_degrees": 8.0},
                {"kind": "opposition", "orb_degrees": 8.0},
                {"kind": "square", "orb_degrees": 6.0},
                {"kind": "trine", "orb_degrees": 6.0},
                {"kind": "sextile", "orb_degrees": 4.0}
            ],
            "orientation": "ascendant_left"
        })
        .to_string();
        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/comparisons/save",
                TOKEN,
                ORIGIN,
                comparison,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/comparisons/calculate",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1,"comparison_artifact_id":"fictional_comparison_artifact","comparison_preset_id":"fictional_comparison"}"#,
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));

        let response = router
            .clone()
            .oneshot(api_request(
                "/api/v1/workspace/set",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1,"active_person_id":"fictional_person","active_comparison_id":"fictional_comparison"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .oneshot(api_request(
                "/api/v1/workspace/view",
                TOKEN,
                ORIGIN,
                r#"{"protocol_version":1}"#,
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 512 * 1024).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let workspace: ApiResponse<Option<WorkspacePresentation>> =
            serde_json::from_slice(&body).unwrap();
        let workspace = workspace.data.expect("active presentation");
        assert_eq!(workspace.inner.chart_label, "Fictional Natal");
        assert_eq!(
            workspace.inner.person_label.as_deref(),
            Some("Fictional Person")
        );
        assert_eq!(workspace.inner.abbreviation, "EST");
        assert_eq!(workspace.inner.utc_offset_display, "UTC-05:00");
        assert_eq!(workspace.inner.utc_instant, "2000-01-15T17:00:00Z");
        assert_eq!(workspace.outer.abbreviation, "EDT");
        assert_eq!(workspace.outer.utc_instant, "2026-08-17T20:20:00Z");
        assert_eq!(workspace.scene.natal.points.len(), 4);
        assert_eq!(workspace.scene.transit.len(), 4);
        assert_eq!(workspace.orientation, WheelOrientationInput::AscendantLeft);

        std::fs::remove_file(&vault_path).unwrap();
        std::fs::remove_file(&lock_path).unwrap();
        std::fs::remove_dir(&test_directory).unwrap();
    }

    #[test]
    fn inactivity_expiration_clears_native_session() {
        let vault = FileVault::new(PathBuf::from("/tmp/fictional.oracle")).unwrap();
        let mut store = SessionStore::new(Duration::from_secs(900));
        let start = Instant::now();
        store.replace(
            vault,
            VaultDocument::empty(),
            VaultRevision::parse(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            Zeroizing::new(b"secret".to_vec()),
            start,
        );
        store.expire_and_touch(start + Duration::from_secs(901));
        assert!(store.current.is_none());
    }
}
