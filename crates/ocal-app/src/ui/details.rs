//! Event details panel.
use super::*;

impl Ocal {
    pub(super) fn details(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(e) = self.selected.as_ref() else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        let writable = self.writable(e);
        let color = self.calendar_color(&e.calendar, cx);
        let target = cx.entity().downgrade();
        let action = e.clone();
        let open_target = target.clone();
        let more = menu(
            "details-menu",
            tool_button(
                "details-more",
                IconName::Ellipsis,
                "Event actions",
                "Shift F10",
                cx,
            ),
            self.event_items(e),
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.event_action(action.clone(), index, window, cx)
                });
            },
        )
        .open(self.event_menu)
        .on_open_change(move |open, _, cx| {
            let _ = open_target.update(cx, |app, cx| {
                app.event_menu = *open;
                cx.notify();
            });
        });
        let mut content = div()
            .id("details-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(rems(1.1))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(
                div()
                    .text_size(rems(1.25))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_normal()
                    .child(e.title.clone()),
            )
            .child(detail_line(
                IconName::Clock,
                format!(
                    "{}\n{}",
                    e.dates_in(self.tz()).0.format("%A, %-d %B %Y"),
                    e.time_label(self.tz())
                ),
                cx,
            ))
            .child(detail_line(
                IconName::Calendar,
                self.snapshot
                    .calendars
                    .iter()
                    .find(|c| c.id == e.calendar)
                    .map(|c| c.display_name())
                    .unwrap_or_default(),
                cx,
            ))
            .child(detail_line(IconName::Globe, e.timezone.clone(), cx));
        if !e.location.is_empty() {
            content = content.child(detail_line(IconName::MapPin, e.location.clone(), cx));
        }
        if !e.link.is_empty() {
            let link = e.link.clone();
            content = content.child(
                action_button(
                    "join-meeting",
                    IconName::Video,
                    "Join meeting",
                    ButtonVariant::Primary,
                    cx,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    match ocal_core::safe_url(&link) {
                        Ok(url) => {
                            if let Err(error) = std::process::Command::new("xdg-open")
                                .arg(url.as_str())
                                .spawn()
                            {
                                app.status = error.to_string();
                                app.error = true;
                            }
                        }
                        Err(error) => {
                            app.status = error.to_string();
                            app.error = true;
                        }
                    }
                    cx.notify();
                })),
            );
        } else if e.conference_status == "pending" {
            content = content.child(detail_line(
                IconName::Video,
                "Google Meet link is being generated. It will appear here automatically.".into(),
                cx,
            ));
        } else if e.conference_status == "failure" {
            content = content.child(
                div()
                    .text_color(t.danger)
                    .child("Google could not generate this Meet link."),
            );
        }
        if !e.notes.is_empty() {
            content = content.child(
                div()
                    .text_size(rems(0.78))
                    .whitespace_normal()
                    .child(e.notes.clone()),
            );
        }
        if e.series_id.is_some() || e.repeat != Repeat::Never {
            content = content.child(detail_line(
                IconName::Repeat,
                if e.remote_id.is_some() {
                    "Part of a recurring event"
                } else {
                    "Recurring series"
                }
                .into(),
                cx,
            ));
        }
        for attendee in &e.attendees {
            content = content.child(
                div()
                    .text_size(rems(0.72))
                    .whitespace_normal()
                    .child(format!("{} · {}", attendee.email, attendee.response)),
            );
        }
        if e.remote_id.is_some() && e.attendees.iter().any(|a| a.me) {
            let ev = e.clone();
            content = content.child(
                div().flex().gap(rems(0.3)).children(
                    [
                        ("Yes", "accepted"),
                        ("Maybe", "tentative"),
                        ("No", "declined"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, response))| {
                        let e = ev.clone();
                        button(("rsvp", i), label, ButtonVariant::Secondary, cx)
                            .disabled(self.busy)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.busy = true;
                                app.dispatch(Command::Respond(e.clone(), response.into()));
                                cx.notify();
                            }))
                    }),
                ),
            );
        }
        let extra = f32::from(!e.location.is_empty()) * 2.5
            + f32::from(!e.link.is_empty() || !e.conference_status.is_empty()) * 3.
            + f32::from(e.repeat != Repeat::Never || e.series_id.is_some()) * 2.
            + e.attendees.len() as f32 * 1.8
            + f32::from(e.attendees.iter().any(|a| a.me)) * 3.
            + if e.notes.is_empty() {
                0.
            } else {
                2. + e.notes.chars().count() as f32 / 40. * 1.3
            };
        let edit = e.clone();
        let delete = e.clone();
        let footer = if self.delete_confirm {
            div()
                .p(rems(1.))
                .flex()
                .flex_col()
                .gap(rems(0.5))
                .child(div().text_size(rems(0.74)).whitespace_normal().child(
                    if e.series_id.is_some() && e.remote_id.is_none() {
                        "Delete every event in this series?"
                    } else {
                        "Delete this event?"
                    },
                ))
                .child(
                    div()
                        .flex()
                        .gap(rems(0.5))
                        .child(
                            button("confirm-delete", "Delete", ButtonVariant::Primary, cx)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.busy = true;
                                    app.dispatch(Command::Delete(delete.clone()));
                                    cx.notify();
                                })),
                        )
                        .child(
                            button("cancel-delete", "Cancel", ButtonVariant::Secondary, cx)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.delete_confirm = false;
                                    cx.notify();
                                })),
                        ),
                )
        } else {
            div().p(rems(1.)).child(
                action_button(
                    "edit-event",
                    IconName::PencilLine,
                    "Edit",
                    ButtonVariant::Primary,
                    cx,
                )
                .disabled(!writable || self.busy)
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_editor(edit.clone(), true, window, cx)
                })),
            )
        };
        div()
            .id("event-details")
            .debug_selector(|| "event-details".into())
            .role(gpui_kit::Role::Dialog)
            .aria_label("Event details")
            .absolute()
            .right(px(12.))
            .top(rems(4.5))
            .w(rems(25.))
            .max_w(window.viewport_size().width - px(24.))
            .h(
                (window.viewport_size().height - rems(6.).to_pixels(window.rem_size()))
                    .min(rems((18. + extra).min(34.)).to_pixels(window.rem_size())),
            )
            .flex()
            .flex_col()
            .border_1()
            .border_t_2()
            .border_color(color)
            .bg(t.background)
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .px(rems(0.7))
                    .py(rems(0.45))
                    .flex()
                    .justify_end()
                    .gap(rems(0.3))
                    .child(more)
                    .child(
                        tool_button("close-details", IconName::X, "Close details", "Esc", cx)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.selected = None;
                                app.event_menu = false;
                                window.focus(&app.focus, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(content)
            .child(footer.border_t_1().border_color(t.divider()))
            .into_any_element()
    }
}
pub(super) fn detail_line(glyph: IconName, text: String, cx: &App) -> AnyElement {
    div()
        .flex()
        .gap(rems(0.7))
        .items_start()
        .child(
            icon(glyph)
                .size(rems(0.95))
                .text_color(cx.omarchy().secondary),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(rems(0.78))
                .whitespace_normal()
                .child(text),
        )
        .into_any_element()
}
