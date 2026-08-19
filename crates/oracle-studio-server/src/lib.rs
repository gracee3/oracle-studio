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
use oracle_studio_core::{PersonKind, VaultDocument};
use oracle_studio_protocol::{
    ApiError, ApiErrorCode, ApiResponse, ChartSummary, ComparisonSummary, CreateVaultRequest,
    LocationSummary, PROTOCOL_VERSION, PersonSummary, ProtocolRequest, SessionStatus,
    UnlockVaultRequest, VaultState, WorkspaceSummary,
};
use oracle_studio_storage::{ExpectedState, FileVault, StorageError, VaultRevision};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::Mutex};
use tower_http::services::{ServeDir, ServeFile};
use zeroize::Zeroizing;

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const CSP: &str = "default-src 'self'; connect-src 'self'; font-src 'self' data:; img-src 'self' data:; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

#[derive(Clone)]
pub struct AppState(Arc<AppStateInner>);

struct AppStateInner {
    expected_origin: String,
    expected_host: String,
    bearer_token: Zeroizing<String>,
    session: Mutex<SessionStore>,
}

impl AppState {
    pub fn new(
        expected_origin: impl Into<String>,
        bearer_token: impl Into<String>,
        idle_timeout: Duration,
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
        })))
    }
}

struct VaultSession {
    vault: FileVault,
    document: VaultDocument,
    revision: VaultRevision,
    _password: Zeroizing<Vec<u8>>,
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
            _password: password,
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
    let api = Router::new()
        .route("/session/status", post(session_status))
        .route("/vault/create", post(create_vault))
        .route("/vault/unlock", post(unlock_vault))
        .route("/vault/lock", post(lock_vault))
        .route("/people/list", post(list_people))
        .route("/locations/list", post(list_locations))
        .route("/charts/list", post(list_charts))
        .route("/comparisons/list", post(list_comparisons))
        .route("/workspace/get", post(get_workspace))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize_api))
        .with_state(state);
    let distribution = distribution.as_ref().to_owned();
    let index = distribution.join("index.html");
    Router::new()
        .nest("/api/v1", api)
        .fallback_service(ServeDir::new(distribution).not_found_service(ServeFile::new(index)))
        .layer(middleware::from_fn(security_headers))
}

pub async fn bind_loopback(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)).await
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

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
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
        })
        .collect::<Vec<_>>();
    Json(ApiResponse::current(people)).into_response()
}

async fn list_locations(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    empty_unlocked_list::<LocationSummary>(&state, request).await
}

async fn list_charts(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    empty_unlocked_list::<ChartSummary>(&state, request).await
}

async fn list_comparisons(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    empty_unlocked_list::<ComparisonSummary>(&state, request).await
}

async fn empty_unlocked_list<T>(state: &AppState, request: ProtocolRequest) -> Response
where
    T: serde::Serialize,
{
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    if let Err(response) = active_session(&mut session) {
        return response.into_response();
    }
    Json(ApiResponse::current(Vec::<T>::new())).into_response()
}

async fn get_workspace(
    State(state): State<AppState>,
    Json(request): Json<ProtocolRequest>,
) -> Response {
    if let Err(response) = require_protocol(request.protocol_version) {
        return response.into_response();
    }
    let mut session = state.0.session.lock().await;
    if let Err(response) = active_session(&mut session) {
        return response.into_response();
    }
    Json(ApiResponse::current(WorkspaceSummary::default())).into_response()
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
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

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

    fn api_request(path: &str, token: &str, origin: &str, body: &'static str) -> Request<Body> {
        Request::post(path)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, origin)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
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
