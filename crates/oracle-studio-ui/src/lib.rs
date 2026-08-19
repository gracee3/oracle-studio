//! Browser-side Oracle Studio shell and replaceable platform-service boundary.

use std::{future::Future, pin::Pin, sync::Arc};

use gloo_net::http::Request;
use leptos::{ev::SubmitEvent, html::Input, prelude::*};
use leptos_router::{
    components::{A, Route, Router, Routes},
    path,
};
use oracle_studio_protocol::{
    ApiError, ApiResponse, ChartSummary, ComparisonSummary, CreateVaultRequest, LocationSummary,
    PersonSummary, ProtocolRequest, SessionStatus, UnlockVaultRequest, VaultState,
    WorkspaceSummary,
};
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::JsValue;

pub type PlatformFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, PlatformError>> + 'a>>;

/// Native capabilities consumed by components. HTTP is one implementation, not
/// part of the component contract, so a future Tauri or browser-local adapter
/// can replace it without changing routes or presenters.
pub trait StudioPlatform: Send + Sync {
    fn session_status(&self) -> PlatformFuture<'_, SessionStatus>;
    fn create_vault(&self, request: CreateVaultRequest) -> PlatformFuture<'_, SessionStatus>;
    fn unlock_vault(&self, request: UnlockVaultRequest) -> PlatformFuture<'_, SessionStatus>;
    fn lock_vault(&self) -> PlatformFuture<'_, SessionStatus>;
    fn people(&self) -> PlatformFuture<'_, Vec<PersonSummary>>;
    fn locations(&self) -> PlatformFuture<'_, Vec<LocationSummary>>;
    fn charts(&self) -> PlatformFuture<'_, Vec<ChartSummary>>;
    fn comparisons(&self) -> PlatformFuture<'_, Vec<ComparisonSummary>>;
    fn workspace(&self) -> PlatformFuture<'_, WorkspaceSummary>;
}

#[derive(Clone)]
pub struct HttpStudioPlatform {
    bearer_token: String,
}

impl HttpStudioPlatform {
    pub fn from_launch_fragment() -> Result<Self, PlatformError> {
        let window = web_sys::window().ok_or_else(|| PlatformError::new("window unavailable"))?;
        let location = window.location();
        let hash = location
            .hash()
            .map_err(|_| PlatformError::new("launch fragment unavailable"))?;
        let token = hash
            .trim_start_matches('#')
            .split('&')
            .find_map(|part| part.strip_prefix("token="))
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| {
                PlatformError::new("Open Studio with the one-time URL printed by the native host.")
            })?
            .to_owned();

        let replacement = format!(
            "{}{}",
            location
                .pathname()
                .map_err(|_| PlatformError::new("page path unavailable"))?,
            location
                .search()
                .map_err(|_| PlatformError::new("page query unavailable"))?
        );
        window
            .history()
            .map_err(|_| PlatformError::new("browser history unavailable"))?
            .replace_state_with_url(&JsValue::NULL, "", Some(&replacement))
            .map_err(|_| PlatformError::new("could not consume the launch token"))?;
        Ok(Self {
            bearer_token: token,
        })
    }

    async fn post<RequestBody, ResponseBody>(
        &self,
        path: &str,
        body: RequestBody,
    ) -> Result<ResponseBody, PlatformError>
    where
        RequestBody: Serialize,
        ResponseBody: DeserializeOwned,
    {
        let body = serde_json::to_string(&body)
            .map_err(|_| PlatformError::new("could not encode the local request"))?;
        let response = Request::post(path)
            .header("Authorization", &format!("Bearer {}", self.bearer_token))
            .header("Content-Type", "application/json")
            .body(body)
            .map_err(|_| PlatformError::new("could not prepare the local request"))?
            .send()
            .await
            .map_err(|_| PlatformError::new("the local Studio host is unavailable"))?;
        if response.ok() {
            let response: ApiResponse<ResponseBody> = response
                .json()
                .await
                .map_err(|_| PlatformError::new("the local host returned an invalid response"))?;
            Ok(response.data)
        } else {
            let fallback = format!(
                "the local host rejected the request ({})",
                response.status()
            );
            let message = response
                .json::<ApiError>()
                .await
                .map(|error| error.message)
                .unwrap_or(fallback);
            Err(PlatformError::new(message))
        }
    }
}

impl StudioPlatform for HttpStudioPlatform {
    fn session_status(&self) -> PlatformFuture<'_, SessionStatus> {
        Box::pin(self.post("/api/v1/session/status", ProtocolRequest::current()))
    }

    fn create_vault(&self, request: CreateVaultRequest) -> PlatformFuture<'_, SessionStatus> {
        Box::pin(self.post("/api/v1/vault/create", request))
    }

    fn unlock_vault(&self, request: UnlockVaultRequest) -> PlatformFuture<'_, SessionStatus> {
        Box::pin(self.post("/api/v1/vault/unlock", request))
    }

    fn lock_vault(&self) -> PlatformFuture<'_, SessionStatus> {
        Box::pin(self.post("/api/v1/vault/lock", ProtocolRequest::current()))
    }

    fn people(&self) -> PlatformFuture<'_, Vec<PersonSummary>> {
        Box::pin(self.post("/api/v1/people/list", ProtocolRequest::current()))
    }

    fn locations(&self) -> PlatformFuture<'_, Vec<LocationSummary>> {
        Box::pin(self.post("/api/v1/locations/list", ProtocolRequest::current()))
    }

    fn charts(&self) -> PlatformFuture<'_, Vec<ChartSummary>> {
        Box::pin(self.post("/api/v1/charts/list", ProtocolRequest::current()))
    }

    fn comparisons(&self) -> PlatformFuture<'_, Vec<ComparisonSummary>> {
        Box::pin(self.post("/api/v1/comparisons/list", ProtocolRequest::current()))
    }

    fn workspace(&self) -> PlatformFuture<'_, WorkspaceSummary> {
        Box::pin(self.post("/api/v1/workspace/get", ProtocolRequest::current()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformError {
    message: String,
}

impl PlatformError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone)]
struct StudioContext {
    platform: Arc<dyn StudioPlatform>,
    status: RwSignal<Option<Result<SessionStatus, PlatformError>>>,
}

#[component]
pub fn App(platform: Arc<dyn StudioPlatform>) -> impl IntoView {
    let status = RwSignal::new(None);
    provide_context(StudioContext {
        platform: Arc::clone(&platform),
        status,
    });
    refresh_status(platform, status);

    view! {
        <Router>
            <div class="studio-shell">
                <header class="app-header">
                    <div>
                        <p class="eyebrow">"Local astrology workspace"</p>
                        <A attr:class="wordmark" href="/">"Oracle Studio"</A>
                    </div>
                    <VaultIndicator />
                </header>
                <nav class="primary-nav" aria-label="Studio sections">
                    <A href="/people">"People"</A>
                    <A href="/locations">"Locations"</A>
                    <A href="/charts/new">"New chart"</A>
                    <A href="/workspace">"Workspace"</A>
                </nav>
                <main id="main-content" tabindex="-1">
                    <Routes fallback=|| view! { <NotFound /> }>
                        <Route path=path!("") view=HomePage />
                        <Route path=path!("vault") view=VaultPage />
                        <Route path=path!("people") view=PeoplePage />
                        <Route path=path!("people/:id") view=PersonPage />
                        <Route path=path!("charts/:id") view=ChartEditorPage />
                        <Route path=path!("locations") view=LocationsPage />
                        <Route path=path!("workspace") view=WorkspacePage />
                    </Routes>
                </main>
            </div>
        </Router>
    }
}

fn refresh_status(
    platform: Arc<dyn StudioPlatform>,
    status: RwSignal<Option<Result<SessionStatus, PlatformError>>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        status.set(Some(platform.session_status().await));
    });
}

#[component]
fn VaultIndicator() -> impl IntoView {
    let context = expect_context::<StudioContext>();
    let platform = Arc::clone(&context.platform);
    let status = context.status;
    view! {
        <div class="vault-indicator" aria-live="polite">
            {move || match status.get() {
                None => view! { <span>"Checking vault…"</span> }.into_any(),
                Some(Err(error)) => view! { <span class="error-text">{error.message().to_owned()}</span> }.into_any(),
                Some(Ok(session)) if session.state == VaultState::Unlocked => view! {
                    <span class="status-dot unlocked" aria-hidden="true"></span>
                    <span>{session.vault_name.unwrap_or_else(|| "Vault".to_owned())}</span>
                    <button class="quiet-button" type="button" on:click={
                        let platform = Arc::clone(&platform);
                        move |_| {
                            let platform = Arc::clone(&platform);
                            wasm_bindgen_futures::spawn_local(async move {
                                status.set(Some(platform.lock_vault().await));
                            });
                        }
                    }>"Lock"</button>
                }.into_any(),
                Some(Ok(_)) => view! {
                    <span class="status-dot" aria-hidden="true"></span>
                    <A href="/vault">"Unlock vault"</A>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <section class="hero panel">
            <p class="eyebrow">"Charts first"</p>
            <h1>"A private studio for natal and transit work."</h1>
            <p>
                "Your vault stays encrypted on this computer. Start with a person, a saved location, and a chart; then compare natal and transit positions in the workspace."
            </p>
            <div class="button-row">
                <A attr:class="primary-button" href="/vault">"Open a vault"</A>
                <A attr:class="secondary-button" href="/workspace">"View workspace"</A>
            </div>
        </section>
        <section class="feature-grid" aria-label="Studio capabilities">
            <article class="panel"><span class="feature-number">"01"</span><h2>"People"</h2><p>"Keep chart subjects and professional clients organized inside the encrypted vault."</p></article>
            <article class="panel"><span class="feature-number">"02"</span><h2>"Places"</h2><p>"Use saved, offline location snapshots with explicit coordinates and time zones."</p></article>
            <article class="panel"><span class="feature-number">"03"</span><h2>"Biwheel"</h2><p>"Read a deterministic natal/transit comparison with precise chart metadata."</p></article>
        </section>
    }
}

#[component]
fn VaultPage() -> impl IntoView {
    view! {
        <PageHeader eyebrow="Encrypted local storage" title="Open your studio" description="The password is sent only to the loopback host and is never retained by the browser application." />
        <div class="two-column">
            <VaultForm create=false />
            <VaultForm create=true />
        </div>
    }
}

#[component]
fn VaultForm(create: bool) -> impl IntoView {
    let context = expect_context::<StudioContext>();
    let path_ref = NodeRef::<Input>::new();
    let password_ref = NodeRef::<Input>::new();
    let feedback = RwSignal::new(None::<Result<String, String>>);
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let Some(path_input) = path_ref.get() else {
            return;
        };
        let Some(password_input) = password_ref.get() else {
            return;
        };
        let path = path_input.value();
        let password = password_input.value();
        password_input.set_value("");
        if path.trim().is_empty() || password.is_empty() {
            feedback.set(Some(
                Err("Enter both a vault path and password.".to_owned()),
            ));
            return;
        }
        feedback.set(Some(Ok("Working…".to_owned())));
        let platform = Arc::clone(&context.platform);
        let status = context.status;
        wasm_bindgen_futures::spawn_local(async move {
            let result = if create {
                platform
                    .create_vault(CreateVaultRequest::current(path, password))
                    .await
            } else {
                platform
                    .unlock_vault(UnlockVaultRequest::current(path, password))
                    .await
            };
            match result {
                Ok(session) => {
                    status.set(Some(Ok(session)));
                    feedback.set(Some(Ok(if create {
                        "Vault created and unlocked.".to_owned()
                    } else {
                        "Vault unlocked.".to_owned()
                    })));
                }
                Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
            }
        });
    };
    view! {
        <form class="panel vault-form" on:submit=submit>
            <p class="eyebrow">{if create { "New vault" } else { "Existing vault" }}</p>
            <h2>{if create { "Create" } else { "Unlock" }}</h2>
            <label>
                <span>"Vault path"</span>
                <input node_ref=path_ref type="text" autocomplete="off" spellcheck="false" placeholder="/home/you/Documents/studio.oracle" />
            </label>
            <label>
                <span>"Password"</span>
                <input node_ref=password_ref type="password" autocomplete=if create { "new-password" } else { "current-password" } />
            </label>
            <button class="primary-button" type="submit">{if create { "Create vault" } else { "Unlock vault" }}</button>
            <div class="form-feedback" role="status" aria-live="polite">
                {move || feedback.get().map(|result| match result {
                    Ok(message) => view! { <span>{message}</span> }.into_any(),
                    Err(message) => view! { <span class="error-text">{message}</span> }.into_any(),
                })}
            </div>
        </form>
    }
}

#[component]
fn PeoplePage() -> impl IntoView {
    view! {
        <PageHeader eyebrow="Encrypted records" title="People" description="Create a person, attach natal charts, and choose one default natal chart per person." />
        <EmptyState title="No people loaded" body="Unlock a vault to load people. Person editing arrives with the chart workspace milestone." action="Open vault" href="/vault" />
    }
}

#[component]
fn PersonPage() -> impl IntoView {
    view! {
        <PageHeader eyebrow="Person detail" title="Charts and history" description="This route is reserved for one person’s natal charts, events, and calculation history." />
        <EmptyState title="Person details are not loaded" body="The platform boundary is ready; the schema-v3 person and chart services follow next." action="Back to people" href="/people" />
    }
}

#[component]
fn ChartEditorPage() -> impl IntoView {
    view! {
        <PageHeader eyebrow="Chart entry" title="Chart editor" description="Local civil time, location, calculation options, and ordered point selection will be edited here." />
        <EmptyState title="Chart editor foundation" body="Time-zone resolution and immutable calculation snapshots arrive with vault schema v3." action="Open locations" href="/locations" />
    }
}

#[component]
fn LocationsPage() -> impl IntoView {
    view! {
        <PageHeader eyebrow="Offline location catalog" title="Locations" description="Saved places are encrypted snapshots. The optional GeoNames catalog remains outside the vault and never sends searches over the network." />
        <EmptyState title="No catalog installed" body="Manual coordinates and time-zone entry, plus the explicit GeoNames installer, arrive in the offline-catalog milestone." action="Open vault" href="/vault" />
    }
}

#[component]
fn WorkspacePage() -> impl IntoView {
    view! {
        <PageHeader eyebrow="Natal + transit" title="Chart workspace" description="The active comparison will place chart information above a deterministic, accessible biwheel." />
        <div class="chart-placeholder panel" role="img" aria-label="Empty biwheel workspace">
            <div class="orbit orbit-outer"></div><div class="orbit orbit-inner"></div><div class="orbit orbit-core"></div>
            <span>"Choose two calculated charts"</span>
        </div>
    }
}

#[component]
fn PageHeader(
    #[prop(into)] eyebrow: String,
    #[prop(into)] title: String,
    #[prop(into)] description: String,
) -> impl IntoView {
    view! {
        <header class="page-header">
            <p class="eyebrow">{eyebrow}</p>
            <h1>{title}</h1>
            <p>{description}</p>
        </header>
    }
}

#[component]
fn EmptyState(
    #[prop(into)] title: String,
    #[prop(into)] body: String,
    #[prop(into)] action: String,
    #[prop(into)] href: String,
) -> impl IntoView {
    view! {
        <section class="panel empty-state">
            <div class="empty-mark" aria-hidden="true">"✦"</div>
            <h2>{title}</h2><p>{body}</p><A attr:class="secondary-button" href=href>{action}</A>
        </section>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <section class="panel empty-state">
            <p class="eyebrow">"404"</p><h1>"That studio route does not exist."</h1>
            <A attr:class="secondary-button" href="/">"Return home"</A>
        </section>
    }
}
