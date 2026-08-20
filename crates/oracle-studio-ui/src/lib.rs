//! Open browser-local Oracle Studio presentation.

#[cfg(not(target_arch = "wasm32"))]
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::rc::Rc;

    use js_sys::{Array, Uint8Array};
    use leptos::{
        ev::SubmitEvent,
        html::{Input, Select, Textarea},
        prelude::*,
    };
    use oracle_studio_core::{
        AmbiguousTimeChoice, ChartCalculationOptions, ChartDefinition, ChartRole, ComparisonPreset,
        LocalDateTimeInput, LocalTimeResolution, LocationProvenance, PersonKind, SavedLocation,
        StableId, WheelOrientation, default_aspects, default_chart_points,
    };
    use oracle_studio_location_catalog::{
        CatalogInstallInput, CatalogRetrieval, CatalogSearchMatch,
    };
    use oracle_studio_platform::{
        ActiveWorkspace, CapabilityStatus, PlatformCommand, PlatformResponse, StudioPlatform,
        VaultLockState, VaultSummary, WorkspaceSummary,
    };
    use oracle_studio_worker::BrowserStudioPlatform;
    use wasm_bindgen::{JsCast, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{BeforeUnloadEvent, Blob, File, HtmlAnchorElement, Url};

    type Platform = StoredValue<Rc<BrowserStudioPlatform>, LocalStorage>;

    #[derive(Clone, Copy)]
    struct Model {
        vaults: RwSignal<Vec<VaultSummary>>,
        workspace: RwSignal<WorkspaceSummary>,
        capabilities: RwSignal<Option<CapabilityStatus>>,
        catalog_results: RwSignal<Vec<CatalogSearchMatch>>,
        notice: RwSignal<Option<String>>,
        problem: RwSignal<Option<String>>,
        busy: RwSignal<bool>,
    }

    impl Model {
        fn new() -> Self {
            Self {
                vaults: RwSignal::new(Vec::new()),
                workspace: RwSignal::new(WorkspaceSummary {
                    active: None,
                    scratch_dirty: false,
                    people: Vec::new(),
                    locations: Vec::new(),
                    charts: Vec::new(),
                    comparisons: Vec::new(),
                }),
                capabilities: RwSignal::new(None),
                catalog_results: RwSignal::new(Vec::new()),
                notice: RwSignal::new(None),
                problem: RwSignal::new(None),
                busy: RwSignal::new(false),
            }
        }
    }

    #[component]
    pub fn App() -> impl IntoView {
        let model = Model::new();
        let platform = StoredValue::new_local(Rc::new(BrowserStudioPlatform::spawn()));
        install_scratch_exit_warning(model.workspace);
        Effect::new(move |_| dispatch(platform, model, PlatformCommand::Initialize));

        view! {
            <a class="skip-link" href="#main-content">"Skip to workspace"</a>
            <header class="site-header">
                <a class="brand" href="#library" aria-label="Oracle Studio home">
                    <span class="brand-mark" aria-hidden="true">"☉"</span>
                    <span><strong>"Oracle Studio"</strong><small>"Browser-local chart work"</small></span>
                </a>
                <nav aria-label="Studio sections">
                    <a href="#library">"Vaults"</a>
                    <a href="#people">"People"</a>
                    <a href="#locations">"Locations"</a>
                    <a href="#charts">"Charts"</a>
                    <a href="#workspace">"Workspace"</a>
                </nav>
            </header>

            <main id="main-content" tabindex="-1">
                <section class="hero">
                    <div>
                        <p class="eyebrow">"Private by construction"</p>
                        <h1>"Your studio lives in this browser."</h1>
                        <p>"Start immediately in scratch, or unlock any number of encrypted portable vaults. No account, token, server, or filesystem path is required."</p>
                    </div>
                    <div class="hero-status">
                        <span class="status-dot" aria-hidden="true"></span>
                        <strong>"Local-only session"</strong>
                        <small>{move || active_label(&model.workspace.get())}</small>
                    </div>
                </section>

                <div class="announcements">
                    <p class="notice" aria-live="polite">{move || model.notice.get().unwrap_or_default()}</p>
                    <p class="problem" role="alert">{move || model.problem.get().unwrap_or_default()}</p>
                </div>

                <VaultLibrary platform model />

                {move || if model.workspace.get().active.is_some() {
                    view! {
                        <PeopleSection platform model />
                        <LocationsSection platform model />
                        <ChartsSection platform model />
                        <WorkspaceSection platform model />
                    }.into_any()
                } else {
                    view! {
                        <section class="empty-workspace panel">
                            <p class="eyebrow">"No active workspace"</p>
                            <h2>"Explore freely, save deliberately."</h2>
                            <p>"A new chart starts an in-memory scratch workspace. It is never written to disk until you choose a title and password."</p>
                            <button class="primary" disabled=move || model.busy.get() on:click=move |_| dispatch(platform, model, PlatformCommand::CreateScratch)>
                                "New chart in scratch"
                            </button>
                        </section>
                    }.into_any()
                }}
            </main>
            <footer>
                <span>"Oracle Studio · AGPL-3.0-or-later"</span>
                <span>"GeoNames catalog data: CC BY 4.0"</span>
            </footer>
        }
    }

    #[component]
    fn VaultLibrary(platform: Platform, model: Model) -> impl IntoView {
        let scratch_title = NodeRef::<Input>::new();
        let scratch_password = NodeRef::<Input>::new();
        let import = NodeRef::<Input>::new();
        let replace_duplicate = NodeRef::<Input>::new();
        let save_scratch = move |event: SubmitEvent| {
            event.prevent_default();
            let Some(title) = value(scratch_title) else {
                return;
            };
            let Some(password) = value(scratch_password) else {
                return;
            };
            if let Some(input) = scratch_password.get() {
                input.set_value("");
            }
            dispatch(
                platform,
                model,
                PlatformCommand::SaveScratch {
                    title,
                    password: password.into_bytes(),
                },
            );
        };
        let import_vault = move |_| {
            let Some(file) = import
                .get()
                .and_then(|input| input.files())
                .and_then(|files| files.item(0))
            else {
                return;
            };
            let replace_confirmed = replace_duplicate.get().is_some_and(|input| input.checked());
            spawn_local(async move {
                match read_file(file).await {
                    Ok(bytes) => dispatch(
                        platform,
                        model,
                        PlatformCommand::ImportVault {
                            bytes,
                            replace_confirmed,
                        },
                    ),
                    Err(message) => model.problem.set(Some(message)),
                }
            });
        };

        view! {
            <section id="library" class="panel library">
                <div class="section-heading">
                    <div><p class="eyebrow">"Vault library"</p><h2>"Portable studios"</h2></div>
                    <div class="import-actions">
                        <label class="file-action">"Import .oracle-vault"<input node_ref=import type="file" accept=".oracle-vault,application/octet-stream" on:change=import_vault /></label>
                        <label class="confirm-replace"><input node_ref=replace_duplicate type="checkbox" />"Replace an existing vault with the same ID"</label>
                    </div>
                </div>
                <div class="storage-warning">
                    <strong>"Exports are your backups."</strong>
                    <span>{move || model.capabilities.get().map(|status| status.backup_warning).unwrap_or_else(|| "Browser storage can be evicted.".into())}</span>
                </div>
                <div class="vault-grid">
                    <For
                        each=move || model.vaults.get()
                        key=|vault| format!("{}:{}:{:?}", vault.id, vault.revision, vault.lock_state)
                        children=move |vault| view! { <VaultCard platform model vault /> }
                    />
                    {move || if model.vaults.get().is_empty() {
                        view! { <div class="empty-card"><span aria-hidden="true">"◇"</span><p>"No encrypted vaults in this browser yet."</p></div> }.into_any()
                    } else { ().into_any() }}
                </div>
                <div class="scratch-row">
                    <div>
                        <strong>"Scratch workspace"</strong>
                        <p>{move || if model.workspace.get().scratch_dirty { "Unsaved changes — save or explicitly discard before leaving." } else { "Memory-only work for quick exploration." }}</p>
                    </div>
                    {move || if model.workspace.get().active == Some(ActiveWorkspace::Scratch) {
                        view! {
                            <form class="inline-form" on:submit=save_scratch>
                                <label><span>"Public vault title"</span><input node_ref=scratch_title required maxlength="256" autocomplete="off" /></label>
                                <label><span>"Vault password"</span><input node_ref=scratch_password type="password" required autocomplete="new-password" /></label>
                                <button class="primary" disabled=move || model.busy.get() type="submit">"Save encrypted vault"</button>
                                <button type="button" on:click=move |_| {
                                    let confirmed = !model.workspace.get().scratch_dirty || web_sys::window().and_then(|window| window.confirm_with_message("Discard unsaved scratch work?").ok()).unwrap_or(false);
                                    dispatch(platform, model, PlatformCommand::DiscardScratch { confirmed });
                                }>"Discard"</button>
                            </form>
                        }.into_any()
                    } else {
                        view! { <button on:click=move |_| dispatch(platform, model, PlatformCommand::CreateScratch)>"Open scratch"</button> }.into_any()
                    }}
                </div>
            </section>
        }
    }

    #[component]
    fn VaultCard(platform: Platform, model: Model, vault: VaultSummary) -> impl IntoView {
        let password = NodeRef::<Input>::new();
        let id = vault.id.clone();
        let unlock_id = id.clone();
        let export_id = id.clone();
        let lock_id = id.clone();
        let remove_id = id.clone();
        let activate_id = id.clone();
        let state = vault.lock_state.clone();
        let unlock = move |event: SubmitEvent| {
            event.prevent_default();
            let Some(password_value) = value(password) else {
                return;
            };
            if let Some(input) = password.get() {
                input.set_value("");
            }
            dispatch(
                platform,
                model,
                PlatformCommand::UnlockVault {
                    vault_id: unlock_id.clone(),
                    password: password_value.into_bytes(),
                },
            );
        };
        view! {
            <article class="vault-card">
                <div class="vault-card-top"><span class="vault-icon" aria-hidden="true">"◈"</span><span class=format!("badge {:?}", state)>{format!("{:?}", state)}</span></div>
                <h3>{vault.title}</h3>
                <p class="vault-id">{id}</p>
                <small>{format!("Updated {}", vault.modified_at)}</small>
                {if state == VaultLockState::Locked {
                    view! {
                        <form class="unlock-form" on:submit=unlock>
                            <label><span>"Password"</span><input node_ref=password type="password" required autocomplete="current-password" /></label>
                            <button class="primary" type="submit">"Unlock"</button>
                        </form>
                    }.into_any()
                } else {
                    view! {
                        <div class="button-row">
                            <button on:click=move |_| dispatch(platform, model, PlatformCommand::ActivateVault { vault_id: activate_id.clone() })>"Switch here"</button>
                            <button on:click=move |_| dispatch(platform, model, PlatformCommand::LockVault { vault_id: lock_id.clone() })>"Lock"</button>
                        </div>
                    }.into_any()
                }}
                <div class="button-row secondary-actions">
                    <button on:click=move |_| dispatch(platform, model, PlatformCommand::ExportVault { vault_id: export_id.clone() })>"Export"</button>
                    <button class="danger" on:click=move |_| {
                        let confirmed = web_sys::window().and_then(|window| window.confirm_with_message("Remove this browser copy? Exported backups are unaffected.").ok()).unwrap_or(false);
                        dispatch(platform, model, PlatformCommand::RemoveVault { vault_id: remove_id.clone(), confirmed });
                    }>"Remove"</button>
                </div>
            </article>
        }
    }

    #[component]
    fn PeopleSection(platform: Platform, model: Model) -> impl IntoView {
        let id = NodeRef::<Input>::new();
        let name = NodeRef::<Input>::new();
        let notes = NodeRef::<Textarea>::new();
        let submit = move |event: SubmitEvent| {
            event.prevent_default();
            let (Some(id_value), Some(display_name)) = (value(id), value(name)) else {
                return;
            };
            match StableId::new("person.id", id_value) {
                Ok(id) => dispatch(
                    platform,
                    model,
                    PlatformCommand::AddPerson {
                        id,
                        display_name,
                        kind: PersonKind::Personal,
                        notes: text_value(notes).filter(|value| !value.trim().is_empty()),
                    },
                ),
                Err(error) => model.problem.set(Some(error.to_string())),
            }
        };
        view! {
            <section id="people" class="panel split-panel">
                <div><p class="eyebrow">"People"</p><h2>"Chart subjects"</h2><ul class="entity-list"><For each=move || model.workspace.get().people key=|item| item.id.clone() children=|item| view! { <li><strong>{item.label}</strong><small>{item.id}</small></li> } /></ul></div>
                <form class="studio-form" on:submit=submit>
                    <h3>"Add or update a person"</h3>
                    <label><span>"Record ID"</span><input node_ref=id required /></label>
                    <label><span>"Display name"</span><input node_ref=name required /></label>
                    <label><span>"Notes (optional)"</span><textarea node_ref=notes></textarea></label>
                    <button class="primary" type="submit">"Save person"</button>
                </form>
            </section>
        }
    }

    #[component]
    fn LocationsSection(platform: Platform, model: Model) -> impl IntoView {
        let id = NodeRef::<Input>::new();
        let label = NodeRef::<Input>::new();
        let country = NodeRef::<Input>::new();
        let zone = NodeRef::<Input>::new();
        let latitude = NodeRef::<Input>::new();
        let longitude = NodeRef::<Input>::new();
        let cities = NodeRef::<Input>::new();
        let admin1 = NodeRef::<Input>::new();
        let admin2 = NodeRef::<Input>::new();
        let query = NodeRef::<Input>::new();
        let save = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                SavedLocation::new(
                    StableId::new(
                        "saved_location.id",
                        value(id).ok_or("record ID is required")?,
                    )
                    .map_err(|error| error.to_string())?,
                    value(label).ok_or("label is required")?,
                    Vec::new(),
                    value(country)
                        .ok_or("country is required")?
                        .to_ascii_uppercase(),
                    value(latitude)
                        .ok_or("latitude is required")?
                        .parse::<f64>()
                        .map_err(|_| "invalid latitude")?,
                    value(longitude)
                        .ok_or("longitude is required")?
                        .parse::<f64>()
                        .map_err(|_| "invalid longitude")?,
                    None,
                    value(zone).ok_or("time zone is required")?,
                    LocationProvenance::Manual,
                )
                .map_err(|error| error.to_string())
            })();
            match result {
                Ok(location) => {
                    dispatch(platform, model, PlatformCommand::SaveLocation { location })
                }
                Err(message) => model.problem.set(Some(message.to_string())),
            }
        };
        let install = move |event: SubmitEvent| {
            event.prevent_default();
            let files = [cities, admin1, admin2].map(|input| {
                input
                    .get()
                    .and_then(|element| element.files())
                    .and_then(|files| files.item(0))
            });
            let [Some(cities_file), Some(admin1_file), Some(admin2_file)] = files else {
                model
                    .problem
                    .set(Some("Choose all three GeoNames files.".into()));
                return;
            };
            spawn_local(async move {
                let result = async {
                    Ok::<_, String>(CatalogInstallInput {
                        cities500_zip: read_file(cities_file).await?,
                        admin1_codes: read_file(admin1_file).await?,
                        admin2_codes: read_file(admin2_file).await?,
                        retrieved_at: canonical_now(),
                        retrieval: CatalogRetrieval::LocalFiles,
                    })
                }
                .await;
                match result {
                    Ok(input) => {
                        dispatch(platform, model, PlatformCommand::InstallCatalog { input })
                    }
                    Err(message) => model.problem.set(Some(message)),
                }
            });
        };
        let search = move |event: SubmitEvent| {
            event.prevent_default();
            if let Some(query) = value(query) {
                dispatch(
                    platform,
                    model,
                    PlatformCommand::SearchCatalog { query, limit: 20 },
                );
            }
        };
        view! {
            <section id="locations" class="panel">
                <div class="section-heading"><div><p class="eyebrow">"Locations"</p><h2>"Encrypted snapshots, local catalog"</h2></div><span class="attribution">"GeoNames · CC BY 4.0"</span></div>
                <div class="two-column">
                    <form class="studio-form" on:submit=save>
                        <h3>"Manual location"</h3>
                        <label><span>"Record ID"</span><input node_ref=id required /></label>
                        <label><span>"Label"</span><input node_ref=label required /></label>
                        <div class="field-row"><label><span>"Country code"</span><input node_ref=country required maxlength="2" value="US" /></label><label><span>"IANA time zone"</span><input node_ref=zone required value="America/New_York" /></label></div>
                        <div class="field-row"><label><span>"Latitude"</span><input node_ref=latitude required inputmode="decimal" /></label><label><span>"Longitude"</span><input node_ref=longitude required inputmode="decimal" /></label></div>
                        <button class="primary" type="submit">"Save encrypted snapshot"</button>
                    </form>
                    <form class="studio-form catalog-form" on:submit=install>
                        <h3>"Install GeoNames files"</h3>
                        <p>"Parsing and search run in the worker. Source bytes and hashes stay in IndexedDB, outside encrypted vaults."</p>
                        <button type="button" on:click=move |_| dispatch(platform, model, PlatformCommand::InstallPinnedCatalog)>"Install image-pinned catalog"</button>
                        <span class="or-divider">"or choose local files"</span>
                        <label><span>"cities500.zip"</span><input node_ref=cities type="file" required accept=".zip" /></label>
                        <label><span>"admin1CodesASCII.txt"</span><input node_ref=admin1 type="file" required accept=".txt,text/plain" /></label>
                        <label><span>"admin2Codes.txt"</span><input node_ref=admin2 type="file" required accept=".txt,text/plain" /></label>
                        <button type="submit">"Install local catalog"</button>
                        <small>{move || model.capabilities.get().and_then(|status| status.catalog).map(|catalog| format!("Active: {} places · {}", catalog.place_count, catalog.content_id)).unwrap_or_else(|| "No catalog installed. Manual entry remains available.".into())}</small>
                    </form>
                </div>
                <form class="catalog-search" on:submit=search>
                    <label><span>"Search the active catalog"</span><input node_ref=query required autocomplete="off" /></label>
                    <button type="submit">"Search locally"</button>
                </form>
                <ul class="entity-list cards catalog-results">
                    <For
                        each=move || model.catalog_results.get()
                        key=|result| result.place().geonames_id()
                        children=|result| view! {
                            <li>
                                <strong>{result.place().name().to_owned()}</strong>
                                <small>{format!("{} · {} · {:?}", result.place().country_code(), result.place().time_zone(), result.match_kind())}</small>
                            </li>
                        }
                    />
                </ul>
                <ul class="entity-list cards"><For each=move || model.workspace.get().locations key=|item| item.id.clone() children=|item| view! { <li><strong>{item.label}</strong><small>{item.id}</small></li> } /></ul>
            </section>
        }
    }

    #[component]
    fn ChartsSection(platform: Platform, model: Model) -> impl IntoView {
        let id = NodeRef::<Input>::new();
        let label = NodeRef::<Input>::new();
        let role = NodeRef::<Select>::new();
        let date = NodeRef::<Input>::new();
        let time = NodeRef::<Input>::new();
        let zone = NodeRef::<Input>::new();
        let time_choice = NodeRef::<Select>::new();
        let save = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                let role = match select_value(role).as_deref() {
                    Some("natal") => ChartRole::Natal,
                    Some("event") => ChartRole::Event,
                    _ => ChartRole::Transit,
                };
                ChartDefinition::new(
                    StableId::new("chart.id", value(id).ok_or("chart ID is required")?)
                        .map_err(|error| error.to_string())?,
                    value(label).ok_or("chart label is required")?,
                    role,
                    None,
                    LocalDateTimeInput::new(
                        value(date).ok_or("date is required")?,
                        value(time).ok_or("time is required")?,
                        value(zone).ok_or("zone is required")?,
                    )
                    .map_err(|error| error.to_string())?,
                    ChartCalculationOptions::default(),
                    default_chart_points(),
                    false,
                )
                .map_err(|error| error.to_string())
            })();
            match result {
                Ok(chart) => dispatch(platform, model, PlatformCommand::SaveChart { chart }),
                Err(message) => model.problem.set(Some(message.to_string())),
            }
        };
        let resolve = move |_| {
            let result = (|| {
                let input = LocalDateTimeInput::new(
                    value(date).ok_or("date is required")?,
                    value(time).ok_or("time is required")?,
                    value(zone).ok_or("zone is required")?,
                )
                .map_err(|error| error.to_string())?;
                let choice = match select_value(time_choice).as_deref() {
                    Some("earlier") => Some(AmbiguousTimeChoice::Earlier),
                    Some("later") => Some(AmbiguousTimeChoice::Later),
                    _ => None,
                };
                Ok::<_, String>((input, choice))
            })();
            match result {
                Ok((input, choice)) => dispatch(
                    platform,
                    model,
                    PlatformCommand::ResolveLocalTime { input, choice },
                ),
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <section id="charts" class="panel split-panel">
                <div><p class="eyebrow">"Charts"</p><h2>"Exact local-time definitions"</h2><ul class="entity-list"><For each=move || model.workspace.get().charts key=|chart| chart.id.clone() children=|chart| view! { <li><strong>{format!("{} · {}", chart.label, chart.role)}</strong><small>{chart.local_input}</small></li> } /></ul></div>
                <form class="studio-form chart-editor" on:submit=save>
                    <h3>"New chart"</h3>
                    <label><span>"Chart ID"</span><input node_ref=id required /></label>
                    <label><span>"Chart label"</span><input node_ref=label required /></label>
                    <label><span>"Role"</span><select node_ref=role><option value="natal">"Natal"</option><option value="transit">"Transit"</option><option value="event">"Event"</option></select></label>
                    <div class="field-row"><label><span>"Local date"</span><input node_ref=date type="date" required /></label><label><span>"Local time"</span><input node_ref=time type="time" step="1" required /></label></div>
                    <label><span>"IANA time zone"</span><input node_ref=zone required value="America/New_York" /></label>
                    <label><span>"Ambiguous clock time"</span><select node_ref=time_choice><option value="">"Show both choices"</option><option value="earlier">"Use earlier instant"</option><option value="later">"Use later instant"</option></select></label>
                    <button type="button" on:click=resolve>"Check DST resolution"</button>
                    <button class="primary" type="submit">"Save chart definition"</button>
                    <p class="provider-note" role="status">{move || match model.capabilities.get().map(|status| status.ephemeris) { Some(oracle_studio_platform::EphemerisStatus::Unavailable) | None => "Ephemeris provider unavailable in this production build. Definitions and DST resolution remain available; results are never fabricated.", Some(_) => "Deterministic acceptance provider enabled." }}</p>
                </form>
            </section>
        }
    }

    #[component]
    fn WorkspaceSection(platform: Platform, model: Model) -> impl IntoView {
        let id = NodeRef::<Input>::new();
        let label = NodeRef::<Input>::new();
        let inner = NodeRef::<Input>::new();
        let outer = NodeRef::<Input>::new();
        let orientation = NodeRef::<Select>::new();
        let save = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                ComparisonPreset::new(
                    StableId::new(
                        "comparison.id",
                        value(id).ok_or("comparison ID is required")?,
                    )
                    .map_err(|error| error.to_string())?,
                    value(label).ok_or("label is required")?,
                    StableId::new(
                        "comparison.inner_chart_definition_id",
                        value(inner).ok_or("inner chart ID is required")?,
                    )
                    .map_err(|error| error.to_string())?,
                    StableId::new(
                        "comparison.outer_chart_definition_id",
                        value(outer).ok_or("outer chart ID is required")?,
                    )
                    .map_err(|error| error.to_string())?,
                    default_chart_points(),
                    default_chart_points(),
                    default_aspects(),
                    if select_value(orientation).as_deref() == Some("aries-top") {
                        WheelOrientation::AriesTop
                    } else {
                        WheelOrientation::AscendantLeft
                    },
                )
                .map_err(|error| error.to_string())
            })();
            match result {
                Ok(preset) => dispatch(platform, model, PlatformCommand::SaveComparison { preset }),
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <section id="workspace" class="panel workspace-panel">
                <p class="eyebrow">"Workspace"</p><h2>"Comparison canvas"</h2>
                <div class="chart-stage" aria-label="Transit biwheel workspace">
                    <div class="orbit-placeholder" aria-hidden="true"><span>"☉"</span></div>
                    <div><h3>"Ready for an in-worker ephemeris provider"</h3><p>"The Rust SVG biwheel renderer and animated HTML player remain available to browser presentations. This build intentionally has no production ephemeris implementation."</p><p>{move || format!("{} chart definitions · {} comparison presets", model.workspace.get().charts.len(), model.workspace.get().comparisons.len())}</p></div>
                </div>
                <form class="studio-form comparison-builder" on:submit=save>
                    <h3>"Comparison preset"</h3>
                    <p>"Presets reference chart definitions; immutable comparison snapshots are created only when both charts have provider-backed calculations."</p>
                    <label><span>"Preset ID"</span><input node_ref=id required /></label>
                    <label><span>"Preset label"</span><input node_ref=label required /></label>
                    <div class="field-row"><label><span>"Inner chart ID"</span><input node_ref=inner required /></label><label><span>"Outer chart ID"</span><input node_ref=outer required /></label></div>
                    <label><span>"Wheel orientation"</span><select node_ref=orientation><option value="ascendant-left">"Ascendant left"</option><option value="aries-top">"Aries top"</option></select></label>
                    <button type="submit">"Save comparison preset"</button>
                </form>
                <ul class="entity-list cards"><For each=move || model.workspace.get().comparisons key=|item| item.id.clone() children=|item| view! { <li><strong>{item.label}</strong><small>{item.id}</small></li> } /></ul>
            </section>
        }
    }

    fn dispatch(platform: Platform, model: Model, command: PlatformCommand) {
        model.busy.set(true);
        model.problem.set(None);
        let future = platform.with_value(|platform| platform.execute(command));
        spawn_local(async move {
            match future.await {
                Ok(response) => apply_response(model, response),
                Err(error) => model.problem.set(Some(error.message)),
            }
            model.busy.set(false);
        });
    }

    fn apply_response(model: Model, response: PlatformResponse) {
        match response {
            PlatformResponse::Ready {
                vaults,
                workspace,
                capabilities,
            } => {
                model.vaults.set(vaults);
                model.workspace.set(workspace);
                model.capabilities.set(Some(capabilities));
                model.notice.set(Some("Browser-local studio ready.".into()));
            }
            PlatformResponse::Vaults(vaults) => model.vaults.set(vaults),
            PlatformResponse::Workspace(workspace) => model.workspace.set(workspace),
            PlatformResponse::Updated { vaults, workspace } => {
                model.vaults.set(vaults);
                model.workspace.set(workspace);
                model.notice.set(Some("Local workspace updated.".into()));
            }
            PlatformResponse::Export { filename, bytes } => match download(&filename, &bytes) {
                Ok(()) => model.notice.set(Some(format!("Downloaded {filename}."))),
                Err(message) => model.problem.set(Some(message)),
            },
            PlatformResponse::LocalTime(resolution) => {
                model.notice.set(Some(format_local_time(&resolution)))
            }
            PlatformResponse::CatalogInstalled(metadata) => {
                model.capabilities.update(|status| {
                    if let Some(status) = status {
                        status.catalog = Some(metadata.clone());
                    }
                });
                model.notice.set(Some(format!(
                    "Installed {} GeoNames places.",
                    metadata.place_count
                )));
            }
            PlatformResponse::CatalogResults(results) => {
                model
                    .notice
                    .set(Some(format!("Found {} local matches.", results.len())));
                model.catalog_results.set(results);
            }
        }
    }

    fn install_scratch_exit_warning(workspace: RwSignal<WorkspaceSummary>) {
        let closure =
            Closure::<dyn FnMut(BeforeUnloadEvent)>::new(move |event: BeforeUnloadEvent| {
                if workspace.get_untracked().scratch_dirty {
                    event.prevent_default();
                    event.set_return_value("Unsaved scratch work will be lost.");
                }
            });
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("beforeunload", closure.as_ref().unchecked_ref());
            closure.forget();
        }
    }

    async fn read_file(file: File) -> Result<Vec<u8>, String> {
        let buffer = JsFuture::from(file.array_buffer())
            .await
            .map_err(|_| format!("Could not read {}.", file.name()))?;
        Ok(Uint8Array::new(&buffer).to_vec())
    }

    fn download(filename: &str, bytes: &[u8]) -> Result<(), String> {
        let parts = Array::new();
        parts.push(&Uint8Array::from(bytes));
        let blob = Blob::new_with_u8_array_sequence(&parts)
            .map_err(|_| "Could not create the export download.".to_string())?;
        let url = Url::create_object_url_with_blob(&blob)
            .map_err(|_| "Could not create the export URL.".to_string())?;
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or("Document unavailable.")?;
        let anchor: HtmlAnchorElement = document
            .create_element("a")
            .map_err(|_| "Could not create download link.")?
            .unchecked_into();
        anchor.set_href(&url);
        anchor.set_download(filename);
        anchor.click();
        Url::revoke_object_url(&url).map_err(|_| "Could not release the export URL.".to_string())
    }

    fn active_label(workspace: &WorkspaceSummary) -> String {
        match workspace.active.as_ref() {
            Some(ActiveWorkspace::Scratch) => if workspace.scratch_dirty {
                "Scratch · unsaved"
            } else {
                "Scratch · clean"
            }
            .into(),
            Some(ActiveWorkspace::Vault(id)) => {
                format!("Vault {}… mounted", &id[..id.len().min(8)])
            }
            None => "No active workspace".into(),
        }
    }

    fn value(node: NodeRef<Input>) -> Option<String> {
        node.get()
            .map(|input| input.value())
            .filter(|value| !value.is_empty())
    }
    fn text_value(node: NodeRef<Textarea>) -> Option<String> {
        node.get().map(|input| input.value())
    }
    fn select_value(node: NodeRef<Select>) -> Option<String> {
        node.get().map(|input| input.value())
    }

    fn format_local_time(resolution: &LocalTimeResolution) -> String {
        match resolution {
            LocalTimeResolution::Unique(value) => format!(
                "Unique local time: {} · {} · {}",
                value.utc_instant(),
                value.abbreviation(),
                value.utc_offset_display()
            ),
            LocalTimeResolution::Ambiguous { earlier, later } => format!(
                "Ambiguous local time: earlier {} {} {}; later {} {} {}",
                earlier.utc_instant(),
                earlier.abbreviation(),
                earlier.utc_offset_display(),
                later.utc_instant(),
                later.abbreviation(),
                later.utc_offset_display()
            ),
            LocalTimeResolution::Nonexistent => {
                "That local clock time does not exist because of a daylight-saving transition."
                    .into()
            }
        }
    }

    fn canonical_now() -> String {
        let iso = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".into());
        iso.split_once('.')
            .map_or(iso.clone(), |(seconds, _)| format!("{seconds}Z"))
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::App;

#[cfg(not(target_arch = "wasm32"))]
#[leptos::component]
pub fn App() -> impl leptos::IntoView {
    leptos::view! { <main><h1>"Oracle Studio"</h1><p>"Build this application for wasm32-unknown-unknown."</p></main> }
}
