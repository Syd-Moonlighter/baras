//! Reusable labeled slider with an editable numeric value.
//!
//! Combines a range slider and a number input that both drive the same
//! `on_change` callback, plus an optional static unit suffix (e.g. "px", "%").
//! Values are expressed in display space (f64); callers convert to their
//! stored representation inside `on_change`.

use dioxus::prelude::*;

#[component]
pub fn Slider(
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 1.0)] step: f64,
    #[props(default = "")] suffix: &'static str,
    #[props(default = "")] tooltip: &'static str,
    #[props(default = false)] disabled: bool,
    on_change: EventHandler<f64>,
) -> Element {
    rsx! {
        div { class: "setting-row",
            label {
                "{label}"
                if !tooltip.is_empty() {
                    span { class: "help-icon", title: "{tooltip}", "?" }
                }
            }
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                disabled: disabled,
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        on_change.call(v.clamp(min, max));
                    }
                },
            }
            div { class: "slider-number",
                input {
                    r#type: "number",
                    class: "slider-value-input",
                    min: "{min}",
                    max: "{max}",
                    step: "any",
                    value: "{value}",
                    disabled: disabled,
                    onchange: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            on_change.call(v.clamp(min, max));
                        }
                    },
                }
                div { class: "slider-steppers",
                    button {
                        r#type: "button",
                        class: "slider-step",
                        tabindex: "-1",
                        disabled: disabled,
                        onclick: move |_| on_change.call((value + step).clamp(min, max)),
                        i { class: "fa-solid fa-caret-up" }
                    }
                    button {
                        r#type: "button",
                        class: "slider-step",
                        tabindex: "-1",
                        disabled: disabled,
                        onclick: move |_| on_change.call((value - step).clamp(min, max)),
                        i { class: "fa-solid fa-caret-down" }
                    }
                }
            }
            if !suffix.is_empty() {
                span { class: "slider-unit", "{suffix}" }
            }
        }
    }
}
