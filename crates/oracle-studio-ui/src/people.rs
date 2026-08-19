use std::sync::Arc;

use leptos::{
    ev::SubmitEvent,
    html::{Input, Select, Textarea},
    prelude::*,
};
use leptos_router::{components::A, hooks::use_params_map};
use oracle_studio_protocol::{
    ChartRoleInput, ChartSummary, PROTOCOL_VERSION, PersonKindInput, PersonSummary,
    SavePersonRequest,
};

use crate::{PageHeader, PlatformError, StudioContext, StudioPlatform};

#[component]
pub(crate) fn PeoplePage() -> impl IntoView {
    let context = expect_context::<StudioContext>();
    let people = RwSignal::new(None::<Result<Vec<PersonSummary>, PlatformError>>);
    let feedback = RwSignal::new(None::<Result<String, String>>);
    let id_ref = NodeRef::<Input>::new();
    let name_ref = NodeRef::<Input>::new();
    let kind_ref = NodeRef::<Select>::new();
    let notes_ref = NodeRef::<Textarea>::new();
    refresh_people(Arc::clone(&context.platform), people);

    let submit = {
        let platform = Arc::clone(&context.platform);
        move |event: SubmitEvent| {
            event.prevent_default();
            let (Some(id), Some(name), Some(kind), Some(notes)) = (
                id_ref.get(),
                name_ref.get(),
                kind_ref.get(),
                notes_ref.get(),
            ) else {
                return;
            };
            let request = SavePersonRequest {
                protocol_version: PROTOCOL_VERSION,
                id: id.value(),
                display_name: name.value(),
                kind: if kind.value() == "professional_client" {
                    PersonKindInput::ProfessionalClient
                } else {
                    PersonKindInput::Personal
                },
                notes: (!notes.value().trim().is_empty()).then(|| notes.value()),
            };
            feedback.set(Some(Ok("Saving encrypted person record…".to_owned())));
            let platform = Arc::clone(&platform);
            wasm_bindgen_futures::spawn_local(async move {
                match platform.save_person(request).await {
                    Ok(_) => {
                        id.set_value("");
                        name.set_value("");
                        notes.set_value("");
                        feedback.set(Some(Ok("Person saved.".to_owned())));
                        refresh_people(Arc::clone(&platform), people);
                    }
                    Err(error) => feedback.set(Some(Err(error.message().to_owned()))),
                }
            });
        }
    };

    view! {
        <PageHeader eyebrow="Encrypted records" title="People" description="Create a person, attach natal charts, and keep one explicit default natal chart per person." />
        <div class="people-layout">
            <form class="panel studio-form" on:submit=submit>
                <p class="eyebrow">"New or updated record"</p>
                <h2>"Person details"</h2>
                <label><span>"Record ID"</span><input node_ref=id_ref required type="text" placeholder="emmy" /></label>
                <label><span>"Display name"</span><input node_ref=name_ref required type="text" autocomplete="name" /></label>
                <label>
                    <span>"Record type"</span>
                    <select node_ref=kind_ref>
                        <option value="personal">"Personal"</option>
                        <option value="professional_client">"Professional client"</option>
                    </select>
                </label>
                <label><span>"Notes (optional)"</span><textarea node_ref=notes_ref rows="4"></textarea></label>
                <button class="primary-button" type="submit">"Save person"</button>
            </form>
            <section class="panel record-panel" aria-labelledby="people-list-title">
                <p class="eyebrow">"Vault directory"</p>
                <h2 id="people-list-title">"Saved people"</h2>
                {move || match people.get() {
                    None => view! { <p class="muted">"Loading people…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-text">{error.message().to_owned()}</p> }.into_any(),
                    Some(Ok(items)) if items.is_empty() => view! { <p class="muted">"No people are saved in this vault yet."</p> }.into_any(),
                    Some(Ok(items)) => view! {
                        <ul class="record-list">
                            {items.into_iter().map(|person| {
                                let href = format!("/people/{}", person.id);
                                view! {
                                    <li>
                                        <div><strong>{person.display_name}</strong><small>{person_kind(&person.kind)}</small></div>
                                        <A attr:class="quiet-button" href=href>"Open"</A>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                }}
            </section>
        </div>
        <div class="form-feedback" role="status" aria-live="polite">
            {move || feedback.get().map(|result| match result {
                Ok(message) => view! { <span>{message}</span> }.into_any(),
                Err(message) => view! { <span class="error-text">{message}</span> }.into_any(),
            })}
        </div>
    }
}

#[component]
pub(crate) fn PersonPage() -> impl IntoView {
    let context = expect_context::<StudioContext>();
    let params = use_params_map();
    let person_id = params.read().get("id").unwrap_or_default();
    let data =
        RwSignal::new(None::<Result<(Vec<PersonSummary>, Vec<ChartSummary>), PlatformError>>);
    let platform = Arc::clone(&context.platform);
    wasm_bindgen_futures::spawn_local(async move {
        let people = match platform.people().await {
            Ok(people) => people,
            Err(error) => {
                data.set(Some(Err(error)));
                return;
            }
        };
        data.set(Some(platform.charts().await.map(|charts| (people, charts))));
    });

    view! {
        {move || match data.get() {
            None => view! { <p class="muted">"Loading person and chart history…"</p> }.into_any(),
            Some(Err(error)) => view! { <p class="error-text" role="alert">{error.message().to_owned()}</p> }.into_any(),
            Some(Ok((people, charts))) => {
                let Some(person) = people.into_iter().find(|person| person.id == person_id) else {
                    return view! { <section class="panel empty-state"><h1>"Person not found"</h1><A attr:class="secondary-button" href="/people">"Back to people"</A></section> }.into_any();
                };
                let associated = charts.into_iter().filter(|chart| chart.person_id.as_deref() == Some(person.id.as_str())).collect::<Vec<_>>();
                view! {
                    <PageHeader eyebrow="Person detail" title=person.display_name.clone() description="Natal definitions, event/transit work, and immutable calculation history for this encrypted record." />
                    <section class="panel person-summary">
                        <div><span>"Type"</span><strong>{person_kind(&person.kind)}</strong></div>
                        <div><span>"Record ID"</span><code>{person.id.clone()}</code></div>
                        <p>{person.notes.unwrap_or_else(|| "No private notes recorded.".to_owned())}</p>
                    </section>
                    <div class="section-heading"><div><p class="eyebrow">"Definitions + history"</p><h2>"Charts"</h2></div><A attr:class="primary-button" href="/charts/new">"New chart"</A></div>
                    {if associated.is_empty() {
                        view! { <section class="panel empty-state"><h2>"No charts yet"</h2><p>"Create a natal, event, or transit definition for this person."</p></section> }.into_any()
                    } else {
                        view! { <div class="chart-card-grid">{associated.into_iter().map(chart_card).collect_view()}</div> }.into_any()
                    }}
                }.into_any()
            }
        }}
    }
}

fn chart_card(chart: ChartSummary) -> impl IntoView {
    let href = format!("/charts/{}", chart.id);
    let history = chart.calculation_history;
    view! {
        <article class="panel chart-card">
            <div class="card-kicker"><span>{role_label(chart.role)}</span>{chart.default_natal.then(|| view! { <span class="badge">"Default natal"</span> })}</div>
            <h3>{chart.label}</h3>
            <p>{format!("{} · {} · {}", chart.local_date, chart.local_time, chart.time_zone)}</p>
            {if history.is_empty() {
                view! { <p class="muted">"Not calculated yet."</p> }.into_any()
            } else {
                view! {
                    <ol class="history-list">
                        {history.into_iter().rev().map(|item| view! {
                            <li><strong>{item.location_label}</strong><span>{format!("{} {} · {}", item.abbreviation, item.utc_offset_display, item.utc_instant)}</span></li>
                        }).collect_view()}
                    </ol>
                }.into_any()
            }}
            <A attr:class="secondary-button" href=href>"Edit or calculate"</A>
        </article>
    }
}

fn refresh_people(
    platform: Arc<dyn StudioPlatform>,
    people: RwSignal<Option<Result<Vec<PersonSummary>, PlatformError>>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        people.set(Some(platform.people().await));
    });
}

fn person_kind(kind: &str) -> &'static str {
    if kind == "professional_client" {
        "Professional client"
    } else {
        "Personal"
    }
}

fn role_label(role: ChartRoleInput) -> &'static str {
    match role {
        ChartRoleInput::Natal => "Natal",
        ChartRoleInput::Event => "Event",
        ChartRoleInput::Transit => "Transit",
    }
}
