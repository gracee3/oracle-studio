//! Browser-local Oracle Studio workbench presentation.

#[cfg(not(target_arch = "wasm32"))]
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

    use js_sys::{Array, Uint8Array};
    use leptos::{
        ev::{KeyboardEvent, PointerEvent, SubmitEvent},
        html::{Input, Select, Textarea},
        prelude::*,
    };
    use oracle_studio_chart_view::{RenderOptions, filtered_scene, render_biwheel_svg};
    use oracle_studio_core::{
        AmbiguousTimeChoice, AyanamsaId, ChartCalculationOptions, ChartDefinition, ChartRole,
        ComparisonPreset, HouseSystemId, LocalDateTimeInput, LocalTimeResolution,
        LocationProvenance, PersonKind, PreviewCoordinator, PreviewEnqueue, SavedLocation,
        StableId, StepDirection, TimeInterval, WheelOrientation as ComparisonWheelOrientation,
        ZodiacId, default_aspects, default_chart_points, generate_unique_id, step_local_time,
    };
    use oracle_studio_location_catalog::CatalogSearchMatch;
    use oracle_studio_platform::{
        ActiveWorkspace, CapabilityStatus, ChartSummary, LabelDensity, PlatformCommand,
        PlatformResponse, PreviewGeneration, PreviewSaveMode, StudioPlatform, VaultLockState,
        VaultSummary, WheelOrientation, WheelPalette, WheelTemplate, WheelTemplateSettings,
        WorkbenchPresentation, WorkbenchPreviewRequest, WorkspaceSummary,
    };
    use oracle_studio_worker::BrowserStudioPlatform;
    use wasm_bindgen::{JsCast, closure::Closure};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{BeforeUnloadEvent, Blob, Element, File, HtmlAnchorElement, Url};

    type Platform = StoredValue<Rc<BrowserStudioPlatform>, LocalStorage>;

    const POINT_FILTERS: [(&str, &str); 19] = [
        ("Sun", "Sun"),
        ("Moon", "Moon"),
        ("Mercury", "Mercury"),
        ("Venus", "Venus"),
        ("Mars", "Mars"),
        ("Jupiter", "Jupiter"),
        ("Saturn", "Saturn"),
        ("Uranus", "Uranus"),
        ("Neptune", "Neptune"),
        ("Pluto", "Pluto"),
        ("MeanNode", "Mean north node"),
        ("MeanSouthNode", "Mean south node"),
        ("TrueNode", "True north node"),
        ("TrueSouthNode", "True south node"),
        ("Ascendant", "Ascendant"),
        ("Midheaven", "Midheaven"),
        ("Descendant", "Descendant"),
        ("ImumCoeli", "IC"),
        ("Vertex", "Vertex"),
    ];
    const ASPECT_FILTERS: [(&str, &str); 5] = [
        ("Conjunction", "Conjunction"),
        ("Opposition", "Opposition"),
        ("Trine", "Trine"),
        ("Square", "Square"),
        ("Sextile", "Sextile"),
    ];

    #[derive(Clone)]
    struct PreviewPayload {
        inner_chart_definition_id: StableId,
        outer_chart_definition_id: StableId,
        inner_saved_location_id: StableId,
        outer_saved_location_id: StableId,
        outer_local_input: LocalDateTimeInput,
        outer_ambiguous_time_choice: Option<AmbiguousTimeChoice>,
        adjustment_notice: Option<String>,
    }

    #[derive(Clone, Default)]
    struct HoldController(Rc<RefCell<HoldTimers>>);

    #[derive(Default)]
    struct HoldTimers {
        timeout_id: Option<i32>,
        interval_id: Option<i32>,
        timeout_callback: Option<Closure<dyn FnMut()>>,
        interval_callback: Option<Closure<dyn FnMut()>>,
    }

    impl HoldController {
        fn start(&self, action: Rc<dyn Fn()>) {
            self.cancel();
            action();
            let controller = self.clone();
            let callback = Closure::<dyn FnMut()>::new(move || {
                let repeated_action = action.clone();
                let repeated = Closure::<dyn FnMut()>::new(move || repeated_action());
                if let Some(window) = web_sys::window()
                    && let Ok(interval_id) = window
                        .set_interval_with_callback_and_timeout_and_arguments_0(
                            repeated.as_ref().unchecked_ref(),
                            120,
                        )
                {
                    let mut timers = controller.0.borrow_mut();
                    timers.interval_id = Some(interval_id);
                    timers.interval_callback = Some(repeated);
                }
            });
            if let Some(window) = web_sys::window()
                && let Ok(timeout_id) = window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.as_ref().unchecked_ref(),
                        350,
                    )
            {
                let mut timers = self.0.borrow_mut();
                timers.timeout_id = Some(timeout_id);
                timers.timeout_callback = Some(callback);
            }
        }

        fn cancel(&self) {
            let (timeout_id, interval_id, timeout_callback, interval_callback) = {
                let mut timers = self.0.borrow_mut();
                (
                    timers.timeout_id.take(),
                    timers.interval_id.take(),
                    timers.timeout_callback.take(),
                    timers.interval_callback.take(),
                )
            };
            if let Some(window) = web_sys::window() {
                if let Some(id) = timeout_id {
                    window.clear_timeout_with_handle(id);
                }
                if let Some(id) = interval_id {
                    window.clear_interval_with_handle(id);
                }
            }
            drop((timeout_callback, interval_callback));
        }
    }

    #[derive(Clone, Copy)]
    struct Model {
        vaults: RwSignal<Vec<VaultSummary>>,
        workspace: RwSignal<WorkspaceSummary>,
        capabilities: RwSignal<Option<CapabilityStatus>>,
        wheel_templates: RwSignal<WheelTemplateSettings>,
        catalog_results: RwSignal<Vec<CatalogSearchMatch>>,
        presentation: RwSignal<Option<WorkbenchPresentation>>,
        fallback_presentation: RwSignal<Option<WorkbenchPresentation>>,
        desired_outer: RwSignal<Option<LocalDateTimeInput>>,
        outer_ambiguous_choice: RwSignal<Option<AmbiguousTimeChoice>>,
        inner_chart_id: RwSignal<String>,
        outer_chart_id: RwSignal<String>,
        inner_location_id: RwSignal<String>,
        outer_location_id: RwSignal<String>,
        visible_points: RwSignal<BTreeSet<String>>,
        visible_aspects: RwSignal<BTreeSet<String>>,
        selected_points: RwSignal<BTreeSet<String>>,
        selected_aspects: RwSignal<BTreeSet<String>>,
        latest_generation: RwSignal<u64>,
        calculating: RwSignal<bool>,
        notice: RwSignal<Option<String>>,
        problem: RwSignal<Option<String>>,
        busy: RwSignal<bool>,
        left_open: RwSignal<bool>,
        right_open: RwSignal<bool>,
        coordinator: StoredValue<Rc<RefCell<PreviewCoordinator<PreviewPayload>>>, LocalStorage>,
        holds: StoredValue<HoldController, LocalStorage>,
    }

    impl Model {
        fn new() -> Self {
            Self {
                vaults: RwSignal::new(Vec::new()),
                workspace: RwSignal::new(empty_workspace()),
                capabilities: RwSignal::new(None),
                wheel_templates: RwSignal::new(WheelTemplateSettings::default()),
                catalog_results: RwSignal::new(Vec::new()),
                presentation: RwSignal::new(None),
                fallback_presentation: RwSignal::new(None),
                desired_outer: RwSignal::new(None),
                outer_ambiguous_choice: RwSignal::new(None),
                inner_chart_id: RwSignal::new(String::new()),
                outer_chart_id: RwSignal::new(String::new()),
                inner_location_id: RwSignal::new(String::new()),
                outer_location_id: RwSignal::new(String::new()),
                visible_points: RwSignal::new(
                    POINT_FILTERS.iter().map(|(id, _)| (*id).into()).collect(),
                ),
                visible_aspects: RwSignal::new(
                    ASPECT_FILTERS.iter().map(|(id, _)| (*id).into()).collect(),
                ),
                selected_points: RwSignal::new(BTreeSet::new()),
                selected_aspects: RwSignal::new(BTreeSet::new()),
                latest_generation: RwSignal::new(0),
                calculating: RwSignal::new(false),
                notice: RwSignal::new(None),
                problem: RwSignal::new(None),
                busy: RwSignal::new(false),
                left_open: RwSignal::new(false),
                right_open: RwSignal::new(false),
                coordinator: StoredValue::new_local(Rc::new(RefCell::new(
                    PreviewCoordinator::default(),
                ))),
                holds: StoredValue::new_local(HoldController::default()),
            }
        }

        fn stop_hold(self) {
            self.holds.with_value(HoldController::cancel);
        }
    }

    #[component]
    pub fn App() -> impl IntoView {
        let model = Model::new();
        let platform = StoredValue::new_local(Rc::new(BrowserStudioPlatform::spawn()));
        install_lifecycle_guards(model);
        Effect::new(move |_| dispatch(platform, model, PlatformCommand::Initialize));

        view! {
            <a class="skip-link" href="#wheel-stage">"Skip to chart wheel"</a>
            <div class="studio-shell">
                <header class="app-header">
                    <a class="brand" href="#workbench" aria-label="Oracle Studio workbench">
                        <span class="brand-mark" aria-hidden="true">"☉"</span>
                        <span><strong>"Oracle Studio"</strong><small>"Moshier workbench"</small></span>
                    </a>
                    <nav aria-label="Application views">
                        <a href="#workbench">"Workbench"</a>
                        <a href="#settings">"Settings"</a>
                        <a href="#files">"Files"</a>
                    </nav>
                    <div class="header-actions">
                        <button class="sidebar-toggle charts-toggle" type="button" aria-label="Toggle charts and wheels" aria-expanded=move || model.left_open.get() on:click=move |_| { model.right_open.set(false); model.left_open.update(|open| *open = !*open); }>"Charts"</button>
                        <button class="sidebar-toggle controls-toggle" type="button" aria-label="Toggle chart controls" aria-expanded=move || model.right_open.get() on:click=move |_| { model.left_open.set(false); model.right_open.update(|open| *open = !*open); }>"Controls"</button>
                        <span class="session-state">{move || active_label(&model.workspace.get())}</span>
                    </div>
                </header>

                <div class="announcements" aria-live="polite">
                    <span class="notice">{move || model.notice.get().unwrap_or_default()}</span>
                    <span class="problem" role="alert">{move || model.problem.get().unwrap_or_default()}</span>
                </div>

                <WorkbenchView platform model />
                <SettingsView platform model />
                <FilesView platform model />
            </div>
        }
    }

    #[component]
    fn WorkbenchView(platform: Platform, model: Model) -> impl IntoView {
        view! {
            <main id="workbench" class="route workbench-route" tabindex="-1">
                <button class:drawer-open=move || model.left_open.get() class="drawer-scrim left-scrim" aria-label="Close charts drawer" on:click=move |_| model.left_open.set(false)></button>
                <aside class:drawer-open=move || model.left_open.get() class="workbench-sidebar left-sidebar" aria-label="Charts and wheels">
                    <div class="sidebar-title"><h1>"Charts"</h1><button class="drawer-close" aria-label="Close charts drawer" on:click=move |_| model.left_open.set(false)>"×"</button></div>
                    <ChartsPanel platform model />
                    <WheelsPanel platform model />
                </aside>

                <WheelStage platform model />

                <button class:drawer-open=move || model.right_open.get() class="drawer-scrim right-scrim" aria-label="Close controls drawer" on:click=move |_| model.right_open.set(false)></button>
                <aside class:drawer-open=move || model.right_open.get() class="workbench-sidebar right-sidebar" aria-label="Controls, points, and aspects">
                    <div class="sidebar-title"><h1>"Controls"</h1><button class="drawer-close" aria-label="Close controls drawer" on:click=move |_| model.right_open.set(false)>"×"</button></div>
                    <TimeControls platform model />
                    <FilterPanel model />
                </aside>
            </main>
        }
    }

    #[component]
    fn ChartsPanel(platform: Platform, model: Model) -> impl IntoView {
        view! {
            <section class="sidebar-module charts-module">
                <div class="module-heading"><h2>"Saved charts"</h2><a href="#settings">"New"</a></div>
                {move || {
                    let workspace = model.workspace.get();
                    if workspace.charts.is_empty() {
                        view! { <p class="empty-copy">"Create a chart and location in Settings to begin."</p> }.into_any()
                    } else {
                        workspace.charts.into_iter().map(|chart| {
                            let inner_id = chart.id.clone();
                            let outer_id = chart.id.clone();
                            let chart_id = chart.id.clone();
                            view! {
                                <article class="chart-card" class:is-inner=move || model.inner_chart_id.get() == chart_id class:is-outer=move || model.outer_chart_id.get() == chart.id>
                                    <div><strong>{chart.label}</strong><small>{format!("{} · {}", chart.role, chart.local_input)}</small></div>
                                    <div class="compact-actions">
                                        <button type="button" on:click=move |_| {
                                            model.inner_chart_id.set(inner_id.clone());
                                            queue_selected_preview(platform, model, None);
                                        }>"Use as Inner"</button>
                                        <button type="button" on:click=move |_| select_outer_chart(platform, model, &outer_id)>"Use as Outer"</button>
                                    </div>
                                </article>
                            }
                        }).collect_view().into_any()
                    }
                }}
                {move || selected_chart(&model.workspace.get(), &model.outer_chart_id.get()).map(|chart| view! {
                    <SelectedChartEditor platform model chart=chart.clone() />
                }).into_any()}
                <div class="location-pair">
                    <label><span>"Inner location"</span><select on:change=move |event| {
                        model.inner_location_id.set(event_target_value(&event));
                        queue_selected_preview(platform, model, None);
                    }>{move || location_options(&model.workspace.get(), &model.inner_location_id.get())}</select></label>
                    <label><span>"Outer location"</span><select on:change=move |event| {
                        model.outer_location_id.set(event_target_value(&event));
                        queue_selected_preview(platform, model, None);
                    }>{move || location_options(&model.workspace.get(), &model.outer_location_id.get())}</select></label>
                </div>
            </section>
        }
    }

    #[component]
    fn SelectedChartEditor(platform: Platform, model: Model, chart: ChartSummary) -> impl IntoView {
        let label = NodeRef::<Input>::new();
        let role = NodeRef::<Select>::new();
        let date = NodeRef::<Input>::new();
        let time = NodeRef::<Input>::new();
        let zone = NodeRef::<Input>::new();
        let chart_id = chart.id.clone();
        let submit = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                let id = StableId::new("chart.id", chart_id.clone()).map_err(|e| e.to_string())?;
                let role = chart_role(select_value(role).as_deref());
                let local_input = LocalDateTimeInput::new(
                    value(date).ok_or("date is required")?,
                    normalized_time(value(time).ok_or("time is required")?),
                    value(zone).ok_or("time zone is required")?,
                )
                .map_err(|e| e.to_string())?;
                model.desired_outer.set(Some(local_input.clone()));
                model.outer_ambiguous_choice.set(None);
                Ok::<_, String>(PlatformCommand::UpdateChartBasics {
                    chart_id: id,
                    label: value(label).ok_or("chart name is required")?,
                    role,
                    local_input,
                })
            })();
            match result {
                Ok(command) => dispatch(platform, model, command),
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <form class="compact-editor" on:submit=submit>
                <h3>"Selected chart editor"</h3>
                <label><span>"Name"</span><input node_ref=label required value=chart.label /></label>
                <label><span>"Role"</span><select node_ref=role>
                    <option value="natal" selected=chart.role == "natal">"Natal"</option>
                    <option value="transit" selected=chart.role == "transit">"Transit"</option>
                    <option value="event" selected=chart.role == "event">"Event"</option>
                </select></label>
                <div class="field-row"><label><span>"Date"</span><input node_ref=date type="date" required value=chart.local_date /></label><label><span>"Time"</span><input node_ref=time type="time" step="1" required value=chart.local_time /></label></div>
                <label><span>"IANA time zone"</span><input node_ref=zone required value=chart.time_zone /></label>
                <button type="submit">"Save definition"</button>
            </form>
        }
    }

    #[component]
    fn WheelsPanel(platform: Platform, model: Model) -> impl IntoView {
        view! {
            <section class="sidebar-module wheels-module">
                <div class="module-heading"><h2>"Wheels"</h2><a href="#settings">"Edit"</a></div>
                <div class="wheel-thumbnail" aria-hidden="true"><span></span></div>
                <div class="template-list">
                    {move || model.wheel_templates.get().templates.into_iter().map(|template| {
                        let id = template.id.clone();
                        let id_for_class = template.id.clone();
                        view! {
                            <button type="button" class:selected=move || model.wheel_templates.get().last_selected_template_id == id_for_class on:click=move |_| select_template(platform, model, &id)>
                                <strong>{template.name}</strong><small>{format!("{:?} · {:?}", template.palette, template.label_density)}</small>
                            </button>
                        }
                    }).collect_view()}
                </div>
            </section>
        }
    }

    #[component]
    fn WheelStage(platform: Platform, model: Model) -> impl IntoView {
        let save_as_name = NodeRef::<Input>::new();
        let wheel_click = move |event: leptos::ev::MouseEvent| {
            if let Some(element) = interaction_element(event.target()) {
                toggle_interaction(model, &element);
            }
        };
        let wheel_key = move |event: KeyboardEvent| {
            if event.key() == "Escape" {
                model.selected_points.set(BTreeSet::new());
                model.selected_aspects.set(BTreeSet::new());
            } else if (event.key() == " " || event.key() == "Enter")
                && let Some(element) = interaction_element(event.target())
            {
                event.prevent_default();
                toggle_interaction(model, &element);
            }
        };
        view! {
            <section id="wheel-stage" class="wheel-stage" aria-label="Chart workbench">
                <div class="chart-meta inner-meta">{move || model.presentation.get().map(|p| format!("INNER · {}\n{} {}\n{}", p.inner.label, p.inner.local_input.local_date(), p.inner.local_input.local_time(), p.inner.location_label)).unwrap_or_default()}</div>
                <div class="chart-meta outer-meta">{move || model.presentation.get().map(|p| format!("OUTER · {}\n{} {}\n{}", p.outer.label, p.outer.local_input.local_date(), p.outer.local_input.local_time(), p.outer.location_label)).unwrap_or_default()}</div>
                <div class="wheel-frame" class:is-calculating=move || model.calculating.get() on:click=wheel_click on:keydown=wheel_key>
                    {move || if let Some(presentation) = model.presentation.get() {
                        let scene = filtered_scene(&presentation.scene, &model.visible_points.get(), &model.visible_aspects.get());
                        let settings = model.wheel_templates.get();
                        let template = settings.selected();
                        let svg = render_biwheel_svg(&scene, &RenderOptions {
                            orientation: template.orientation,
                            palette: template.palette,
                            label_density: template.label_density,
                            selected_points: model.selected_points.get().into_iter().collect(),
                            selected_aspects: model.selected_aspects.get().into_iter().collect(),
                        });
                        view! { <div class="wheel-svg" inner_html=svg></div> }.into_any()
                    } else {
                        let workspace = model.workspace.get();
                        view! {
                            <div class="wheel-empty">
                                <span aria-hidden="true">"☉"</span>
                                <h2>{if workspace.active.is_none() { "Open a workspace" } else { "Two charts and a location make a wheel" }}</h2>
                                <p>"Calculated Moshier previews stay transient until you choose Update Chart or Save As."</p>
                                {if workspace.active.is_none() {
                                    view! { <button class="primary" on:click=move |_| dispatch(platform, model, PlatformCommand::CreateScratch)>"Open scratch"</button> }.into_any()
                                } else {
                                    view! { <a class="button-link" href="#settings">"Add charts and locations"</a> }.into_any()
                                }}
                            </div>
                        }.into_any()
                    }}
                    <div class="calculation-indicator" role="status">{move || if model.calculating.get() { "Calculating newest cursor…" } else { "" }}</div>
                </div>
                <div class="wheel-actions">
                    <button class="primary" type="button" disabled=move || model.presentation.get().is_none() || model.calculating.get() on:click=move |_| commit_preview(platform, model, PreviewSaveMode::UpdateChart)>"Update Chart"</button>
                    <form on:submit=move |event: SubmitEvent| {
                        event.prevent_default();
                        if let Some(name) = value(save_as_name) {
                            commit_preview(platform, model, PreviewSaveMode::SaveAs { name });
                        } else {
                            model.problem.set(Some("Save As requires a new name.".into()));
                        }
                    }>
                        <input node_ref=save_as_name aria-label="New chart name" placeholder="New chart name" required />
                        <button type="submit" disabled=move || model.presentation.get().is_none() || model.calculating.get()>"Save As"</button>
                    </form>
                    <span>{move || model.presentation.get().and_then(|p| p.adjustment_notice).unwrap_or_default()}</span>
                </div>
            </section>
        }
    }

    #[component]
    fn TimeControls(platform: Platform, model: Model) -> impl IntoView {
        view! {
            <section class="sidebar-module controls-module">
                <p class="module-help">"Inner fixed · outer moves"</p>
                <div class="time-controller" aria-label="Outer chart time controls">
                    {TimeInterval::ALL.into_iter().map(|interval| view! { <TimeColumn platform model interval /> }).collect_view()}
                </div>
            </section>
        }
    }

    #[component]
    fn TimeColumn(platform: Platform, model: Model, interval: TimeInterval) -> impl IntoView {
        let forward_action: Rc<dyn Fn()> =
            Rc::new(move || step_outer(platform, model, interval, StepDirection::Forward));
        let backward_action: Rc<dyn Fn()> =
            Rc::new(move || step_outer(platform, model, interval, StepDirection::Backward));
        let forward_down = forward_action.clone();
        let forward_key = forward_action.clone();
        let backward_down = backward_action.clone();
        let backward_key = backward_action.clone();
        view! {
            <div class="time-column">
                <button class="repeat-button" aria-label=format!("Hold to move forward by {}", interval.label())
                    on:pointerdown=move |event: PointerEvent| begin_hold(model, event, forward_down.clone())
                    on:pointerup=move |_| model.stop_hold()
                    on:pointercancel=move |_| model.stop_hold()
                    on:lostpointercapture=move |_| model.stop_hold()
                    on:keydown=move |event: KeyboardEvent| begin_keyboard_hold(model, event, forward_key.clone())
                    on:keyup=move |_| model.stop_hold()>{">>"}</button>
                <button aria-label=format!("Move forward by {}", interval.label()) on:click=move |_| step_outer(platform, model, interval, StepDirection::Forward)>{">"}</button>
                <strong>{interval.label()}</strong>
                <button aria-label=format!("Move backward by {}", interval.label()) on:click=move |_| step_outer(platform, model, interval, StepDirection::Backward)>{"<"}</button>
                <button class="repeat-button" aria-label=format!("Hold to move backward by {}", interval.label())
                    on:pointerdown=move |event: PointerEvent| begin_hold(model, event, backward_down.clone())
                    on:pointerup=move |_| model.stop_hold()
                    on:pointercancel=move |_| model.stop_hold()
                    on:lostpointercapture=move |_| model.stop_hold()
                    on:keydown=move |event: KeyboardEvent| begin_keyboard_hold(model, event, backward_key.clone())
                    on:keyup=move |_| model.stop_hold()>{"<<"}</button>
            </div>
        }
    }

    #[component]
    fn FilterPanel(model: Model) -> impl IntoView {
        view! {
            <section class="sidebar-module filters-module">
                <div class="module-heading"><h2>"Points"</h2><button type="button" on:click=move |_| {
                    model.visible_points.set(POINT_FILTERS.iter().map(|(id, _)| (*id).into()).collect());
                }>"All"</button></div>
                <div class="filter-grid">
                    {POINT_FILTERS.into_iter().map(|(id, label)| view! {
                        <label><input type="checkbox" checked=move || model.visible_points.get().contains(id) on:change=move |event| set_filter(model.visible_points, id, event_target_checked(&event)) /><span>{label}</span></label>
                    }).collect_view()}
                </div>
            </section>
            <section class="sidebar-module filters-module aspects-filter">
                <div class="module-heading"><h2>"Aspects"</h2><button type="button" on:click=move |_| {
                    model.visible_aspects.set(ASPECT_FILTERS.iter().map(|(id, _)| (*id).into()).collect());
                }>"All"</button></div>
                <div class="filter-grid">
                    {ASPECT_FILTERS.into_iter().map(|(id, label)| view! {
                        <label><input type="checkbox" checked=move || model.visible_aspects.get().contains(id) on:change=move |event| set_filter(model.visible_aspects, id, event_target_checked(&event)) /><span>{label}</span></label>
                    }).collect_view()}
                </div>
                <p class="module-help">"Focus highlights connections. Click or Space keeps multiple selections; Escape clears."</p>
            </section>
        }
    }

    #[component]
    fn SettingsView(platform: Platform, model: Model) -> impl IntoView {
        view! {
            <main id="settings" class="route scroll-route" tabindex="-1">
                <div class="route-heading"><div><p class="eyebrow">"Studio preferences"</p><h1>"Settings"</h1></div><a href="#workbench">"Back to workbench"</a></div>
                <TemplateSettings platform model />
                <PeopleSettings platform model />
                <LocationSettings platform model />
                <ChartSettings platform model />
                <AdvancedSettings platform model />
            </main>
        }
    }

    #[component]
    fn TemplateSettings(platform: Platform, model: Model) -> impl IntoView {
        let name = NodeRef::<Input>::new();
        let orientation = NodeRef::<Select>::new();
        let palette = NodeRef::<Select>::new();
        let density = NodeRef::<Select>::new();
        let save = move |new_record: bool| {
            let result = (|| {
                let name_value = value(name).ok_or("template name is required")?;
                let id = if new_record {
                    let ids = model
                        .wheel_templates
                        .get_untracked()
                        .templates
                        .iter()
                        .map(|item| item.id.clone())
                        .collect();
                    generate_unique_id("wheel-template", &name_value, &ids)
                        .map_err(|e| e.to_string())?
                        .as_str()
                        .to_owned()
                } else {
                    model
                        .wheel_templates
                        .get_untracked()
                        .last_selected_template_id
                };
                Ok::<_, String>(WheelTemplate {
                    id,
                    name: name_value,
                    orientation: if select_value(orientation).as_deref() == Some("zodiac-zero-top")
                    {
                        WheelOrientation::ZodiacZeroTop
                    } else {
                        WheelOrientation::AscendantLeft
                    },
                    palette: match select_value(palette).as_deref() {
                        Some("paper-light") => WheelPalette::PaperLight,
                        Some("high-contrast") => WheelPalette::HighContrast,
                        _ => WheelPalette::StudioDark,
                    },
                    label_density: if select_value(density).as_deref() == Some("compact") {
                        LabelDensity::Compact
                    } else {
                        LabelDensity::Full
                    },
                })
            })();
            match result {
                Ok(template) => dispatch(
                    platform,
                    model,
                    PlatformCommand::SaveWheelTemplate { template },
                ),
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <section class="settings-panel">
                <div><p class="eyebrow">"Global, unencrypted"</p><h2>"Wheel templates"</h2><p>"Templates contain visual choices only—never chart identities, dates, points, or aspects."</p></div>
                <form class="settings-form" on:submit=move |event: SubmitEvent| { event.prevent_default(); save(false); }>
                    <label><span>"Name"</span><input node_ref=name required prop:value=move || model.wheel_templates.get().selected().name.clone() /></label>
                    <label><span>"Orientation"</span><select node_ref=orientation><option value="ascendant-left" prop:selected=move || model.wheel_templates.get().selected().orientation == WheelOrientation::AscendantLeft>"Ascendant Left"</option><option value="zodiac-zero-top" prop:selected=move || model.wheel_templates.get().selected().orientation == WheelOrientation::ZodiacZeroTop>"Zodiac Zero Top"</option></select></label>
                    <label><span>"Palette"</span><select node_ref=palette><option value="studio-dark" prop:selected=move || model.wheel_templates.get().selected().palette == WheelPalette::StudioDark>"Studio Dark"</option><option value="paper-light" prop:selected=move || model.wheel_templates.get().selected().palette == WheelPalette::PaperLight>"Paper Light"</option><option value="high-contrast" prop:selected=move || model.wheel_templates.get().selected().palette == WheelPalette::HighContrast>"High Contrast"</option></select></label>
                    <label><span>"Label density"</span><select node_ref=density><option value="full" prop:selected=move || model.wheel_templates.get().selected().label_density == LabelDensity::Full>"Full"</option><option value="compact" prop:selected=move || model.wheel_templates.get().selected().label_density == LabelDensity::Compact>"Compact"</option></select></label>
                    <div class="button-row"><button class="primary" type="submit">"Save selected"</button><button type="button" on:click=move |_| save(true)>"Save as new"</button><button class="danger" type="button" on:click=move |_| dispatch(platform, model, PlatformCommand::RemoveWheelTemplate { template_id: model.wheel_templates.get_untracked().last_selected_template_id })>"Remove"</button></div>
                </form>
            </section>
        }
    }

    #[component]
    fn PeopleSettings(platform: Platform, model: Model) -> impl IntoView {
        let name = NodeRef::<Input>::new();
        let notes = NodeRef::<Textarea>::new();
        let submit = move |event: SubmitEvent| {
            event.prevent_default();
            let Some(display_name) = value(name) else {
                return;
            };
            let ids = model
                .workspace
                .get_untracked()
                .people
                .iter()
                .map(|item| item.id.clone())
                .collect();
            match generate_unique_id("person", &display_name, &ids) {
                Ok(id) => dispatch(
                    platform,
                    model,
                    PlatformCommand::AddPerson {
                        id,
                        display_name,
                        kind: PersonKind::Personal,
                        notes: text_value(notes).filter(|text| !text.trim().is_empty()),
                    },
                ),
                Err(error) => model.problem.set(Some(error.to_string())),
            }
        };
        view! {
            <section class="settings-panel"><div><p class="eyebrow">"Encrypted records"</p><h2>"People"</h2><EntityList items=Signal::derive(move || model.workspace.get().people) /></div>
                <form class="settings-form" on:submit=submit><label><span>"Display name"</span><input node_ref=name required /></label><label><span>"Notes"</span><textarea node_ref=notes></textarea></label><button class="primary" type="submit">"Add person"</button></form>
            </section>
        }
    }

    #[component]
    fn LocationSettings(platform: Platform, model: Model) -> impl IntoView {
        let label = NodeRef::<Input>::new();
        let country = NodeRef::<Input>::new();
        let zone = NodeRef::<Input>::new();
        let latitude = NodeRef::<Input>::new();
        let longitude = NodeRef::<Input>::new();
        let query = NodeRef::<Input>::new();
        let submit = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                let label_value = value(label).ok_or("location name is required")?;
                let ids = model
                    .workspace
                    .get_untracked()
                    .locations
                    .iter()
                    .map(|item| item.id.clone())
                    .collect();
                SavedLocation::new(
                    generate_unique_id("location", &label_value, &ids)
                        .map_err(|e| e.to_string())?,
                    label_value,
                    Vec::new(),
                    value(country)
                        .ok_or("country is required")?
                        .to_ascii_uppercase(),
                    value(latitude)
                        .ok_or("latitude is required")?
                        .parse()
                        .map_err(|_| "invalid latitude")?,
                    value(longitude)
                        .ok_or("longitude is required")?
                        .parse()
                        .map_err(|_| "invalid longitude")?,
                    None,
                    value(zone).ok_or("time zone is required")?,
                    LocationProvenance::Manual,
                )
                .map_err(|e| e.to_string())
            })();
            match result {
                Ok(location) => {
                    dispatch(platform, model, PlatformCommand::SaveLocation { location })
                }
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <section class="settings-panel"><div><p class="eyebrow">"Encrypted snapshots"</p><h2>"Locations / GeoNames"</h2><EntityList items=Signal::derive(move || model.workspace.get().locations) /></div>
                <div class="settings-stack">
                    <form class="settings-form" on:submit=submit><label><span>"Location name"</span><input node_ref=label required /></label><div class="field-row"><label><span>"Country"</span><input node_ref=country maxlength="2" required value="US" /></label><label><span>"IANA time zone"</span><input node_ref=zone required value="America/New_York" /></label></div><div class="field-row"><label><span>"Latitude"</span><input node_ref=latitude required inputmode="decimal" /></label><label><span>"Longitude"</span><input node_ref=longitude required inputmode="decimal" /></label></div><button class="primary">"Save location"</button></form>
                    <div class="settings-form"><h3>"Local GeoNames catalog"</h3><p>{move || model.capabilities.get().and_then(|status| status.catalog).map(|catalog| format!("{} local places · {}", catalog.place_count, catalog.content_id)).unwrap_or_else(|| "No catalog installed; manual locations remain available.".into())}</p><button on:click=move |_| dispatch(platform, model, PlatformCommand::InstallPinnedCatalog)>"Install pinned catalog"</button><form class="inline-search" on:submit=move |event: SubmitEvent| { event.prevent_default(); if let Some(query) = value(query) { dispatch(platform, model, PlatformCommand::SearchCatalog { query, limit: 20 }); } }><input node_ref=query aria-label="Search GeoNames" required /><button>"Search locally"</button></form><ul class="search-results">{move || model.catalog_results.get().into_iter().map(|result| view! { <li><strong>{result.place().name().to_owned()}</strong><small>{format!("{} · {}", result.place().country_code(), result.place().time_zone())}</small></li> }).collect_view()}</ul></div>
                </div>
            </section>
        }
    }

    #[component]
    fn ChartSettings(platform: Platform, model: Model) -> impl IntoView {
        let label = NodeRef::<Input>::new();
        let role = NodeRef::<Select>::new();
        let date = NodeRef::<Input>::new();
        let time = NodeRef::<Input>::new();
        let zone = NodeRef::<Input>::new();
        let zodiac = NodeRef::<Select>::new();
        let houses = NodeRef::<Select>::new();
        let submit = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                let label_value = value(label).ok_or("chart name is required")?;
                let ids = model
                    .workspace
                    .get_untracked()
                    .charts
                    .iter()
                    .map(|item| item.id.clone())
                    .collect();
                let zodiac_value = if select_value(zodiac).as_deref() == Some("sidereal") {
                    ZodiacId::Sidereal
                } else {
                    ZodiacId::Tropical
                };
                let default_options = ChartCalculationOptions::default();
                let options = ChartCalculationOptions::new(
                    zodiac_value,
                    (zodiac_value == ZodiacId::Sidereal).then_some(AyanamsaId::Lahiri),
                    match select_value(houses).as_deref() {
                        Some("whole-sign") => HouseSystemId::WholeSign,
                        Some("equal") => HouseSystemId::Equal,
                        _ => HouseSystemId::Placidus,
                    },
                    default_options.ordered_objects().to_vec(),
                )
                .map_err(|e| e.to_string())?;
                ChartDefinition::new(
                    generate_unique_id("chart", &label_value, &ids).map_err(|e| e.to_string())?,
                    label_value,
                    chart_role(select_value(role).as_deref()),
                    None,
                    LocalDateTimeInput::new(
                        value(date).ok_or("date is required")?,
                        normalized_time(value(time).ok_or("time is required")?),
                        value(zone).ok_or("time zone is required")?,
                    )
                    .map_err(|e| e.to_string())?,
                    options,
                    default_chart_points(),
                    false,
                )
                .map_err(|e| e.to_string())
            })();
            match result {
                Ok(chart) => dispatch(platform, model, PlatformCommand::SaveChart { chart }),
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <section class="settings-panel"><div><p class="eyebrow">"Definitions and defaults"</p><h2>"Charts"</h2><ul class="entity-list">{move || model.workspace.get().charts.into_iter().map(|chart| view! { <li><strong>{chart.label}</strong><small>{chart.local_input}</small></li> }).collect_view()}</ul></div>
                <form class="settings-form" on:submit=submit><label><span>"Chart name"</span><input node_ref=label required /></label><div class="field-row"><label><span>"Role"</span><select node_ref=role><option value="natal">"Natal"</option><option value="transit">"Transit"</option><option value="event">"Event"</option></select></label><label><span>"Zodiac"</span><select node_ref=zodiac><option value="tropical">"Tropical"</option><option value="sidereal">"Sidereal · Lahiri"</option></select></label></div><div class="field-row"><label><span>"Date"</span><input node_ref=date type="date" required /></label><label><span>"Time"</span><input node_ref=time type="time" step="1" required /></label></div><label><span>"IANA time zone"</span><input node_ref=zone required value="America/New_York" /></label><label><span>"House system"</span><select node_ref=houses><option value="placidus">"Placidus"</option><option value="whole-sign">"Whole Sign"</option><option value="equal">"Equal"</option></select></label><button class="primary">"Create chart"</button></form>
            </section>
        }
    }

    #[component]
    fn AdvancedSettings(platform: Platform, model: Model) -> impl IntoView {
        let label = NodeRef::<Input>::new();
        let inner = NodeRef::<Select>::new();
        let outer = NodeRef::<Select>::new();
        let submit = move |event: SubmitEvent| {
            event.prevent_default();
            let result = (|| {
                let label_value = value(label).ok_or("comparison name is required")?;
                let ids = model
                    .workspace
                    .get_untracked()
                    .comparisons
                    .iter()
                    .map(|item| item.id.clone())
                    .collect();
                ComparisonPreset::new(
                    generate_unique_id("comparison", &label_value, &ids)
                        .map_err(|e| e.to_string())?,
                    label_value,
                    StableId::new(
                        "comparison.inner",
                        select_value(inner).ok_or("inner chart is required")?,
                    )
                    .map_err(|e| e.to_string())?,
                    StableId::new(
                        "comparison.outer",
                        select_value(outer).ok_or("outer chart is required")?,
                    )
                    .map_err(|e| e.to_string())?,
                    default_chart_points(),
                    default_chart_points(),
                    default_aspects(),
                    ComparisonWheelOrientation::AscendantLeft,
                )
                .map_err(|e| e.to_string())
            })();
            match result {
                Ok(preset) => dispatch(platform, model, PlatformCommand::SaveComparison { preset }),
                Err(message) => model.problem.set(Some(message)),
            }
        };
        view! {
            <details class="settings-panel advanced"><summary><span><span class="eyebrow">"Advanced"</span><strong>"Comparison records"</strong></span></summary><div><p>"Saved comparison presets remain separate from global visual wheel templates."</p><EntityList items=Signal::derive(move || model.workspace.get().comparisons) /></div><form class="settings-form" on:submit=submit><label><span>"Preset name"</span><input node_ref=label required /></label><label><span>"Inner chart"</span><select node_ref=inner required>{move || chart_options(&model.workspace.get(), "")}</select></label><label><span>"Outer chart"</span><select node_ref=outer required>{move || chart_options(&model.workspace.get(), "")}</select></label><button>"Save comparison preset"</button></form></details>
        }
    }

    #[component]
    fn EntityList(items: Signal<Vec<oracle_studio_platform::EntitySummary>>) -> impl IntoView {
        view! { <ul class="entity-list">{move || items.get().into_iter().map(|item| view! { <li><strong>{item.label}</strong></li> }).collect_view()}</ul> }
    }

    #[component]
    fn FilesView(platform: Platform, model: Model) -> impl IntoView {
        let title = NodeRef::<Input>::new();
        let password = NodeRef::<Input>::new();
        let import = NodeRef::<Input>::new();
        let replace = NodeRef::<Input>::new();
        view! {
            <main id="files" class="route scroll-route" tabindex="-1">
                <div class="route-heading"><div><p class="eyebrow">"Portable, browser-local storage"</p><h1>"Files"</h1></div><a href="#workbench">"Back to workbench"</a></div>
                <section class="file-warning"><strong>"Exports are your backups."</strong><span>{move || model.capabilities.get().map(|status| status.backup_warning).unwrap_or_else(|| "Browser storage can be evicted.".into())}</span></section>
                <section class="file-toolbar">
                    <button class="primary" on:click=move |_| dispatch(platform, model, PlatformCommand::CreateScratch)>"New scratch"</button>
                    <label class="file-action">"Import .oracle-vault"<input node_ref=import type="file" accept=".oracle-vault,application/octet-stream" on:change=move |_| {
                        let Some(file) = import.get().and_then(|input| input.files()).and_then(|files| files.item(0)) else { return; };
                        let replace_confirmed = replace.get().is_some_and(|input| input.checked());
                        spawn_local(async move { match read_file(file).await { Ok(bytes) => dispatch(platform, model, PlatformCommand::ImportVault { bytes, replace_confirmed }), Err(message) => model.problem.set(Some(message)) } });
                    } /></label>
                    <label class="check-label"><input node_ref=replace type="checkbox" /><span>"Replace duplicate vault ID"</span></label>
                </section>
                {move || if model.workspace.get().active == Some(ActiveWorkspace::Scratch) {
                    view! { <form class="save-scratch settings-form" on:submit=move |event: SubmitEvent| { event.prevent_default(); if let (Some(title), Some(password_value)) = (value(title), value(password)) { dispatch(platform, model, PlatformCommand::SaveScratch { title, password: password_value.into_bytes() }); if let Some(input) = password.get() { input.set_value(""); } } }><h2>"Save scratch as an encrypted vault"</h2><label><span>"Public title"</span><input node_ref=title required /></label><label><span>"Password"</span><input node_ref=password type="password" autocomplete="new-password" required /></label><div class="button-row"><button class="primary">"Save encrypted vault"</button><button type="button" class="danger" on:click=move |_| { let confirmed = !model.workspace.get_untracked().scratch_dirty || confirm("Discard unsaved scratch work?"); dispatch(platform, model, PlatformCommand::DiscardScratch { confirmed }); }>"Discard scratch"</button></div></form> }.into_any()
                } else { ().into_any() }}
                <section class="vault-grid">
                    <For each=move || model.vaults.get() key=|vault| format!("{}:{}:{:?}", vault.id, vault.revision, vault.lock_state) children=move |vault| view! { <VaultCard platform model vault /> } />
                    {move || if model.vaults.get().is_empty() { view! { <div class="empty-card"><span>"◇"</span><p>"No encrypted vaults in this browser."</p></div> }.into_any() } else { ().into_any() }}
                </section>
            </main>
        }
    }

    #[component]
    fn VaultCard(platform: Platform, model: Model, vault: VaultSummary) -> impl IntoView {
        let password = NodeRef::<Input>::new();
        let unlock_id = vault.id.clone();
        let activate_id = vault.id.clone();
        let lock_id = vault.id.clone();
        let export_id = vault.id.clone();
        let remove_id = vault.id.clone();
        let state = vault.lock_state.clone();
        view! {
            <article class="vault-card"><div class="vault-card-top"><span class="vault-icon">"◈"</span><span class=format!("badge {:?}", state)>{format!("{:?}", state)}</span></div><h2>{vault.title}</h2><small>{format!("Updated {}", vault.modified_at)}</small>
                {if state == VaultLockState::Locked {
                    view! { <form class="unlock-form" on:submit=move |event: SubmitEvent| { event.prevent_default(); if let Some(password_value) = value(password) { dispatch(platform, model, PlatformCommand::UnlockVault { vault_id: unlock_id.clone(), password: password_value.into_bytes() }); if let Some(input) = password.get() { input.set_value(""); } } }><label><span>"Password"</span><input node_ref=password type="password" autocomplete="current-password" required /></label><button class="primary">"Unlock"</button></form> }.into_any()
                } else {
                    view! { <div class="button-row"><button class="primary" on:click=move |_| dispatch(platform, model, PlatformCommand::ActivateVault { vault_id: activate_id.clone() })>"Use"</button><button on:click=move |_| dispatch(platform, model, PlatformCommand::LockVault { vault_id: lock_id.clone() })>"Lock"</button></div> }.into_any()
                }}
                <div class="button-row secondary-actions"><button on:click=move |_| dispatch(platform, model, PlatformCommand::ExportVault { vault_id: export_id.clone() })>"Export"</button><button class="danger" on:click=move |_| dispatch(platform, model, PlatformCommand::RemoveVault { vault_id: remove_id.clone(), confirmed: confirm("Remove this browser copy? Exported backups are unaffected.") })>"Remove"</button></div>
            </article>
        }
    }

    fn dispatch(platform: Platform, model: Model, command: PlatformCommand) {
        model.busy.set(true);
        model.problem.set(None);
        let future = platform.with_value(|platform| platform.execute(command));
        spawn_local(async move {
            match future.await {
                Ok(response) => {
                    let preview_after = apply_response(model, response);
                    if preview_after && ensure_workbench_defaults(model) {
                        queue_selected_preview(platform, model, None);
                    }
                }
                Err(error) => model.problem.set(Some(error.message)),
            }
            model.busy.set(false);
        });
    }

    fn apply_response(model: Model, response: PlatformResponse) -> bool {
        match response {
            PlatformResponse::Ready {
                vaults,
                workspace,
                capabilities,
                wheel_templates,
            } => {
                model.vaults.set(vaults);
                model.workspace.set(workspace);
                model.capabilities.set(Some(capabilities));
                model.wheel_templates.set(wheel_templates);
                model.notice.set(Some("Browser-local studio ready.".into()));
                true
            }
            PlatformResponse::Vaults(vaults) => {
                model.vaults.set(vaults);
                false
            }
            PlatformResponse::Workspace(workspace) => {
                model.workspace.set(workspace);
                true
            }
            PlatformResponse::Updated { vaults, workspace } => {
                model.vaults.set(vaults);
                model.workspace.set(workspace);
                model.notice.set(Some("Local workspace updated.".into()));
                true
            }
            PlatformResponse::WheelTemplates(settings) => {
                model.wheel_templates.set(settings);
                false
            }
            PlatformResponse::WorkbenchPreview(presentation) => {
                model.presentation.set(Some(presentation));
                false
            }
            PlatformResponse::Export { filename, bytes } => {
                match download(&filename, &bytes) {
                    Ok(()) => model.notice.set(Some(format!("Downloaded {filename}."))),
                    Err(message) => model.problem.set(Some(message)),
                };
                false
            }
            PlatformResponse::LocalTime(resolution) => {
                model.notice.set(Some(format_local_time(&resolution)));
                false
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
                false
            }
            PlatformResponse::CatalogResults(results) => {
                model
                    .notice
                    .set(Some(format!("Found {} local matches.", results.len())));
                model.catalog_results.set(results);
                false
            }
        }
    }

    fn ensure_workbench_defaults(model: Model) -> bool {
        let workspace = model.workspace.get_untracked();
        if workspace.active.is_none()
            || workspace.charts.is_empty()
            || workspace.locations.is_empty()
        {
            model.presentation.set(None);
            return false;
        }
        if !workspace
            .charts
            .iter()
            .any(|chart| chart.id == model.inner_chart_id.get_untracked())
        {
            model.inner_chart_id.set(workspace.charts[0].id.clone());
        }
        if !workspace
            .charts
            .iter()
            .any(|chart| chart.id == model.outer_chart_id.get_untracked())
        {
            model.outer_chart_id.set(
                workspace
                    .charts
                    .get(1)
                    .unwrap_or(&workspace.charts[0])
                    .id
                    .clone(),
            );
            let chart = selected_chart(&workspace, &model.outer_chart_id.get_untracked()).unwrap();
            model.desired_outer.set(chart_input(chart).ok());
        }
        if !workspace
            .locations
            .iter()
            .any(|item| item.id == model.inner_location_id.get_untracked())
        {
            model
                .inner_location_id
                .set(workspace.locations[0].id.clone());
        }
        if !workspace
            .locations
            .iter()
            .any(|item| item.id == model.outer_location_id.get_untracked())
        {
            model
                .outer_location_id
                .set(workspace.locations[0].id.clone());
        }
        if model.desired_outer.get_untracked().is_none()
            && let Some(chart) = selected_chart(&workspace, &model.outer_chart_id.get_untracked())
        {
            model.desired_outer.set(chart_input(chart).ok());
        }
        true
    }

    fn select_outer_chart(platform: Platform, model: Model, id: &str) {
        model.outer_chart_id.set(id.into());
        model.outer_ambiguous_choice.set(None);
        if let Some(chart) = selected_chart(&model.workspace.get_untracked(), id) {
            model.desired_outer.set(chart_input(chart).ok());
        }
        queue_selected_preview(platform, model, None);
    }

    fn queue_selected_preview(platform: Platform, model: Model, adjustment_notice: Option<String>) {
        let result = (|| {
            let desired = model
                .desired_outer
                .get_untracked()
                .ok_or("outer chart time is unavailable")?;
            Ok::<_, String>(PreviewPayload {
                inner_chart_definition_id: StableId::new(
                    "workbench.inner",
                    model.inner_chart_id.get_untracked(),
                )
                .map_err(|e| e.to_string())?,
                outer_chart_definition_id: StableId::new(
                    "workbench.outer",
                    model.outer_chart_id.get_untracked(),
                )
                .map_err(|e| e.to_string())?,
                inner_saved_location_id: StableId::new(
                    "workbench.inner_location",
                    model.inner_location_id.get_untracked(),
                )
                .map_err(|e| e.to_string())?,
                outer_saved_location_id: StableId::new(
                    "workbench.outer_location",
                    model.outer_location_id.get_untracked(),
                )
                .map_err(|e| e.to_string())?,
                outer_local_input: desired,
                outer_ambiguous_time_choice: model.outer_ambiguous_choice.get_untracked(),
                adjustment_notice,
            })
        })();
        let payload = match result {
            Ok(value) => value,
            Err(message) => {
                model.problem.set(Some(message));
                return;
            }
        };
        let decision = model
            .coordinator
            .with_value(|state| state.borrow_mut().enqueue(payload));
        match decision {
            PreviewEnqueue::Dispatch {
                generation,
                payload,
            } => send_preview(platform, model, generation, payload),
            PreviewEnqueue::Coalesced { generation } => {
                model.latest_generation.set(generation);
                model.calculating.set(true);
            }
        }
    }

    fn send_preview(platform: Platform, model: Model, generation: u64, payload: PreviewPayload) {
        model.latest_generation.set(generation);
        model.calculating.set(true);
        model.problem.set(None);
        let request = WorkbenchPreviewRequest {
            generation: PreviewGeneration::new(generation),
            inner_chart_definition_id: payload.inner_chart_definition_id,
            outer_chart_definition_id: payload.outer_chart_definition_id,
            inner_saved_location_id: payload.inner_saved_location_id,
            outer_saved_location_id: payload.outer_saved_location_id,
            outer_local_input: payload.outer_local_input,
            outer_ambiguous_time_choice: payload.outer_ambiguous_time_choice,
            adjustment_notice: payload.adjustment_notice,
        };
        let future = platform
            .with_value(|platform| platform.execute(PlatformCommand::WorkbenchPreview { request }));
        spawn_local(async move {
            let result = future.await;
            let completion = model
                .coordinator
                .with_value(|state| state.borrow_mut().complete(generation));
            match result {
                Ok(PlatformResponse::WorkbenchPreview(presentation))
                    if completion.accept_response
                        && presentation.generation.get() == generation =>
                {
                    model
                        .desired_outer
                        .set(Some(presentation.outer.local_input.clone()));
                    model.notice.set(presentation.adjustment_notice.clone());
                    model.presentation.set(Some(presentation));
                    model.fallback_presentation.set(None);
                }
                Ok(PlatformResponse::WorkbenchPreview(presentation)) => {
                    model.fallback_presentation.set(Some(presentation));
                }
                Ok(response) => {
                    apply_response(model, response);
                }
                Err(error) => {
                    model.stop_hold();
                    model
                        .coordinator
                        .with_value(|state| state.borrow_mut().cancel());
                    if let Some(fallback) = model.fallback_presentation.get_untracked() {
                        model
                            .desired_outer
                            .set(Some(fallback.outer.local_input.clone()));
                        model.presentation.set(Some(fallback));
                        model.fallback_presentation.set(None);
                    } else if let Some(last) = model.presentation.get_untracked() {
                        model.desired_outer.set(Some(last.outer.local_input));
                    }
                    model.problem.set(Some(error.message));
                    model.calculating.set(false);
                    return;
                }
            }
            if let Some((next_generation, next_payload)) = completion.next {
                send_preview(platform, model, next_generation, next_payload);
            } else {
                model.calculating.set(false);
            }
        });
    }

    fn step_outer(
        platform: Platform,
        model: Model,
        interval: TimeInterval,
        direction: StepDirection,
    ) {
        let Some(input) = model.desired_outer.get_untracked() else {
            return;
        };
        let previous_offset = model
            .presentation
            .get_untracked()
            .map(|value| value.outer.utc_offset_seconds);
        match step_local_time(&input, previous_offset, interval, direction) {
            Ok(step) => {
                model.desired_outer.set(Some(step.local_input));
                model.outer_ambiguous_choice.set(step.ambiguous_time_choice);
                queue_selected_preview(platform, model, step.adjustment_notice);
            }
            Err(error) => {
                model.stop_hold();
                model.problem.set(Some(error.to_string()));
            }
        }
    }

    fn commit_preview(platform: Platform, model: Model, save_mode: PreviewSaveMode) {
        if let Some(presentation) = model.presentation.get_untracked() {
            dispatch(
                platform,
                model,
                PlatformCommand::CommitWorkbenchPreview {
                    generation: presentation.generation,
                    save_mode,
                },
            );
        }
    }

    fn select_template(platform: Platform, model: Model, id: &str) {
        model
            .wheel_templates
            .update(|settings| settings.last_selected_template_id = id.into());
        dispatch(
            platform,
            model,
            PlatformCommand::SelectWheelTemplate {
                template_id: id.into(),
            },
        );
    }

    fn begin_hold(model: Model, event: PointerEvent, action: Rc<dyn Fn()>) {
        event.prevent_default();
        if let Some(target) = event
            .current_target()
            .and_then(|target| target.dyn_into::<Element>().ok())
        {
            let _ = target.set_pointer_capture(event.pointer_id());
        }
        model.holds.with_value(|holds| holds.start(action));
    }

    fn begin_keyboard_hold(model: Model, event: KeyboardEvent, action: Rc<dyn Fn()>) {
        if !event.repeat() && (event.key() == " " || event.key() == "Enter") {
            event.prevent_default();
            model.holds.with_value(|holds| holds.start(action));
        }
    }

    fn toggle_interaction(model: Model, element: &Element) {
        match element.get_attribute("data-interaction").as_deref() {
            Some("point") => {
                if let Some(id) = element.get_attribute("data-point-id") {
                    toggle_set(model.selected_points, &id);
                }
            }
            Some("aspect") => {
                if let Some(id) = element.get_attribute("data-aspect-id") {
                    toggle_set(model.selected_aspects, &id);
                }
            }
            _ => {}
        }
    }

    fn interaction_element(target: Option<web_sys::EventTarget>) -> Option<Element> {
        target?
            .dyn_into::<Element>()
            .ok()?
            .closest("[data-interaction]")
            .ok()
            .flatten()
    }

    fn toggle_set(signal: RwSignal<BTreeSet<String>>, id: &str) {
        signal.update(|set| {
            if !set.remove(id) {
                set.insert(id.into());
            }
        });
    }

    fn set_filter(signal: RwSignal<BTreeSet<String>>, id: &str, checked: bool) {
        signal.update(|set| {
            if checked {
                set.insert(id.into());
            } else {
                set.remove(id);
            }
        });
    }

    fn selected_chart<'a>(workspace: &'a WorkspaceSummary, id: &str) -> Option<&'a ChartSummary> {
        workspace.charts.iter().find(|chart| chart.id == id)
    }

    fn chart_input(
        chart: &ChartSummary,
    ) -> Result<LocalDateTimeInput, oracle_studio_core::ModelError> {
        LocalDateTimeInput::new(&chart.local_date, &chart.local_time, &chart.time_zone)
    }

    fn chart_role(value: Option<&str>) -> ChartRole {
        match value {
            Some("natal") => ChartRole::Natal,
            Some("event") => ChartRole::Event,
            _ => ChartRole::Transit,
        }
    }

    fn normalized_time(value: String) -> String {
        if value.matches(':').count() == 1 {
            format!("{value}:00")
        } else {
            value
        }
    }

    fn location_options(workspace: &WorkspaceSummary, selected: &str) -> impl IntoView + use<> {
        workspace.locations.iter().map(|item| view! { <option value=item.id.clone() selected=item.id == selected>{item.label.clone()}</option> }).collect_view()
    }

    fn chart_options(workspace: &WorkspaceSummary, selected: &str) -> impl IntoView + use<> {
        workspace.charts.iter().map(|item| view! { <option value=item.id.clone() selected=item.id == selected>{item.label.clone()}</option> }).collect_view()
    }

    fn install_lifecycle_guards(model: Model) {
        let before_unload =
            Closure::<dyn FnMut(BeforeUnloadEvent)>::new(move |event: BeforeUnloadEvent| {
                if model.workspace.get_untracked().scratch_dirty {
                    event.prevent_default();
                    event.set_return_value("Unsaved scratch work will be lost.");
                }
            });
        let blur_model = model;
        let blur = Closure::<dyn FnMut()>::new(move || blur_model.stop_hold());
        let visibility_model = model;
        let visibility = Closure::<dyn FnMut()>::new(move || {
            if web_sys::window()
                .and_then(|window| window.document())
                .is_some_and(|document| document.hidden())
            {
                visibility_model.stop_hold();
            }
        });
        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback(
                "beforeunload",
                before_unload.as_ref().unchecked_ref(),
            );
            let _ = window.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref());
            if let Some(document) = window.document() {
                let _ = document.add_event_listener_with_callback(
                    "visibilitychange",
                    visibility.as_ref().unchecked_ref(),
                );
            }
            before_unload.forget();
            blur.forget();
            visibility.forget();
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
            .map_err(|_| "Could not create export download.".to_string())?;
        let url = Url::create_object_url_with_blob(&blob)
            .map_err(|_| "Could not create export URL.".to_string())?;
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
        Url::revoke_object_url(&url).map_err(|_| "Could not release export URL.".to_string())
    }

    fn confirm(message: &str) -> bool {
        web_sys::window()
            .and_then(|window| window.confirm_with_message(message).ok())
            .unwrap_or(false)
    }

    fn active_label(workspace: &WorkspaceSummary) -> String {
        match workspace.active.as_ref() {
            Some(ActiveWorkspace::Scratch) => if workspace.scratch_dirty {
                "Scratch · unsaved"
            } else {
                "Scratch · clean"
            }
            .into(),
            Some(ActiveWorkspace::Vault(_)) => "Encrypted vault · active".into(),
            None => "No active workspace".into(),
        }
    }

    fn empty_workspace() -> WorkspaceSummary {
        WorkspaceSummary {
            active: None,
            scratch_dirty: false,
            people: Vec::new(),
            locations: Vec::new(),
            charts: Vec::new(),
            comparisons: Vec::new(),
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
                "Unique local time: {} · {}",
                value.utc_instant(),
                value.utc_offset_display()
            ),
            LocalTimeResolution::Ambiguous { earlier, later } => format!(
                "Ambiguous local time: {} or {}",
                earlier.utc_instant(),
                later.utc_instant()
            ),
            LocalTimeResolution::Nonexistent => {
                "That local clock time does not exist because of a daylight-saving transition."
                    .into()
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::App;

#[cfg(not(target_arch = "wasm32"))]
#[leptos::component]
pub fn App() -> impl leptos::IntoView {
    leptos::view! { <main><h1>"Oracle Studio"</h1><p>"Build this application for wasm32-unknown-unknown."</p></main> }
}
