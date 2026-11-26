//! Popup selector ported from xcss Select.tsx: native validation and a painted
//! top-layer listbox share one controlled value; all selection state is Rust.
use crate::common::t;
use blog_interface::events::{EventListener, EventListenerOptions};
use blog_interface::prelude::*;
use blog_interface::thread_bound::SendWrapper;
use blog_interface::timers::callback::Timeout;
use blog_interface::{ev, html, svg};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
#[derive(Clone, Copy)]
struct SelectControl {
    label: Signal<String>,
    value: Signal<String>,
    options: Memo<Vec<(String, String)>>,
    change: Callback<String>,
    required: bool,
    disabled: Signal<bool>,
    trigger: NodeRef<html::Button>,
    native: NodeRef<html::Select>,
    panel: NodeRef<html::Span>,
    open: RwSignal<bool>,
    active: RwSignal<usize>,
    reveal: RwSignal<u64>,
    message: RwSignal<String>,
    typed: StoredValue<(String, f64)>,
    id: StoredValue<String>,
}

#[component]
pub fn Select(
    label: Signal<String>,
    value: Signal<String>,
    options: Signal<Vec<(String, String)>>,
    change: Callback<String>,
    #[prop(default = false)] required: bool,
    #[prop(default=Signal::derive(||false))] disabled: Signal<bool>,
) -> impl IntoView {
    let control = SelectControl {
        label,
        value,
        options: Memo::new(move |_| options.get()),
        change,
        required,
        disabled,
        trigger: NodeRef::new(),
        native: NodeRef::new(),
        panel: NodeRef::new(),
        open: RwSignal::new(false),
        active: RwSignal::new(0),
        // Pointer hover paints the active option, but only deliberate navigation
        // reveals it without moving Firefox's click target beneath the pointer.
        reveal: RwSignal::new(0),
        message: RwSignal::new(String::new()),
        typed: StoredValue::new((String::new(), 0.)),
        id: StoredValue::new(format!(
            "blog-select-{}",
            window()
                .crypto()
                .expect("invariant: the application window exposes browser crypto")
                .random_uuid()
        )),
    };
    control.watch_value();
    control.watch_reveal();
    control.watch_popup();
    control.watch_disabled();
    control.watch_reset();
    html::span().class("blog-select").child((
        control.trigger_view(),
        control.native_view(),
        control.panel_view(),
        control.error_view(),
    ))
}

fn viewport(value: Result<JsValue, JsValue>) -> f64 {
    value.ok().and_then(|value| value.as_f64()).unwrap_or(0.)
}

fn event_node(event: &web_sys::Event) -> Option<web_sys::Node> {
    event.target().and_then(|target| target.dyn_into().ok())
}

fn native_disabled(field: &web_sys::HtmlSelectElement) -> bool {
    field
        .matches(":disabled")
        .expect("invariant: :disabled is a valid CSS selector")
}

impl SelectControl {
    fn accepts_input(self) -> bool {
        self.native
            .get()
            .is_some_and(|field| !native_disabled(&field))
    }

    fn dom_id(self, suffix: impl std::fmt::Display) -> String {
        self.id.with_value(|id| format!("{id}-{suffix}"))
    }

    fn reveal_index(self, index: usize) {
        self.active.set(index);
        self.reveal.update(|revision| *revision += 1);
    }

    fn native_value<T: AsRef<str> + Default>(self, value: T) -> T {
        if self.required && value.as_ref() == "0" {
            T::default()
        } else {
            value
        }
    }

    fn selected_index(self, options: &[(String, String)]) -> usize {
        let value = self.value.get_untracked();
        options.iter().position(|(id, _)| *id == value).unwrap_or(0)
    }

    fn close(self) {
        if let Some(p) = self.panel.get()
            && let Err(error) = p.hide_popover()
        {
            web_sys::console::debug_2(&"Selector popup was already detached".into(), &error);
        }
        self.open.set(false);
        self.typed.set_value((String::new(), 0.));
    }

    fn position(self) {
        let (Some(anchor), Some(popup)) = (self.trigger.get(), self.panel.get()) else {
            return;
        };
        let rect = anchor.get_bounding_client_rect();
        let win = window();
        let viewport_width = viewport(win.inner_width());
        let width = rect.width().min(viewport_width - 16.);
        let left = rect.left().min(viewport_width - width - 8.).max(8.);
        let below = (viewport(win.inner_height()) - rect.bottom() - 12.).max(0.);
        let above = (rect.top() - 12.).max(0.);
        let upwards = below < 180f64.min(f64::from(popup.scroll_height())) && above > below;
        let style = popup.unchecked_ref::<web_sys::HtmlElement>().style();
        let set_px = |property, size| {
            style
                .set_property(property, &format!("{size}px"))
                .expect("invariant: the selector popup owns a writable style declaration");
        };
        set_px("width", width);
        set_px("left", left);
        set_px(
            "max-height",
            320f64.min(if upwards { above } else { below }),
        );
        let top = if upwards {
            (rect.top() - 4. - popup.get_bounding_client_rect().height()).max(8.)
        } else {
            rect.bottom() + 4.
        };
        set_px("top", top);
    }

    fn show(self, index: Option<usize>) {
        if !self.accepts_input() {
            return;
        }
        let Some(index) = self.options.with_untracked(|opts| {
            let last = opts.len().checked_sub(1)?;
            Some(index.unwrap_or_else(|| self.selected_index(opts)).min(last))
        }) else {
            return;
        };
        self.active.set(index);
        let Some(panel) = self.panel.get() else {
            return;
        };
        if let Err(error) = panel.show_popover() {
            web_sys::console::warn_2(&"Unable to open the selector popup".into(), &error);
            return;
        }
        self.position();
        self.open.set(true);
        self.reveal.update(|n| *n += 1);
    }

    fn commit(self, index: usize) {
        let Some(field) = self.native.get() else {
            return;
        };
        if native_disabled(&field) {
            self.close();
            return;
        }
        let next = self
            .options
            .with_untracked(|opts| opts.get(index).map(|(next, _)| next.clone()));
        if let Some(next) = next {
            field.set_custom_validity("");
            self.message.set(String::new());
            if self.value.with_untracked(|value| *value != next) {
                field.set_value(self.native_value(next.as_str()));
                self.change.run(next);
            }
        }
        self.close();
    }

    fn watch_value(self) {
        Effect::new(move |_| {
            self.value.track();
            self.options.track();
            if let Some(field) = self.native.get() {
                let selected = self.value.get_untracked();
                field.set_value(self.native_value(selected.as_str()));
                field.set_custom_validity("");
            }
            self.message.set(String::new());
            if self.disabled.get() && self.open.get_untracked() {
                self.close();
            }
            if self.open.get() {
                self.position();
            }
        });
    }

    fn watch_reveal(self) {
        Effect::new(move |_| {
            if !self.open.get() {
                return;
            }
            self.reveal.get();
            if let Some(panel) = self.panel.get()
                && let Some(item) = panel
                    .query_selector(&format!(
                        "[data-option-index=\"{}\"]",
                        self.active.get_untracked()
                    ))
                    .expect("invariant: an integer option index forms a valid CSS selector")
            {
                let rect = item.get_bounding_client_rect();
                let outer = panel.get_bounding_client_rect();
                // Scroll only the listbox, never the document/ancestor fieldset.
                // scrollIntoView may scroll every ancestor, moving the trigger and
                // causing a placement flip while a pointer selection is underway.
                let delta = if rect.top() < outer.top() {
                    rect.top() - outer.top()
                } else {
                    (rect.bottom() - outer.bottom()).max(0.)
                };
                panel.scroll_by_with_x_and_y(0., delta.round());
            }
        });
    }

    fn watch_popup(self) {
        Effect::new(move |_| {
            if !self.open.get() {
                return;
            }
            let outside = move |event: &web_sys::Event| {
                let inside = event_node(event).is_some_and(|target| {
                    self.trigger
                        .get()
                        .is_some_and(|node| node.contains(Some(&target)))
                        || self
                            .panel
                            .get()
                            .is_some_and(|node| node.contains(Some(&target)))
                });
                if !inside {
                    self.close();
                }
            };
            let win = window();
            let doc = win
                .document()
                .expect("invariant: the application window owns its document");
            let capture = EventListenerOptions::run_in_capture_phase();
            let listeners = SendWrapper::new([
                EventListener::new_with_options(&doc, "pointerdown", capture, outside),
                EventListener::new(&doc, "focusin", outside),
                EventListener::new(&win, "resize", move |_| self.position()),
                EventListener::new_with_options(&doc, "scroll", capture, move |event| {
                    // Scrolling options cannot change the anchor's viewport position.
                    if !event_node(event).is_some_and(|target| {
                        self.panel
                            .get()
                            .is_some_and(|panel| panel.contains(Some(&target)))
                    }) {
                        self.position();
                    }
                }),
                EventListener::new(&win, "blur", move |_| self.close()),
            ]);
            on_cleanup(move || drop(listeners));
        });
    }

    fn watch_disabled(self) {
        Effect::new(move |_| {
            if self.open.get()
                && let Some(field) = self.native.get()
            {
                let callback = Closure::<dyn FnMut(js_sys::Array, web_sys::MutationObserver)>::new(
                    move |_, _| {
                        if self
                            .native
                            .get()
                            .is_some_and(|field| native_disabled(&field))
                        {
                            self.close();
                        }
                    },
                );
                let observer = web_sys::MutationObserver::new(callback.as_ref().unchecked_ref())
                    .expect("invariant: a mutation observer callback is callable");
                let opts = web_sys::MutationObserverInit::new();
                opts.set_attributes(true);
                let ancestors =
                    std::iter::successors(field.parent_element(), |node| node.parent_element());
                for parent in ancestors.filter(|parent| parent.tag_name() == "FIELDSET") {
                    observer.observe_with_options(&parent, &opts).expect(
                        "invariant: the selector observes ancestor attributes with valid options",
                    );
                }
                let callback = SendWrapper::new(callback);
                let observer = SendWrapper::new(observer);
                on_cleanup(move || {
                    observer.disconnect();
                    drop(callback);
                });
            }
        });
    }

    fn watch_reset(self) {
        Effect::new(move |_| {
            if let Some(field) = self.native.get()
                && let Some(form) = field.form()
            {
                let listener = SendWrapper::new(EventListener::new(&form, "reset", move |event| {
                    let event = SendWrapper::new(event.clone());
                    let field = SendWrapper::new(field.clone());
                    let selected = self.value.get_untracked();
                    Timeout::new(0, move || {
                        if !event.default_prevented() && field.is_connected() {
                            field.set_custom_validity("");
                            field.set_value(self.native_value(selected.as_str()));
                            self.message.set(String::new());
                            self.close();
                        }
                    })
                    .forget();
                }));
                on_cleanup(move || drop(listener));
            }
        });
    }

    fn keydown(self, event: &web_sys::KeyboardEvent) {
        if !self.accepts_input() {
            return;
        }
        let count = self.options.with_untracked(Vec::len);
        if count == 0 {
            return;
        }
        let key = event.key();
        let open = self.open.get_untracked();
        match key.as_str() {
            "ArrowDown" | "ArrowUp" | "Home" | "End" => {
                event.prevent_default();
                event.stop_propagation();
                if open {
                    let next = match key.as_str() {
                        "Home" => 0,
                        "End" => count - 1,
                        "ArrowDown" => (self.active.get_untracked() + 1).min(count - 1),
                        _ => self.active.get_untracked().saturating_sub(1),
                    };
                    self.reveal_index(next);
                } else {
                    self.show(match key.as_str() {
                        "Home" => Some(0),
                        "End" => Some(count - 1),
                        _ => None,
                    });
                }
            }
            "Enter" | " " => {
                event.prevent_default();
                event.stop_propagation();
                if open {
                    self.commit(self.active.get_untracked());
                } else {
                    self.show(None);
                }
            }
            "Escape" if open => {
                event.prevent_default();
                event.stop_propagation();
                self.close();
            }
            "Tab" if open => self.commit(self.active.get_untracked()),
            _ if !event.ctrl_key()
                && !event.alt_key()
                && !event.meta_key()
                && key.chars().count() == 1 =>
            {
                event.prevent_default();
                self.typeahead(&key, count, open);
            }
            _ => {}
        }
    }

    fn typeahead(self, key: &str, count: usize, open: bool) {
        let now = js_sys::Date::now();
        let (previous, at) = self.typed.get_value();
        let mut text = if now - at < 750. {
            previous
        } else {
            String::new()
        };
        text.extend(key.chars().flat_map(char::to_lowercase));
        let first = text
            .chars()
            .next()
            .expect("invariant: the typeahead key is one character");
        let term = if text.chars().all(|value| value == first) {
            &text[..first.len_utf8()]
        } else {
            text.as_str()
        };
        let offset = usize::from(term.chars().count() == 1);
        let found = self.options.with_untracked(|options| {
            let from = if open {
                self.active.get_untracked()
            } else {
                self.selected_index(options)
            };
            (0..count)
                .map(|n| (from + offset + n) % count)
                .find(|index| options[*index].1.to_lowercase().starts_with(term))
        });
        self.typed.set_value((text, now));
        match (open, found) {
            (true, Some(index)) => self.reveal_index(index),
            (false, found) => self.show(found),
            (true, None) => (),
        }
    }

    fn trigger_view(self) -> impl IntoView {
        html::button()
            .child((
                html::span()
                    .child(move || {
                        let selected = self.value.get();
                        self.options.with(|opts| {
                            opts.iter()
                                .find(|(v, _)| *v == selected)
                                .map_or_else(|| "\u{a0}".into(), |(_, label)| label.clone())
                        })
                    })
                    .class("blog-select-value"),
                svg::svg()
                    .child(svg::path().attr("d", "m9 6 6 6-6 6"))
                    .class("blog-select-arrow")
                    .attr("aria-hidden", "true")
                    .attr("width", "16")
                    .attr("height", "16")
                    .attr("viewBox", "0 0 24 24")
                    .attr("fill", "none")
                    .attr("stroke", "currentColor")
                    .attr("stroke-width", "1.8"),
            ))
            .class("blog-input blog-input--compact blog-select-trigger")
            .node_ref(self.trigger)
            .id(self.dom_id("select"))
            .r#type("button")
            .role("combobox")
            .aria_haspopup("listbox")
            .aria_expanded(move || self.open.get().to_string())
            .aria_controls(self.dom_id("options"))
            .aria_activedescendant(move || {
                self.open
                    .get()
                    .then(|| self.dom_id(format_args!("options-{}", self.active.get())))
            })
            .aria_label(move || self.label.get())
            .aria_describedby(move || {
                (!self.message.get().is_empty()).then(|| self.dom_id("error"))
            })
            .aria_required(self.required.to_string())
            .aria_invalid(move || (!self.message.get().is_empty()).to_string())
            .disabled(move || self.disabled.get())
            .on(ev::click, move |ev| {
                ev.prevent_default();
                if self.open.get_untracked() {
                    self.close();
                } else {
                    self.show(None);
                }
            })
            .on(ev::keydown, move |event| self.keydown(&event))
    }

    fn native_view(self) -> impl IntoView {
        html::select()
            .child(move || {
                self.options
                    .get()
                    .into_iter()
                    .map(|(value, label)| {
                        html::option().child(label).value(self.native_value(value))
                    })
                    .collect_view()
            })
            .class("blog-select-native")
            .node_ref(self.native)
            .id(self.dom_id("native"))
            .aria_hidden("true")
            .tabindex("-1")
            .required(self.required)
            .disabled(move || self.disabled.get())
            .prop("value", move || self.native_value(self.value.get()))
            .on(ev::change, move |ev| {
                self.message.set(String::new());
                let field = event_target::<web_sys::HtmlSelectElement>(&ev);
                field.set_custom_validity("");
                self.change.run(field.value());
            })
            .on(ev::invalid, move |ev: web_sys::Event| {
                ev.prevent_default();
                let field = event_target::<web_sys::HtmlSelectElement>(&ev);
                let m = t("请填写此项。", "Please fill out this field.");
                field.set_custom_validity(&m);
                self.message.set(m);
                self.close();
                let field = SendWrapper::new(field);
                Timeout::new(0, move || {
                    if !field.is_connected() {
                        return;
                    }
                    let first = field.form().and_then(|f| {
                        f.query_selector("input:invalid,select:invalid,textarea:invalid")
                            .expect("invariant: the invalid form control selector is valid CSS")
                    });
                    if first
                        .is_none_or(|e| e == (*field).clone().unchecked_into::<web_sys::Element>())
                        && let Some(trigger) = self.trigger.get()
                        && let Err(error) = trigger.focus()
                    {
                        web_sys::console::warn_2(&"Unable to move browser focus".into(), &error);
                    }
                })
                .forget();
            })
    }

    fn panel_view(self) -> impl IntoView {
        html::span()
            .child(move || {
                self.options
                    .get()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (value, label))| {
                        let selected = move || self.value.with(|current| current == &value);
                        let check = selected.clone();
                        html::span().child(
                            html::span()
                                .child((
                                    html::span().child(label),
                                    html::span()
                                        .aria_hidden("true")
                                        .child(move || if check() { "✓" } else { "" }),
                                ))
                                .class("blog-select-option blog-row blog-row--option")
                                .id(self.dom_id(format_args!("options-{i}")))
                                .role("option")
                                .aria_selected(move || selected().to_string())
                                .attr("data-option-index", i)
                                .attr("data-active", move || {
                                    (self.active.get() == i).then_some("true")
                                })
                                .on(ev::pointermove, move |_| {
                                    self.active.set(i);
                                })
                                .on(ev::click, move |ev| {
                                    ev.prevent_default();
                                    ev.stop_propagation();
                                    self.commit(i);
                                }),
                        )
                    })
                    .collect_view()
            })
            .class("blog-select-panel blog-frame")
            .node_ref(self.panel)
            .id(self.dom_id("options"))
            .popover("manual")
            .role("listbox")
            .aria_label(move || self.label.get())
            .attr("data-blog-select-open", move || {
                self.open.get().then_some("true")
            })
            .on(ev::pointerdown, move |ev| {
                if ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .is_some_and(|t| {
                        t.closest("[role=option]")
                            .expect("invariant: the option role selector is valid CSS")
                            .is_some()
                    })
                {
                    ev.prevent_default();
                }
            })
    }

    fn error_view(self) -> impl IntoView {
        move || {
            let message = self.message.get();
            (!message.is_empty()).then(|| {
                html::span()
                    .child(message)
                    .class("blog-select-error")
                    .id(self.dom_id("error"))
                    .role("alert")
            })
        }
    }
}
