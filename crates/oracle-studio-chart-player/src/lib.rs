//! Browser controller for self-contained Oracle Studio chart timelines.

#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::{cell::RefCell, rc::Rc};

    use chrono::{DateTime, FixedOffset, Utc};
    use oracle_studio_chart_view::{
        RenderOptions, TransitTimeline, WheelOrientation, render_biwheel_svg,
    };
    use serde::Deserialize;
    use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
    use web_sys::{
        Document, Element, Event, HtmlButtonElement, HtmlInputElement, HtmlSelectElement, Window,
    };

    type AnimationCallback = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PlayerData {
        schema_version: u32,
        orientation: WheelOrientation,
        transit_offset_seconds: i32,
        timeline: TransitTimeline,
    }

    struct PlayerState {
        timeline: TransitTimeline,
        orientation: WheelOrientation,
        transit_offset_seconds: i32,
        frame_times: Vec<i64>,
        first_ms: i64,
        last_ms: i64,
        current_ms: f64,
        direction: f64,
        playing: bool,
        previous_animation_ms: Option<f64>,
        natal_visible: bool,
        transit_visible: bool,
        aspects_visible: bool,
    }

    #[wasm_bindgen(start)]
    pub fn start() -> Result<(), JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("window unavailable"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("document unavailable"))?;
        let data_element = required(&document, "oracle-timeline")?;
        let source = data_element
            .text_content()
            .ok_or_else(|| JsValue::from_str("timeline data is empty"))?;
        let data: PlayerData =
            serde_json::from_str(&source).map_err(|error| JsValue::from_str(&error.to_string()))?;
        if data.schema_version != 2 {
            return Err(JsValue::from_str("unsupported chart-player schema"));
        }
        let frame_times = data
            .timeline
            .frames
            .iter()
            .map(|frame| timestamp_millis(&frame.timestamp))
            .collect::<Result<Vec<_>, _>>()?;
        let first_ms = *frame_times
            .first()
            .ok_or_else(|| JsValue::from_str("timeline has no frames"))?;
        let last_ms = *frame_times
            .last()
            .ok_or_else(|| JsValue::from_str("timeline has no frames"))?;
        let state = Rc::new(RefCell::new(PlayerState {
            timeline: data.timeline,
            orientation: data.orientation,
            transit_offset_seconds: data.transit_offset_seconds,
            frame_times,
            first_ms,
            last_ms,
            current_ms: first_ms as f64,
            direction: 1.0,
            playing: false,
            previous_animation_ms: None,
            natal_visible: true,
            transit_visible: true,
            aspects_visible: true,
        }));

        let scrubber: HtmlInputElement = required(&document, "scrubber")?.dyn_into()?;
        scrubber.set_min(&first_ms.to_string());
        scrubber.set_max(&last_ms.to_string());
        scrubber.set_value(&first_ms.to_string());

        install_scrubber(&document, &state)?;
        install_direction_button(&document, &state, "reverse", -1.0)?;
        install_direction_button(&document, &state, "forward", 1.0)?;
        install_exact_step(&document, &state, "previous-frame", -1)?;
        install_exact_step(&document, &state, "next-frame", 1)?;
        install_visibility(&document, &state, "toggle-natal", Visibility::Natal)?;
        install_visibility(&document, &state, "toggle-transit", Visibility::Transit)?;
        install_visibility(&document, &state, "toggle-aspects", Visibility::Aspects)?;
        install_playback(&window, &document, &state)?;
        render(&document, &state)?;
        Ok(())
    }

    fn install_scrubber(
        document: &Document,
        state: &Rc<RefCell<PlayerState>>,
    ) -> Result<(), JsValue> {
        let scrubber: HtmlInputElement = required(document, "scrubber")?.dyn_into()?;
        let state = Rc::clone(state);
        let document = document.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(target) = event
                .target()
                .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
            else {
                return;
            };
            if let Ok(value) = target.value().parse::<f64>() {
                state.borrow_mut().current_ms = value;
                let _ = render(&document, &state);
            }
        });
        scrubber.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref())?;
        closure.forget();
        Ok(())
    }

    fn install_direction_button(
        document: &Document,
        state: &Rc<RefCell<PlayerState>>,
        id: &str,
        direction: f64,
    ) -> Result<(), JsValue> {
        let button: HtmlButtonElement = required(document, id)?.dyn_into()?;
        let state = Rc::clone(state);
        let document = document.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            state.borrow_mut().direction = direction;
            let _ = update_transport(&document, &state.borrow());
        });
        button.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
        Ok(())
    }

    fn install_exact_step(
        document: &Document,
        state: &Rc<RefCell<PlayerState>>,
        id: &str,
        direction: i8,
    ) -> Result<(), JsValue> {
        let button: HtmlButtonElement = required(document, id)?.dyn_into()?;
        let state = Rc::clone(state);
        let document = document.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let next = {
                let state = state.borrow();
                if direction > 0 {
                    state
                        .frame_times
                        .iter()
                        .copied()
                        .find(|value| (*value as f64) > state.current_ms)
                        .unwrap_or(state.last_ms)
                } else {
                    state
                        .frame_times
                        .iter()
                        .rev()
                        .copied()
                        .find(|value| (*value as f64) < state.current_ms)
                        .unwrap_or(state.first_ms)
                }
            };
            state.borrow_mut().current_ms = next as f64;
            let _ = render(&document, &state);
        });
        button.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
        Ok(())
    }

    #[derive(Clone, Copy)]
    enum Visibility {
        Natal,
        Transit,
        Aspects,
    }

    fn install_visibility(
        document: &Document,
        state: &Rc<RefCell<PlayerState>>,
        id: &str,
        visibility: Visibility,
    ) -> Result<(), JsValue> {
        let input: HtmlInputElement = required(document, id)?.dyn_into()?;
        let state = Rc::clone(state);
        let document = document.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(target) = event
                .target()
                .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
            else {
                return;
            };
            let mut borrowed = state.borrow_mut();
            match visibility {
                Visibility::Natal => borrowed.natal_visible = target.checked(),
                Visibility::Transit => borrowed.transit_visible = target.checked(),
                Visibility::Aspects => borrowed.aspects_visible = target.checked(),
            }
            drop(borrowed);
            let _ = apply_visibility(&document, &state.borrow());
        });
        input.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())?;
        closure.forget();
        Ok(())
    }

    fn install_playback(
        window: &Window,
        document: &Document,
        state: &Rc<RefCell<PlayerState>>,
    ) -> Result<(), JsValue> {
        let callback: AnimationCallback = Rc::new(RefCell::new(None));
        let callback_for_frame = Rc::clone(&callback);
        let state_for_frame = Rc::clone(state);
        let document_for_frame = document.clone();
        let window_for_frame = window.clone();
        *callback.borrow_mut() = Some(Closure::<dyn FnMut(f64)>::new(move |animation_ms| {
            let mut should_continue = false;
            {
                let mut state = state_for_frame.borrow_mut();
                if state.playing {
                    if let Some(previous) = state.previous_animation_ms {
                        let rate = required(&document_for_frame, "playback-rate")
                            .ok()
                            .and_then(|element| element.dyn_into::<HtmlSelectElement>().ok())
                            .and_then(|select| select.value().parse::<f64>().ok())
                            .unwrap_or(3600.0);
                        let elapsed_seconds = (animation_ms - previous) / 1000.0;
                        state.current_ms += elapsed_seconds * rate * 1000.0 * state.direction;
                        state.current_ms = state
                            .current_ms
                            .clamp(state.first_ms as f64, state.last_ms as f64);
                        if state.current_ms == state.first_ms as f64
                            || state.current_ms == state.last_ms as f64
                        {
                            state.playing = false;
                        }
                    }
                    state.previous_animation_ms = Some(animation_ms);
                    should_continue = state.playing;
                }
            }
            let _ = render(&document_for_frame, &state_for_frame);
            let _ = update_transport(&document_for_frame, &state_for_frame.borrow());
            if should_continue && let Some(callback) = callback_for_frame.borrow().as_ref() {
                let _ = window_for_frame.request_animation_frame(callback.as_ref().unchecked_ref());
            }
        }));

        let button: HtmlButtonElement = required(document, "play-pause")?.dyn_into()?;
        let state_for_click = Rc::clone(state);
        let document_for_click = document.clone();
        let window_for_click = window.clone();
        let callback_for_click = Rc::clone(&callback);
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let playing = {
                let mut state = state_for_click.borrow_mut();
                state.playing = !state.playing;
                state.previous_animation_ms = None;
                state.playing
            };
            let _ = update_transport(&document_for_click, &state_for_click.borrow());
            if playing && let Some(callback) = callback_for_click.borrow().as_ref() {
                let _ = window_for_click.request_animation_frame(callback.as_ref().unchecked_ref());
            }
        });
        button.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
        Ok(())
    }

    fn render(document: &Document, state: &Rc<RefCell<PlayerState>>) -> Result<(), JsValue> {
        let mut state = state.borrow_mut();
        state.current_ms = state
            .current_ms
            .clamp(state.first_ms as f64, state.last_ms as f64);
        let instant = DateTime::<Utc>::from_timestamp_millis(state.current_ms.round() as i64)
            .ok_or_else(|| JsValue::from_str("timeline instant is out of range"))?;
        let scene = state.timeline.scene_at(instant);
        let svg = render_biwheel_svg(
            &scene,
            &RenderOptions {
                orientation: state.orientation,
                ..RenderOptions::default()
            },
        );
        required(document, "chart-stage")?.set_inner_html(&svg);
        apply_visibility(document, &state)?;

        let scrubber: HtmlInputElement = required(document, "scrubber")?.dyn_into()?;
        scrubber.set_value(&(state.current_ms.round() as i64).to_string());
        let timestamp = required(document, "timestamp")?;
        timestamp.set_text_content(Some(&scene.timestamp));
        timestamp.set_attribute("value", &scene.timestamp)?;

        let local = instant.with_timezone(
            &FixedOffset::east_opt(state.transit_offset_seconds)
                .ok_or_else(|| JsValue::from_str("invalid transit offset"))?,
        );
        let chart_datetime = required(document, "transit-chart-datetime")?;
        chart_datetime.set_attribute("datetime", &local.to_rfc3339())?;
        chart_datetime
            .set_text_content(Some(&local.format("%a, %b %d, %Y · %H:%M %:z").to_string()));
        Ok(())
    }

    fn apply_visibility(document: &Document, state: &PlayerState) -> Result<(), JsValue> {
        set_display(document, "natal-structure-layer", state.natal_visible)?;
        set_display(document, "natal-layer", state.natal_visible)?;
        set_display(document, "transit-layer", state.transit_visible)?;
        set_display(document, "aspect-layer", state.aspects_visible)?;
        Ok(())
    }

    fn set_display(document: &Document, id: &str, visible: bool) -> Result<(), JsValue> {
        if let Some(element) = document.get_element_by_id(id) {
            if visible {
                element.remove_attribute("style")?;
            } else {
                element.set_attribute("style", "display:none")?;
            }
        }
        Ok(())
    }

    fn update_transport(document: &Document, state: &PlayerState) -> Result<(), JsValue> {
        let reverse = required(document, "reverse")?;
        reverse.set_attribute(
            "aria-pressed",
            if state.direction < 0.0 {
                "true"
            } else {
                "false"
            },
        )?;
        let forward = required(document, "forward")?;
        forward.set_attribute(
            "aria-pressed",
            if state.direction > 0.0 {
                "true"
            } else {
                "false"
            },
        )?;
        let play: HtmlButtonElement = required(document, "play-pause")?.dyn_into()?;
        play.set_attribute("aria-pressed", if state.playing { "true" } else { "false" })?;
        play.set_text_content(Some(if state.playing { "Pause" } else { "Play" }));
        Ok(())
    }

    fn timestamp_millis(value: &str) -> Result<i64, JsValue> {
        DateTime::parse_from_rfc3339(value)
            .map(|value| value.timestamp_millis())
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    fn required(document: &Document, id: &str) -> Result<Element, JsValue> {
        document
            .get_element_by_id(id)
            .ok_or_else(|| JsValue::from_str(&format!("missing element #{id}")))
    }
}
