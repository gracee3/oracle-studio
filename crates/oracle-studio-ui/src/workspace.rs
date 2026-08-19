use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use leptos::{
    ev::SubmitEvent,
    html::{Input, Select},
    prelude::*,
};
use oracle_studio_chart_view::{
    ChartAspect, ChartPoint, ChartRing, ChartScene, RenderOptions, WheelOrientation,
    render_biwheel_svg,
};
use oracle_studio_protocol::{
    AspectDefinitionInput, AspectKindInput, BiwheelScene, CalculateComparisonRequest,
    ChartInformation, ChartPointInput, ChartRoleInput, ChartSummary, ComparisonSummary,
    PROTOCOL_VERSION, PersonSummary, SaveComparisonRequest, SetWorkspaceRequest,
    WheelOrientationInput, WorkspacePresentation,
};

use crate::{PageHeader, PlatformError, StudioContext, StudioPlatform, new_id};

const DEFAULT_POINTS: [ChartPointInput; 12] = [
    ChartPointInput::Moon,
    ChartPointInput::Sun,
    ChartPointInput::Mercury,
    ChartPointInput::Venus,
    ChartPointInput::Mars,
    ChartPointInput::Jupiter,
    ChartPointInput::Saturn,
    ChartPointInput::Uranus,
    ChartPointInput::Neptune,
    ChartPointInput::Pluto,
    ChartPointInput::Ascendant,
    ChartPointInput::Midheaven,
];

type WorkspaceData = (
    Vec<PersonSummary>,
    Vec<ChartSummary>,
    Vec<ComparisonSummary>,
);

#[component]
pub(crate) fn WorkspacePage() -> impl IntoView {
    let context = expect_context::<StudioContext>();
    let data = RwSignal::new(None::<Result<WorkspaceData, PlatformError>>);
    let presentation = RwSignal::new(None::<Result<Option<WorkspacePresentation>, PlatformError>>);
    let feedback = RwSignal::new(None::<Result<String, String>>);
    let inner_points = RwSignal::new(DEFAULT_POINTS.to_vec());
    let outer_points = RwSignal::new(DEFAULT_POINTS.to_vec());
    let comparison_id =
        RwSignal::new(new_id("comparison").unwrap_or_else(|_| "comparison_new".to_owned()));
    let label_ref = NodeRef::<Input>::new();
    let inner_ref = NodeRef::<Select>::new();
    let outer_ref = NodeRef::<Select>::new();
    let conjunction_ref = NodeRef::<Input>::new();
    let opposition_ref = NodeRef::<Input>::new();
    let square_ref = NodeRef::<Input>::new();
    let trine_ref = NodeRef::<Input>::new();
    let sextile_ref = NodeRef::<Input>::new();
    let orientation_ref = NodeRef::<Select>::new();

    refresh_workspace(Arc::clone(&context.platform), data, presentation);

    let create = {
        let platform = Arc::clone(&context.platform);
        move |event: SubmitEvent| {
            event.prevent_default();
            let id = comparison_id.get_untracked();
            let Some(label) = label_ref.get().map(|input| input.value()) else {
                return;
            };
            let Some(inner_chart_definition_id) = inner_ref.get().map(|input| input.value()) else {
                return;
            };
            let Some(outer_chart_definition_id) = outer_ref.get().map(|input| input.value()) else {
                return;
            };
            if inner_chart_definition_id.is_empty() || outer_chart_definition_id.is_empty() {
                feedback.set(Some(Err(
                    "Choose calculated inner and outer charts.".to_owned()
                )));
                return;
            }
            if inner_points.get_untracked().is_empty() || outer_points.get_untracked().is_empty() {
                feedback.set(Some(Err(
                    "Select at least one point for each chart.".to_owned()
                )));
                return;
            }
            let Some(aspects) = aspect_requests(
                conjunction_ref,
                opposition_ref,
                square_ref,
                trine_ref,
                sextile_ref,
                feedback,
            ) else {
                return;
            };
            let orientation = if orientation_ref
                .get()
                .is_some_and(|input| input.value() == "aries_top")
            {
                WheelOrientationInput::AriesTop
            } else {
                WheelOrientationInput::AscendantLeft
            };
            let request = SaveComparisonRequest {
                protocol_version: PROTOCOL_VERSION,
                id: id.clone(),
                label,
                inner_chart_definition_id: inner_chart_definition_id.clone(),
                outer_chart_definition_id,
                inner_points: inner_points.get_untracked(),
                outer_points: outer_points.get_untracked(),
                aspects,
                orientation,
            };
            let active_person_id =
                data.get_untracked()
                    .and_then(Result::ok)
                    .and_then(|(_, charts, _)| {
                        charts
                            .into_iter()
                            .find(|chart| chart.id == inner_chart_definition_id)
                            .and_then(|chart| chart.person_id)
                    });
            feedback.set(Some(Ok(
                "Saving, calculating, and opening the comparison…".to_owned()
            )));
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(error) = platform.save_comparison(request).await {
                    feedback.set(Some(Err(error.message().to_owned())));
                    return;
                }
                if activate_comparison(
                    Arc::clone(&platform),
                    id,
                    active_person_id,
                    data,
                    presentation,
                    feedback,
                )
                .await
                {
                    if let Ok(id) = new_id("comparison") {
                        comparison_id.set(id);
                    }
                    if let Some(label) = label_ref.get() {
                        label.set_value("");
                    }
                }
            });
        }
    };

    view! {
        <PageHeader eyebrow="Natal + transit" title="Chart workspace" description="Build a comparison from immutable chart calculations, then read one deterministic Rust-rendered biwheel." />
        {move || match presentation.get() {
            None => view! { <section class="panel workspace-loading"><p class="muted">"Loading active workspace…"</p></section> }.into_any(),
            Some(Err(error)) => view! { <p class="error-text" role="alert">{error.message().to_owned()}</p> }.into_any(),
            Some(Ok(None)) => view! { <section class="panel empty-state"><p class="eyebrow">"No active comparison"</p><h2>"The wheel is ready when your charts are."</h2><p>"Calculate a natal and transit chart, then create a preset below."</p></section> }.into_any(),
            Some(Ok(Some(value))) => view! { <WorkspaceChart presentation=value /> }.into_any(),
        }}

        <div class="workspace-builder-layout">
            <form class="panel studio-form comparison-builder" on:submit=create>
                <div class="form-section-heading"><div><p class="eyebrow">"Comparison preset"</p><h2>"Build a biwheel"</h2></div></div>
                <label><span>"Preset label"</span><input node_ref=label_ref required type="text" placeholder="Natal + current transit" /></label>
                {move || match data.get() {
                    None => view! { <p class="muted">"Loading calculated charts…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-text">{error.message().to_owned()}</p> }.into_any(),
                    Some(Ok((_, charts, _))) => {
                        let natal = charts.iter().filter(|chart| chart.role == ChartRoleInput::Natal && chart.current_calculation_id.is_some()).cloned().collect::<Vec<_>>();
                        let outer = charts.iter().filter(|chart| matches!(chart.role, ChartRoleInput::Transit | ChartRoleInput::Event) && chart.current_calculation_id.is_some()).cloned().collect::<Vec<_>>();
                        view! { <div class="form-grid">
                            <label><span>"Inner natal chart"</span><select node_ref=inner_ref><option value="">"Choose natal"</option>{natal.into_iter().map(chart_option).collect_view()}</select></label>
                            <label><span>"Outer transit/event chart"</span><select node_ref=outer_ref><option value="">"Choose transit"</option>{outer.into_iter().map(chart_option).collect_view()}</select></label>
                        </div> }.into_any()
                    }
                }}
                <div class="point-picker-pair">
                    <PointPicker legend="Inner / natal points" selection=inner_points />
                    <PointPicker legend="Outer / transit points" selection=outer_points />
                </div>
                <fieldset class="aspect-editor">
                    <legend>"Aspect orbs"</legend>
                    <div class="aspect-grid">
                        <OrbInput label="Conjunction" value="8" input_ref=conjunction_ref />
                        <OrbInput label="Opposition" value="8" input_ref=opposition_ref />
                        <OrbInput label="Square" value="6" input_ref=square_ref />
                        <OrbInput label="Trine" value="6" input_ref=trine_ref />
                        <OrbInput label="Sextile" value="4" input_ref=sextile_ref />
                    </div>
                </fieldset>
                <label><span>"Wheel orientation"</span><select node_ref=orientation_ref><option value="ascendant_left">"Ascendant left"</option><option value="aries_top">"Aries top"</option></select></label>
                <button class="primary-button" type="submit">"Save, calculate, and open"</button>
            </form>

            <section class="panel preset-panel" aria-labelledby="preset-list-title">
                <p class="eyebrow">"Encrypted presets"</p><h2 id="preset-list-title">"Saved comparisons"</h2>
                {move || match data.get() {
                    None => view! { <p class="muted">"Loading presets…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-text">{error.message().to_owned()}</p> }.into_any(),
                    Some(Ok((_, _charts, comparisons))) if comparisons.is_empty() => view! { <p class="muted">"No comparison presets yet."</p> }.into_any(),
                    Some(Ok((_, charts, comparisons))) => {
                        let platform = Arc::clone(&context.platform);
                        view! { <ul class="preset-list">{comparisons.into_iter().map(|comparison| {
                            let id = comparison.id.clone();
                            let active_person_id = charts.iter().find(|chart| chart.id == comparison.inner_chart_id).and_then(|chart| chart.person_id.clone());
                            let platform = Arc::clone(&platform);
                            view! { <li><div><strong>{comparison.label}</strong><span>{format!("{} inner · {} outer points", comparison.inner_points.len(), comparison.outer_points.len())}</span></div><button class="quiet-button" type="button" on:click=move |_| {
                                feedback.set(Some(Ok("Recalculating and opening the preset…".to_owned())));
                                let platform = Arc::clone(&platform);
                                let id = id.clone();
                                let active_person_id = active_person_id.clone();
                                wasm_bindgen_futures::spawn_local(async move {
                                    let _ = activate_comparison(platform, id, active_person_id, data, presentation, feedback).await;
                                });
                            }>"Recalculate + open"</button></li> }
                        }).collect_view()}</ul> }.into_any()
                    }
                }}
            </section>
        </div>
        <div class="form-feedback" role="status" aria-live="polite">{move || feedback.get().map(|result| match result {
            Ok(message) => view! { <span>{message}</span> }.into_any(),
            Err(message) => view! { <span class="error-text">{message}</span> }.into_any(),
        })}</div>
    }
}

#[component]
fn WorkspaceChart(presentation: WorkspacePresentation) -> impl IntoView {
    let scene = protocol_scene(presentation.scene.clone());
    let orientation = match presentation.orientation {
        WheelOrientationInput::AscendantLeft => WheelOrientation::AscendantLeft,
        WheelOrientationInput::AriesTop => WheelOrientation::ZodiacZeroTop,
    };
    let svg = render_biwheel_svg(&scene, &RenderOptions { orientation });
    let export_href = format!(
        "data:image/svg+xml;base64,{}",
        BASE64.encode(svg.as_bytes())
    );
    let export_name = format!("{}.svg", safe_filename(&presentation.comparison_label));
    view! {
        <section class="workspace-chart" aria-labelledby="active-comparison-title">
            <div class="section-heading"><div><p class="eyebrow">"Active comparison"</p><h2 id="active-comparison-title">{presentation.comparison_label}</h2></div><a class="secondary-button" href=export_href download=export_name>"Export static SVG"</a></div>
            <div class="chart-information-grid">
                <ChartInformationCard lane="Inner · natal" information=presentation.inner />
                <ChartInformationCard lane="Outer · transit" information=presentation.outer />
            </div>
            <div class="biwheel-frame" inner_html=svg></div>
            <p class="chart-legend"><span class="legend-swatch natal"></span>"Inner / natal: filled ticks and solid leaders"<span class="legend-swatch transit"></span>"Outer / transit: hollow ticks and dashed leaders"</p>
        </section>
    }
}

#[component]
fn ChartInformationCard(lane: &'static str, information: ChartInformation) -> impl IntoView {
    let place = if information.administrative_names.is_empty() {
        format!(
            "{} · {}",
            information.location_label, information.country_code
        )
    } else {
        format!(
            "{} · {} · {}",
            information.location_label,
            information.administrative_names.join(", "),
            information.country_code
        )
    };
    view! {
        <article class="panel chart-information">
            <div class="lane-label">{lane}</div>
            <h3>{information.chart_label}</h3>
            <p class="person-role">{information.person_label.unwrap_or_else(|| "No linked person".to_owned())}<span>{role_label(information.role)}</span></p>
            <dl>
                <div><dt>"Local date"</dt><dd>{information.local_date}</dd></div>
                <div><dt>"Local time"</dt><dd>{information.local_time}</dd></div>
                <div><dt>"Zone result"</dt><dd>{format!("{} {}", information.abbreviation, information.utc_offset_display)}</dd></div>
                <div><dt>"Location"</dt><dd>{place}</dd></div>
                <div><dt>"Zodiac"</dt><dd>{information.zodiac}</dd></div>
                <div><dt>"Houses"</dt><dd>{information.house_system}</dd></div>
            </dl>
        </article>
    }
}

#[component]
fn PointPicker(legend: &'static str, selection: RwSignal<Vec<ChartPointInput>>) -> impl IntoView {
    view! {
        <fieldset class="point-picker compact"><legend>{legend}</legend><div class="check-grid">
            {DEFAULT_POINTS.into_iter().map(|point| {
                view! { <label class="check-card"><input type="checkbox" checked=true on:change=move |event| {
                    let checked = event_target_checked(&event);
                    selection.update(|points| {
                        if checked && !points.contains(&point) {
                            points.push(point);
                            points.sort_by_key(|candidate| point_order(*candidate));
                        } else if !checked {
                            points.retain(|candidate| *candidate != point);
                        }
                    });
                } /><span>{point_label(point)}</span></label> }
            }).collect_view()}
        </div></fieldset>
    }
}

#[component]
fn OrbInput(label: &'static str, value: &'static str, input_ref: NodeRef<Input>) -> impl IntoView {
    view! { <label><span>{label}</span><input node_ref=input_ref type="number" min="0" max="15" step="0.25" value=value /><small>"degrees"</small></label> }
}

fn chart_option(chart: ChartSummary) -> impl IntoView {
    view! { <option value=chart.id>{format!("{} · {} {}", chart.label, chart.local_date, chart.local_time)}</option> }
}

async fn activate_comparison(
    platform: Arc<dyn StudioPlatform>,
    comparison_id: String,
    active_person_id: Option<String>,
    data: RwSignal<Option<Result<WorkspaceData, PlatformError>>>,
    presentation: RwSignal<Option<Result<Option<WorkspacePresentation>, PlatformError>>>,
    feedback: RwSignal<Option<Result<String, String>>>,
) -> bool {
    let comparison_artifact_id = match new_id("comparison_artifact") {
        Ok(id) => id,
        Err(error) => {
            feedback.set(Some(Err(error.message().to_owned())));
            return false;
        }
    };
    if let Err(error) = platform
        .calculate_comparison(CalculateComparisonRequest {
            protocol_version: PROTOCOL_VERSION,
            comparison_artifact_id,
            comparison_preset_id: comparison_id.clone(),
        })
        .await
    {
        feedback.set(Some(Err(error.message().to_owned())));
        return false;
    }
    if let Err(error) = platform
        .set_workspace(SetWorkspaceRequest {
            protocol_version: PROTOCOL_VERSION,
            active_person_id,
            active_comparison_id: Some(comparison_id),
        })
        .await
    {
        feedback.set(Some(Err(error.message().to_owned())));
        return false;
    }
    feedback.set(Some(Ok("Active comparison updated.".to_owned())));
    refresh_workspace(platform, data, presentation);
    true
}

fn refresh_workspace(
    platform: Arc<dyn StudioPlatform>,
    data: RwSignal<Option<Result<WorkspaceData, PlatformError>>>,
    presentation: RwSignal<Option<Result<Option<WorkspacePresentation>, PlatformError>>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        presentation.set(Some(platform.workspace_presentation().await));
        let people = match platform.people().await {
            Ok(value) => value,
            Err(error) => {
                data.set(Some(Err(error)));
                return;
            }
        };
        let charts = match platform.charts().await {
            Ok(value) => value,
            Err(error) => {
                data.set(Some(Err(error)));
                return;
            }
        };
        data.set(Some(
            platform
                .comparisons()
                .await
                .map(|comparisons| (people, charts, comparisons)),
        ));
    });
}

fn aspect_requests(
    conjunction: NodeRef<Input>,
    opposition: NodeRef<Input>,
    square: NodeRef<Input>,
    trine: NodeRef<Input>,
    sextile: NodeRef<Input>,
    feedback: RwSignal<Option<Result<String, String>>>,
) -> Option<Vec<AspectDefinitionInput>> {
    let parse = |input: NodeRef<Input>| input.get()?.value().parse::<f64>().ok();
    let Some(values) = [
        parse(conjunction),
        parse(opposition),
        parse(square),
        parse(trine),
        parse(sextile),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>() else {
        feedback.set(Some(Err("Every aspect orb must be a number.".to_owned())));
        return None;
    };
    if values.iter().any(|value| !(0.0..=15.0).contains(value)) {
        feedback.set(Some(Err(
            "Aspect orbs must be between 0 and 15 degrees.".to_owned()
        )));
        return None;
    }
    Some(
        [
            AspectKindInput::Conjunction,
            AspectKindInput::Opposition,
            AspectKindInput::Square,
            AspectKindInput::Trine,
            AspectKindInput::Sextile,
        ]
        .into_iter()
        .zip(values)
        .map(|(kind, orb_degrees)| AspectDefinitionInput { kind, orb_degrees })
        .collect(),
    )
}

fn protocol_scene(scene: BiwheelScene) -> ChartScene {
    ChartScene {
        timestamp: scene.timestamp,
        natal: ChartRing {
            timestamp: scene.natal.timestamp,
            zodiac: scene.natal.zodiac,
            house_system: scene.natal.house_system,
            points: scene.natal.points.into_iter().map(protocol_point).collect(),
            houses: scene.natal.houses,
            ascendant_degrees: scene.natal.ascendant_degrees,
        },
        transit_zodiac: scene.transit_zodiac,
        transit_house_system: scene.transit_house_system,
        transit: scene.transit.into_iter().map(protocol_point).collect(),
        aspects: scene
            .aspects
            .into_iter()
            .map(|aspect| ChartAspect {
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

fn protocol_point(point: oracle_studio_protocol::BiwheelPoint) -> ChartPoint {
    ChartPoint {
        id: point.id,
        longitude_degrees: point.longitude_degrees,
        longitude_speed_degrees_per_day: point.longitude_speed_degrees_per_day,
        retrograde: point.retrograde,
    }
}

fn safe_filename(label: &str) -> String {
    let value = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    value.trim_matches('-').to_owned()
}

fn point_order(point: ChartPointInput) -> usize {
    DEFAULT_POINTS
        .iter()
        .position(|candidate| *candidate == point)
        .unwrap_or(usize::MAX)
}

fn point_label(point: ChartPointInput) -> &'static str {
    match point {
        ChartPointInput::Moon => "Moon",
        ChartPointInput::Sun => "Sun",
        ChartPointInput::Mercury => "Mercury",
        ChartPointInput::Venus => "Venus",
        ChartPointInput::Mars => "Mars",
        ChartPointInput::Jupiter => "Jupiter",
        ChartPointInput::Saturn => "Saturn",
        ChartPointInput::Uranus => "Uranus",
        ChartPointInput::Neptune => "Neptune",
        ChartPointInput::Pluto => "Pluto",
        ChartPointInput::Ascendant => "ASC",
        ChartPointInput::Midheaven => "MC",
        _ => "Point",
    }
}

fn role_label(role: ChartRoleInput) -> &'static str {
    match role {
        ChartRoleInput::Natal => "Natal",
        ChartRoleInput::Event => "Event",
        ChartRoleInput::Transit => "Transit",
    }
}
