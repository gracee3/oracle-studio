//! Browser-side Oracle Studio shell and replaceable platform-service boundary.

use std::{future::Future, pin::Pin, sync::Arc};

use gloo_net::http::Request;
use leptos::{ev::SubmitEvent, html::Input, prelude::*};
use leptos_router::{
    components::{A, Route, Router, Routes},
    path,
};
use oracle_studio_protocol::{
    ApiError, ApiResponse, CatalogPlaceSummary, CatalogStatus, ChartSummary, ComparisonSummary,
    CreateVaultRequest, InstallCatalogRequest, LocationProvenanceInput, LocationSummary,
    MutationResult, PROTOCOL_VERSION, PersonSummary, ProtocolRequest, SaveLocationRequest,
    SearchCatalogRequest, SessionStatus, UnlockVaultRequest, VaultState, WorkspaceSummary,
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
    fn save_location(&self, request: SaveLocationRequest) -> PlatformFuture<'_, MutationResult>;
    fn catalog_status(&self) -> PlatformFuture<'_, CatalogStatus>;
    fn install_catalog(&self) -> PlatformFuture<'_, CatalogStatus>;
    fn search_catalog(
        &self,
        request: SearchCatalogRequest,
    ) -> PlatformFuture<'_, Vec<CatalogPlaceSummary>>;
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

    fn save_location(&self, request: SaveLocationRequest) -> PlatformFuture<'_, MutationResult> {
        Box::pin(self.post("/api/v1/locations/save", request))
    }

    fn catalog_status(&self) -> PlatformFuture<'_, CatalogStatus> {
        Box::pin(self.post("/api/v1/catalog/status", ProtocolRequest::current()))
    }

    fn install_catalog(&self) -> PlatformFuture<'_, CatalogStatus> {
        Box::pin(self.post("/api/v1/catalog/install", InstallCatalogRequest::current()))
    }

    fn search_catalog(
        &self,
        request: SearchCatalogRequest,
    ) -> PlatformFuture<'_, Vec<CatalogPlaceSummary>> {
        Box::pin(self.post("/api/v1/catalog/search", request))
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
    let context = expect_context::<StudioContext>();
    let catalog = RwSignal::new(None::<Result<CatalogStatus, PlatformError>>);
    let saved = RwSignal::new(None::<Result<Vec<LocationSummary>, PlatformError>>);
    let results = RwSignal::new(Vec::<CatalogPlaceSummary>::new());
    let feedback = RwSignal::new(None::<Result<String, String>>);
    let search_ref = NodeRef::<Input>::new();
    refresh_catalog_status(Arc::clone(&context.platform), catalog);
    refresh_saved_locations(Arc::clone(&context.platform), saved);

    let install = {
        let platform = Arc::clone(&context.platform);
        move |_| {
            feedback.set(Some(
                Ok("Downloading the public GeoNames files…".to_owned()),
            ));
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                match platform.install_catalog().await {
                    Ok(status) => {
                        catalog.set(Some(Ok(status)));
                        feedback.set(Some(Ok(
                            "The offline catalog is installed. Searches stay on this computer."
                                .to_owned(),
                        )));
                    }
                    Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
                }
            });
        }
    };
    let search = {
        let platform = Arc::clone(&context.platform);
        move |event: SubmitEvent| {
            event.prevent_default();
            let Some(input) = search_ref.get() else {
                return;
            };
            let query = input.value();
            if query.trim().is_empty() {
                feedback.set(Some(Err("Enter a city or place name.".to_owned())));
                return;
            }
            feedback.set(Some(Ok("Searching the local catalog…".to_owned())));
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                match platform
                    .search_catalog(SearchCatalogRequest::current(query, 20))
                    .await
                {
                    Ok(matches) => {
                        let count = matches.len();
                        results.set(matches);
                        feedback.set(Some(Ok(format!(
                            "Found {count} local catalog result{}.",
                            if count == 1 { "" } else { "s" }
                        ))));
                    }
                    Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
                }
            });
        }
    };

    view! {
        <PageHeader eyebrow="Offline location catalog" title="Locations" description="Saved places are encrypted snapshots. The optional GeoNames catalog remains outside the vault and never sends searches over the network." />
        <div class="location-layout">
            <section class="panel catalog-panel" aria-labelledby="catalog-title">
                <p class="eyebrow">"Optional public data"</p>
                <h2 id="catalog-title">"GeoNames cities500"</h2>
                {move || match catalog.get() {
                    None => view! { <p class="muted">"Checking the local catalog…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-text">{error.message().to_owned()}</p> }.into_any(),
                    Some(Ok(status)) if status.installed => view! {
                        <div class="catalog-status installed">
                            <span class="status-dot unlocked" aria-hidden="true"></span>
                            <strong>"Installed for offline search"</strong>
                            <span>{status.place_count.map(|count| format!("{count} places")).unwrap_or_default()}</span>
                            <code>{status.content_id.unwrap_or_default()}</code>
                        </div>
                    }.into_any(),
                    Some(Ok(_)) => view! {
                        <div class="catalog-status">
                            <span class="status-dot" aria-hidden="true"></span>
                            <strong>"Not installed"</strong>
                        </div>
                    }.into_any(),
                }}
                <p class="muted">
                    "Installation downloads cities500 and administrative-name files only after you press this button. The files stay outside your vault; later searches use only the local copy."
                </p>
                <button class="primary-button" type="button" on:click=install>
                    "Download and install catalog"
                </button>
                <p class="attribution">
                    "Contains GeoNames geographical data, available under "
                    <a href="https://creativecommons.org/licenses/by/4.0/" target="_blank" rel="noreferrer">"CC BY 4.0"</a>
                    ". Source: "
                    <a href="https://download.geonames.org/export/dump/" target="_blank" rel="noreferrer">"GeoNames distribution"</a>
                    "."
                </p>
            </section>

            <section class="panel catalog-panel" aria-labelledby="search-title">
                <p class="eyebrow">"Local-only lookup"</p>
                <h2 id="search-title">"Search places"</h2>
                <form class="inline-form" on:submit=search>
                    <label class="grow-field">
                        <span>"City or place name"</span>
                        <input node_ref=search_ref type="search" autocomplete="off" placeholder="Columbia" />
                    </label>
                    <button class="secondary-button" type="submit">"Search offline"</button>
                </form>
                <div class="catalog-results" aria-live="polite">
                    <For
                        each=move || results.get()
                        key=|place| place.geonames_id
                        children={
                            let platform = Arc::clone(&context.platform);
                            move |place| view! {
                                <CatalogPlaceCard
                                    place
                                    platform=Arc::clone(&platform)
                                    feedback
                                    saved
                                />
                            }
                        }
                    />
                </div>
            </section>
        </div>

        <div class="form-feedback location-feedback" role="status" aria-live="polite">
            {move || feedback.get().map(|result| match result {
                Ok(message) => view! { <span>{message}</span> }.into_any(),
                Err(message) => view! { <span class="error-text">{message}</span> }.into_any(),
            })}
        </div>

        <div class="location-layout location-layout-secondary">
            <ManualLocationForm
                platform=Arc::clone(&context.platform)
                feedback
                saved
            />
            <section class="panel catalog-panel" aria-labelledby="saved-locations-title">
                <p class="eyebrow">"Encrypted snapshots"</p>
                <h2 id="saved-locations-title">"Saved locations"</h2>
                {move || match saved.get() {
                    None => view! { <p class="muted">"Loading saved locations…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-text">{error.message().to_owned()}</p> }.into_any(),
                    Some(Ok(locations)) if locations.is_empty() => view! { <p class="muted">"No saved locations in the unlocked vault."</p> }.into_any(),
                    Some(Ok(locations)) => view! {
                        <ul class="saved-location-list">
                            {locations.into_iter().map(|location| view! {
                                <li>
                                    <strong>{location.label}</strong>
                                    <span>{format!("{} · {}", location.country_code, location.time_zone)}</span>
                                    <small>{format!("{:.4}, {:.4}", location.latitude_degrees, location.longitude_degrees)}</small>
                                </li>
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                }}
            </section>
        </div>
    }
}

fn refresh_catalog_status(
    platform: Arc<dyn StudioPlatform>,
    status: RwSignal<Option<Result<CatalogStatus, PlatformError>>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        status.set(Some(platform.catalog_status().await));
    });
}

fn refresh_saved_locations(
    platform: Arc<dyn StudioPlatform>,
    saved: RwSignal<Option<Result<Vec<LocationSummary>, PlatformError>>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        saved.set(Some(platform.locations().await));
    });
}

#[component]
fn CatalogPlaceCard(
    place: CatalogPlaceSummary,
    platform: Arc<dyn StudioPlatform>,
    feedback: RwSignal<Option<Result<String, String>>>,
    saved: RwSignal<Option<Result<Vec<LocationSummary>, PlatformError>>>,
) -> impl IntoView {
    let details = if place.administrative_names.is_empty() {
        place.country_code.clone()
    } else {
        format!(
            "{}, {}",
            place.administrative_names.join(", "),
            place.country_code
        )
    };
    let save = {
        let place = place.clone();
        move |_| {
            let request = SaveLocationRequest {
                protocol_version: PROTOCOL_VERSION,
                id: format!("geonames_{}", place.geonames_id),
                label: place.name.clone(),
                administrative_names: place.administrative_names.clone(),
                country_code: place.country_code.clone(),
                latitude_degrees: place.latitude_degrees,
                longitude_degrees: place.longitude_degrees,
                elevation_meters: place.elevation_meters,
                time_zone: place.time_zone.clone(),
                provenance: LocationProvenanceInput::GeoNames {
                    geonames_id: place.geonames_id,
                    catalog_content_id: place.catalog_content_id.clone(),
                },
            };
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                match platform.save_location(request).await {
                    Ok(_) => {
                        feedback.set(Some(Ok("Saved an encrypted location snapshot.".to_owned())));
                        refresh_saved_locations(Arc::clone(&platform), saved);
                    }
                    Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
                }
            });
        }
    };
    view! {
        <article class="catalog-result">
            <div>
                <h3>{place.name}</h3>
                <p>{details}</p>
                <small>{format!("{} · {:.4}, {:.4} · population {}", place.time_zone, place.latitude_degrees, place.longitude_degrees, place.population)}</small>
            </div>
            <button class="quiet-button" type="button" on:click=save>"Save snapshot"</button>
        </article>
    }
}

#[component]
fn ManualLocationForm(
    platform: Arc<dyn StudioPlatform>,
    feedback: RwSignal<Option<Result<String, String>>>,
    saved: RwSignal<Option<Result<Vec<LocationSummary>, PlatformError>>>,
) -> impl IntoView {
    let id_ref = NodeRef::<Input>::new();
    let label_ref = NodeRef::<Input>::new();
    let admin_ref = NodeRef::<Input>::new();
    let country_ref = NodeRef::<Input>::new();
    let latitude_ref = NodeRef::<Input>::new();
    let longitude_ref = NodeRef::<Input>::new();
    let elevation_ref = NodeRef::<Input>::new();
    let zone_ref = NodeRef::<Input>::new();
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let values = (
            id_ref.get().map(|input| input.value()),
            label_ref.get().map(|input| input.value()),
            country_ref.get().map(|input| input.value()),
            latitude_ref.get().map(|input| input.value()),
            longitude_ref.get().map(|input| input.value()),
            zone_ref.get().map(|input| input.value()),
        );
        let (Some(id), Some(label), Some(country), Some(latitude), Some(longitude), Some(zone)) =
            values
        else {
            return;
        };
        let latitude = match latitude.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                feedback.set(Some(Err("Latitude must be a number.".to_owned())));
                return;
            }
        };
        let longitude = match longitude.parse::<f64>() {
            Ok(value) => value,
            Err(_) => {
                feedback.set(Some(Err("Longitude must be a number.".to_owned())));
                return;
            }
        };
        let elevation = elevation_ref
            .get()
            .map(|input| input.value())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.parse::<f64>());
        let elevation = match elevation.transpose() {
            Ok(value) => value,
            Err(_) => {
                feedback.set(Some(Err("Elevation must be blank or a number.".to_owned())));
                return;
            }
        };
        let administrative_names = admin_ref
            .get()
            .map(|input| input.value())
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        let request = SaveLocationRequest {
            protocol_version: PROTOCOL_VERSION,
            id,
            label,
            administrative_names,
            country_code: country.trim().to_uppercase(),
            latitude_degrees: latitude,
            longitude_degrees: longitude,
            elevation_meters: elevation,
            time_zone: zone,
            provenance: LocationProvenanceInput::Manual,
        };
        let platform = Arc::clone(&platform);
        wasm_bindgen_futures::spawn_local(async move {
            match platform.save_location(request).await {
                Ok(_) => {
                    feedback.set(Some(Ok("Saved the manual location.".to_owned())));
                    refresh_saved_locations(Arc::clone(&platform), saved);
                }
                Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
            }
        });
    };
    view! {
        <form class="panel catalog-panel manual-location-form" on:submit=submit>
            <p class="eyebrow">"Always available"</p>
            <h2>"Manual location"</h2>
            <p class="muted">"Enter explicit coordinates and an IANA time zone when no catalog is installed or the place is absent."</p>
            <div class="form-grid">
                <label><span>"Record ID"</span><input node_ref=id_ref required type="text" placeholder="home_city" /></label>
                <label><span>"Label"</span><input node_ref=label_ref required type="text" placeholder="Home city" /></label>
                <label class="wide-field"><span>"Administrative names (comma-separated)"</span><input node_ref=admin_ref type="text" placeholder="County, State" /></label>
                <label><span>"Country code"</span><input node_ref=country_ref required maxlength="2" type="text" placeholder="US" /></label>
                <label><span>"IANA time zone"</span><input node_ref=zone_ref required type="text" placeholder="America/New_York" /></label>
                <label><span>"Latitude"</span><input node_ref=latitude_ref required inputmode="decimal" type="text" placeholder="38.9072" /></label>
                <label><span>"Longitude"</span><input node_ref=longitude_ref required inputmode="decimal" type="text" placeholder="-77.0369" /></label>
                <label><span>"Elevation meters (optional)"</span><input node_ref=elevation_ref inputmode="decimal" type="text" /></label>
            </div>
            <button class="secondary-button" type="submit">"Save manual location"</button>
        </form>
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
