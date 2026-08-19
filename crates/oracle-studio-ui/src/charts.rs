use std::sync::Arc;

use leptos::{
    ev::SubmitEvent,
    html::{Input, Select},
    prelude::*,
};
use leptos_router::{components::A, hooks::use_params_map};
use oracle_studio_protocol::{
    AmbiguousTimeChoiceInput, AyanamsaInput, CalculateChartRequest, CelestialObjectInput,
    ChartPointInput, ChartRoleInput, ChartSummary, HouseSystemInput, LocalTimeResolutionSummary,
    LocationSummary, PROTOCOL_VERSION, PersonSummary, ResolveChartTimeRequest, SaveChartRequest,
    ZodiacInput,
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

type ChartEditorData = (Vec<PersonSummary>, Vec<LocationSummary>, Vec<ChartSummary>);

#[component]
pub(crate) fn ChartEditorPage() -> impl IntoView {
    let context = expect_context::<StudioContext>();
    let params = use_params_map();
    let route_id = params.read().get("id").unwrap_or_else(|| "new".to_owned());
    let data = RwSignal::new(None::<Result<ChartEditorData, PlatformError>>);
    let resolution = RwSignal::new(None::<Result<LocalTimeResolutionSummary, PlatformError>>);
    let feedback = RwSignal::new(None::<Result<String, String>>);
    let editor_id = RwSignal::new(if route_id == "new" {
        new_id("chart").unwrap_or_else(|_| "chart_new".to_owned())
    } else {
        route_id.clone()
    });
    let active_chart_id = RwSignal::new((route_id != "new").then(|| route_id.clone()));
    let selected_points = RwSignal::new(DEFAULT_POINTS.to_vec());

    let id_ref = NodeRef::<Input>::new();
    let label_ref = NodeRef::<Input>::new();
    let role_ref = NodeRef::<Select>::new();
    let person_ref = NodeRef::<Select>::new();
    let date_ref = NodeRef::<Input>::new();
    let time_ref = NodeRef::<Input>::new();
    let zone_ref = NodeRef::<Input>::new();
    let zodiac_ref = NodeRef::<Select>::new();
    let house_ref = NodeRef::<Select>::new();
    let default_ref = NodeRef::<Input>::new();
    let location_ref = NodeRef::<Select>::new();

    load_editor(
        Arc::clone(&context.platform),
        data,
        selected_points,
        editor_id.get_untracked(),
    );

    let save = {
        let platform = Arc::clone(&context.platform);
        move |event: SubmitEvent| {
            event.prevent_default();
            let Some(request) = chart_request(
                id_ref,
                label_ref,
                role_ref,
                person_ref,
                date_ref,
                time_ref,
                zone_ref,
                zodiac_ref,
                house_ref,
                default_ref,
                selected_points.get_untracked(),
                feedback,
            ) else {
                return;
            };
            let chart_id = request.id.clone();
            feedback.set(Some(Ok(
                "Saving the definition and resolving its local time…".to_owned(),
            )));
            resolution.set(None);
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(error) = platform.save_chart(request).await {
                    feedback.set(Some(Err(error.message().to_owned())));
                    return;
                }
                active_chart_id.set(Some(chart_id.clone()));
                match platform
                    .resolve_chart_time(ResolveChartTimeRequest {
                        protocol_version: PROTOCOL_VERSION,
                        chart_definition_id: chart_id.clone(),
                    })
                    .await
                {
                    Ok(value) => {
                        let message = match &value {
                            LocalTimeResolutionSummary::Unique { .. } => {
                                "Definition saved. Confirm the exact instant below.".to_owned()
                            }
                            LocalTimeResolutionSummary::Ambiguous { .. } => {
                                "Definition saved. Choose one of the two valid instants.".to_owned()
                            }
                            LocalTimeResolutionSummary::Nonexistent => {
                                "Definition saved, but this wall time does not exist in that zone. Edit it before calculating.".to_owned()
                            }
                        };
                        resolution.set(Some(Ok(value)));
                        feedback.set(Some(Ok(message)));
                        load_editor(Arc::clone(&platform), data, selected_points, chart_id);
                    }
                    Err(error) => {
                        resolution.set(Some(Err(error.clone())));
                        feedback.set(Some(Err(error.message().to_owned())));
                    }
                }
            });
        }
    };

    let calculate = {
        let platform = Arc::clone(&context.platform);
        move |choice: Option<AmbiguousTimeChoiceInput>| {
            let Some(chart_definition_id) = active_chart_id.get_untracked() else {
                feedback.set(Some(Err("Save the chart definition first.".to_owned())));
                return;
            };
            let Some(location) = location_ref.get().map(|input| input.value()) else {
                return;
            };
            if location.is_empty() {
                feedback.set(Some(Err("Choose a saved location.".to_owned())));
                return;
            }
            let (chart_calculation_id, calculation_artifact_id) =
                match (new_id("calculation"), new_id("artifact")) {
                    (Ok(calculation), Ok(artifact)) => (calculation, artifact),
                    (Err(error), _) | (_, Err(error)) => {
                        feedback.set(Some(Err(error.message().to_owned())));
                        return;
                    }
                };
            feedback.set(Some(Ok(
                "Calculating an immutable Astraeus snapshot…".to_owned()
            )));
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                let request = CalculateChartRequest {
                    protocol_version: PROTOCOL_VERSION,
                    chart_calculation_id,
                    calculation_artifact_id,
                    chart_definition_id: chart_definition_id.clone(),
                    saved_location_id: location,
                    ambiguous_time_choice: choice,
                };
                match platform.calculate_chart(request).await {
                    Ok(_) => {
                        feedback.set(Some(Ok(
                            "Calculation saved. Earlier calculations remain unchanged.".to_owned(),
                        )));
                        load_editor(
                            Arc::clone(&platform),
                            data,
                            selected_points,
                            chart_definition_id,
                        );
                    }
                    Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
                }
            });
        }
    };

    view! {
        <PageHeader eyebrow="Chart definition" title="Chart editor" description="Enter a local civil time, inspect its exact UTC resolution, then create an immutable calculation snapshot." />
        {move || match data.get() {
            None => view! { <p class="muted">"Loading encrypted chart records…"</p> }.into_any(),
            Some(Err(error)) => view! { <p class="error-text" role="alert">{error.message().to_owned()}</p> }.into_any(),
            Some(Ok((people, locations, charts))) => {
                let save = save.clone();
                let calculate = calculate.clone();
                let editor_id = editor_id.get();
                let existing = charts.iter().find(|chart| chart.id == editor_id).cloned();
                let initial_id = editor_id;
                let initial_label = existing.as_ref().map(|chart| chart.label.clone()).unwrap_or_default();
                let initial_date = existing.as_ref().map(|chart| chart.local_date.clone()).unwrap_or_default();
                let initial_time = existing.as_ref().map(|chart| chart.local_time.clone()).unwrap_or_default();
                let initial_zone = existing.as_ref().map(|chart| chart.time_zone.clone()).unwrap_or_else(|| "America/New_York".to_owned());
                let existing_person = existing.as_ref().and_then(|chart| chart.person_id.clone()).unwrap_or_default();
                let existing_role = existing.as_ref().map(|chart| chart.role).unwrap_or(ChartRoleInput::Natal);
                let existing_zodiac = existing.as_ref().map(|chart| chart.zodiac).unwrap_or(ZodiacInput::Tropical);
                let existing_house = existing.as_ref().map(|chart| chart.house_system).unwrap_or(HouseSystemInput::Placidus);
                let existing_default = existing.as_ref().is_some_and(|chart| chart.default_natal);
                let history = existing.map(|chart| chart.calculation_history).unwrap_or_default();
                view! {
                    <form class="panel studio-form chart-editor" on:submit=save>
                        <div class="form-section-heading"><div><p class="eyebrow">"Editable source"</p><h2>"Definition"</h2></div><code>{initial_id.clone()}</code></div>
                        <input node_ref=id_ref type="hidden" value=initial_id />
                        <div class="form-grid">
                            <label><span>"Chart label"</span><input node_ref=label_ref required type="text" value=initial_label /></label>
                            <label><span>"Role"</span><select node_ref=role_ref>
                                <option value="natal" selected=existing_role == ChartRoleInput::Natal>"Natal"</option>
                                <option value="event" selected=existing_role == ChartRoleInput::Event>"Event"</option>
                                <option value="transit" selected=existing_role == ChartRoleInput::Transit>"Transit"</option>
                            </select></label>
                            <label><span>"Person (optional)"</span><select node_ref=person_ref>
                                <option value="">"No person"</option>
                                {people.into_iter().map(|person| {
                                    let selected = person.id == existing_person;
                                    view! { <option value=person.id selected=selected>{person.display_name}</option> }
                                }).collect_view()}
                            </select></label>
                            <label><span>"Local date"</span><input node_ref=date_ref required type="date" value=initial_date /></label>
                            <label><span>"Local time"</span><input node_ref=time_ref required type="time" step="1" value=initial_time /></label>
                            <label><span>"IANA time zone"</span><input node_ref=zone_ref required type="text" value=initial_zone placeholder="America/New_York" /></label>
                            <label><span>"Zodiac"</span><select node_ref=zodiac_ref>
                                <option value="tropical" selected=existing_zodiac == ZodiacInput::Tropical>"Tropical"</option>
                                <option value="sidereal" selected=existing_zodiac == ZodiacInput::Sidereal>"Sidereal (Fagan-Bradley)"</option>
                            </select></label>
                            <label><span>"House system"</span><select node_ref=house_ref>
                                {house_options(existing_house)}
                            </select></label>
                        </div>
                        <fieldset class="point-picker">
                            <legend>"Ordered chart points"</legend>
                            <p>"Only checked points enter calculations and the biwheel. The order below is canonical."</p>
                            <div class="check-grid">
                                {DEFAULT_POINTS.into_iter().map(|point| {
                                    let checked = selected_points.get_untracked().contains(&point);
                                    view! {
                                        <label class="check-card">
                                            <input type="checkbox" checked=checked on:change=move |event| {
                                                let checked = event_target_checked(&event);
                                                selected_points.update(|points| {
                                                    if checked && !points.contains(&point) {
                                                        points.push(point);
                                                        points.sort_by_key(|candidate| point_order(*candidate));
                                                    } else if !checked {
                                                        points.retain(|candidate| *candidate != point);
                                                    }
                                                });
                                            } />
                                            <span>{point_label(point)}</span>
                                        </label>
                                    }
                                }).collect_view()}
                            </div>
                        </fieldset>
                        <label class="check-line"><input node_ref=default_ref type="checkbox" checked=existing_default /><span>"Make this the person’s default natal chart"</span></label>
                        <button class="primary-button" type="submit">"Save and resolve time"</button>
                    </form>

                    <section class="panel calculation-panel" aria-labelledby="calculation-title">
                        <p class="eyebrow">"Immutable result"</p><h2 id="calculation-title">"Resolve and calculate"</h2>
                        <label><span>"Saved location snapshot"</span><select node_ref=location_ref>
                            <option value="">"Choose a location"</option>
                            {locations.into_iter().map(|location| view! {
                                <option value=location.id>{format!("{} · {} · {:.4}, {:.4}", location.label, location.time_zone, location.latitude_degrees, location.longitude_degrees)}</option>
                            }).collect_view()}
                        </select></label>
                        {move || resolution.get().map(|result| match result {
                            Err(error) => view! { <p class="error-text" role="alert">{error.message().to_owned()}</p> }.into_any(),
                            Ok(LocalTimeResolutionSummary::Nonexistent) => view! {
                                <div class="resolution-card invalid"><strong>"Nonexistent local time"</strong><p>"The clocks skipped over this wall time. Studio will not shift it silently."</p></div>
                            }.into_any(),
                            Ok(LocalTimeResolutionSummary::Unique { value }) => {
                                let calculate = calculate.clone();
                                view! { <ResolutionChoice title="Exact instant" value=value on_choose=move || calculate(None) /> }.into_any()
                            }
                            Ok(LocalTimeResolutionSummary::Ambiguous { earlier, later }) => {
                                let earlier_calculate = calculate.clone();
                                let later_calculate = calculate.clone();
                                view! { <div class="resolution-grid"><ResolutionChoice title="Earlier occurrence" value=earlier on_choose=move || earlier_calculate(Some(AmbiguousTimeChoiceInput::Earlier)) /><ResolutionChoice title="Later occurrence" value=later on_choose=move || later_calculate(Some(AmbiguousTimeChoiceInput::Later)) /></div> }.into_any()
                            }
                        })}
                        {if history.is_empty() {
                            view! { <p class="muted">"No calculation history yet."</p> }.into_any()
                        } else {
                            view! { <ol class="history-list calculation-history">{history.into_iter().rev().map(|item| view! {
                                <li><div><strong>{item.location_label}</strong><span>{format!("{} {}", item.abbreviation, item.utc_offset_display)}</span></div><code>{item.utc_instant}</code><small>{format!("Calculated {}", item.calculated_at)}</small></li>
                            }).collect_view()}</ol> }.into_any()
                        }}
                    </section>
                }.into_any()
            }
        }}
        <div class="form-feedback" role="status" aria-live="polite">{move || feedback.get().map(|result| match result {
            Ok(message) => view! { <span>{message}</span> }.into_any(),
            Err(message) => view! { <span class="error-text">{message}</span> }.into_any(),
        })}</div>
        <div class="button-row"><A attr:class="secondary-button" href="/workspace">"Open comparison workspace"</A></div>
    }
}

#[component]
fn ResolutionChoice<F>(
    title: &'static str,
    value: oracle_studio_protocol::ResolvedTimeSummary,
    on_choose: F,
) -> impl IntoView
where
    F: Fn() + 'static,
{
    view! {
        <article class="resolution-card">
            <span>{title}</span>
            <strong>{format!("{} {}", value.abbreviation, value.utc_offset_display)}</strong>
            <code>{value.utc_instant}</code>
            <button class="primary-button" type="button" on:click=move |_| on_choose()>"Calculate this instant"</button>
        </article>
    }
}

fn load_editor(
    platform: Arc<dyn StudioPlatform>,
    data: RwSignal<Option<Result<ChartEditorData, PlatformError>>>,
    selected_points: RwSignal<Vec<ChartPointInput>>,
    route_id: String,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let people = match platform.people().await {
            Ok(value) => value,
            Err(error) => {
                data.set(Some(Err(error)));
                return;
            }
        };
        let locations = match platform.locations().await {
            Ok(value) => value,
            Err(error) => {
                data.set(Some(Err(error)));
                return;
            }
        };
        match platform.charts().await {
            Ok(charts) => {
                if let Some(chart) = charts.iter().find(|chart| chart.id == route_id) {
                    selected_points.set(chart.ordered_points.clone());
                }
                data.set(Some(Ok((people, locations, charts))));
            }
            Err(error) => data.set(Some(Err(error))),
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn chart_request(
    id_ref: NodeRef<Input>,
    label_ref: NodeRef<Input>,
    role_ref: NodeRef<Select>,
    person_ref: NodeRef<Select>,
    date_ref: NodeRef<Input>,
    time_ref: NodeRef<Input>,
    zone_ref: NodeRef<Input>,
    zodiac_ref: NodeRef<Select>,
    house_ref: NodeRef<Select>,
    default_ref: NodeRef<Input>,
    ordered_points: Vec<ChartPointInput>,
    feedback: RwSignal<Option<Result<String, String>>>,
) -> Option<SaveChartRequest> {
    let (id, label, role, person, local_date, local_time, time_zone, zodiac, house, default_natal) = (
        id_ref.get()?.value(),
        label_ref.get()?.value(),
        role_ref.get()?.value(),
        person_ref.get()?.value(),
        date_ref.get()?.value(),
        time_ref.get()?.value(),
        zone_ref.get()?.value(),
        zodiac_ref.get()?.value(),
        house_ref.get()?.value(),
        default_ref.get()?.checked(),
    );
    if ordered_points.is_empty() {
        feedback.set(Some(Err("Select at least one chart point.".to_owned())));
        return None;
    }
    let role = match role.as_str() {
        "event" => ChartRoleInput::Event,
        "transit" => ChartRoleInput::Transit,
        _ => ChartRoleInput::Natal,
    };
    if default_natal && (role != ChartRoleInput::Natal || person.is_empty()) {
        feedback.set(Some(Err(
            "A default natal chart must have the natal role and a person.".to_owned(),
        )));
        return None;
    }
    let zodiac = if zodiac == "sidereal" {
        ZodiacInput::Sidereal
    } else {
        ZodiacInput::Tropical
    };
    Some(SaveChartRequest {
        protocol_version: PROTOCOL_VERSION,
        id,
        label,
        role,
        person_id: (!person.is_empty()).then_some(person),
        local_date,
        local_time,
        time_zone,
        zodiac,
        ayanamsa: (zodiac == ZodiacInput::Sidereal).then_some(AyanamsaInput::FaganBradley),
        house_system: parse_house(&house),
        ordered_objects: ordered_points
            .iter()
            .filter_map(|point| point_object(*point))
            .collect(),
        ordered_points,
        default_natal,
    })
}

fn point_object(point: ChartPointInput) -> Option<CelestialObjectInput> {
    Some(match point {
        ChartPointInput::Moon => CelestialObjectInput::Moon,
        ChartPointInput::Sun => CelestialObjectInput::Sun,
        ChartPointInput::Mercury => CelestialObjectInput::Mercury,
        ChartPointInput::Venus => CelestialObjectInput::Venus,
        ChartPointInput::Mars => CelestialObjectInput::Mars,
        ChartPointInput::Jupiter => CelestialObjectInput::Jupiter,
        ChartPointInput::Saturn => CelestialObjectInput::Saturn,
        ChartPointInput::Uranus => CelestialObjectInput::Uranus,
        ChartPointInput::Neptune => CelestialObjectInput::Neptune,
        ChartPointInput::Pluto => CelestialObjectInput::Pluto,
        ChartPointInput::MeanNode => CelestialObjectInput::MeanNode,
        ChartPointInput::TrueNode => CelestialObjectInput::TrueNode,
        ChartPointInput::Chiron => CelestialObjectInput::Chiron,
        _ => return None,
    })
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

fn point_order(point: ChartPointInput) -> usize {
    DEFAULT_POINTS
        .iter()
        .position(|candidate| *candidate == point)
        .unwrap_or(usize::MAX)
}

fn house_options(selected: HouseSystemInput) -> impl IntoView {
    [
        ("placidus", "Placidus", HouseSystemInput::Placidus),
        ("koch", "Koch", HouseSystemInput::Koch),
        ("porphyry", "Porphyry", HouseSystemInput::Porphyry),
        ("regiomontanus", "Regiomontanus", HouseSystemInput::Regiomontanus),
        ("campanus", "Campanus", HouseSystemInput::Campanus),
        ("equal", "Equal", HouseSystemInput::Equal),
        ("whole_sign", "Whole sign", HouseSystemInput::WholeSign),
    ]
    .into_iter()
    .map(|(value, label, system)| view! { <option value=value selected=selected == system>{label}</option> })
    .collect_view()
}

fn parse_house(value: &str) -> HouseSystemInput {
    match value {
        "koch" => HouseSystemInput::Koch,
        "porphyry" => HouseSystemInput::Porphyry,
        "regiomontanus" => HouseSystemInput::Regiomontanus,
        "campanus" => HouseSystemInput::Campanus,
        "equal" => HouseSystemInput::Equal,
        "whole_sign" => HouseSystemInput::WholeSign,
        _ => HouseSystemInput::Placidus,
    }
}
