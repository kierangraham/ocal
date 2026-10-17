//! Event details panel.
use super::*;
use crate::appearance::section_label;

/// "in 25 min", "Now · 40 min left" or "Ended", relative to `now`.
pub(super) fn relative_time(e: &Event, now: chrono::DateTime<Utc>) -> Option<String> {
    if e.all_day.is_some() || e.task {
        return None;
    }
    let span = |d: Duration| {
        let m = d.num_minutes().max(1);
        if m < 60 {
            format!("{m} min")
        } else if m < 48 * 60 {
            format!("{} h", (m + 30) / 60)
        } else {
            format!("{} days", (m + 720) / 1440)
        }
    };
    Some(if now < e.start {
        format!("in {}", span(e.start - now))
    } else if now < e.end {
        format!("Now · {} left", span(e.end - now))
    } else {
        "Ended".into()
    })
}

impl Ocal {
    pub(super) fn details(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(e) = self.selected.as_ref() else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        let tz = self.tz();
        let writable = self.writable(e);
        let color = self.calendar_color(&e.calendar, cx);
        let calendar_name = self
            .snapshot
            .calendars
            .iter()
            .find(|c| c.id == e.calendar)
            .map(|c| c.display_name())
            .unwrap_or_default();
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
        let copy = |id: &'static str, text: String, what: &'static str, cx: &mut Context<Self>| {
            tool_button(id, IconName::Copy, what, "", cx).on_click(cx.listener(
                move |app, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    app.toast(format!("{what} copied"), cx);
                },
            ))
        };
        let (day, _) = e.dates_in(tz);
        let mut when = div()
            .flex()
            .flex_col()
            .gap(rems(0.15))
            .child(
                div()
                    .text_color(t.foreground)
                    .child(day.format("%A %-d %B %Y").to_string()),
            )
            .child(div().text_color(t.secondary).child(e.time_label(tz)));
        if let Some(hint) = relative_time(e, Utc::now()) {
            when = when.child(
                div()
                    .text_size(rems(0.7))
                    .text_color(if hint.starts_with("Now") {
                        t.accent
                    } else {
                        t.secondary
                    })
                    .child(hint),
            );
        }
        let mut content = div()
            .id("details-scroll")
            .flex_shrink(1.)
            .min_h_0()
            .overflow_y_scroll()
            .px(rems(1.))
            .pb(rems(1.))
            .flex()
            .flex_col()
            .gap(rems(0.9))
            .child(
                div()
                    .text_size(rems(1.2))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.bright)
                    .whitespace_normal()
                    .when(e.completed, |d| d.line_through())
                    .child(e.title.clone()),
            )
            .child(row(IconName::Clock, when.into_any_element(), cx));
        // Only mention the event's zone when its clock time differs from the display.
        let offset = |z: &str| {
            use chrono::Offset;
            e.start.with_timezone(&zone(z)).offset().fix()
        };
        if e.all_day.is_none() && offset(&e.timezone) != offset(&self.display_zone) {
            content = content.child(detail_line(
                IconName::Globe,
                format!(
                    "{} in {}",
                    e.time_label(zone(&e.timezone)),
                    e.timezone.replace('_', " ")
                ),
                cx,
            ));
        }
        if e.series_id.is_some() || e.repeat != Repeat::Never {
            content = content.child(detail_line(
                IconName::Repeat,
                if e.remote_id.is_some() {
                    "Part of a recurring event".into()
                } else {
                    format!("Repeats {}", e.repeat.label().to_lowercase())
                },
                cx,
            ));
        }
        if !e.location.is_empty() {
            content = content.child(row(
                IconName::MapPin,
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.4))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .child(e.location.clone()),
                    )
                    .child(copy("copy-location", e.location.clone(), "Location", cx))
                    .into_any_element(),
                cx,
            ));
        }
        if !e.link.is_empty() {
            let link = e.link.clone();
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.4))
                    .child(
                        action_button(
                            "join-meeting",
                            IconName::Video,
                            "Join meeting",
                            ButtonVariant::Primary,
                            cx,
                        )
                        .debug_selector(|| "join-meeting".into())
                        .flex_1()
                        .on_click(cx.listener(move |app, _, _, cx| {
                            match ocal_core::safe_url(&link) {
                                Ok(url) => {
                                    if let Err(error) = std::process::Command::new("xdg-open")
                                        .arg(url.as_str())
                                        .spawn()
                                    {
                                        app.toast_error(error.to_string(), cx);
                                    }
                                }
                                Err(error) => {
                                    app.toast_error(error.to_string(), cx);
                                }
                            }
                        })),
                    )
                    .child(copy("copy-link", e.link.clone(), "Meeting link", cx)),
            );
        } else if e.conference_status == "pending" {
            content = content.child(detail_line(
                IconName::Video,
                "Google is generating the Meet link; it appears here automatically.".into(),
                cx,
            ));
        } else if e.conference_status == "failure" {
            content = content.child(
                div()
                    .text_color(t.danger)
                    .child("Google could not generate this Meet link."),
            );
        }
        if !e.attendees.is_empty() {
            let mut guests: Vec<_> = e.attendees.iter().collect();
            guests.sort_by_key(|a| !a.me);
            let going = guests.iter().filter(|a| a.response == "accepted").count();
            let mut list = div().flex().flex_col().gap(rems(0.3)).child(
                section_label("GUESTS", cx)
                    .px(rems(0.))
                    .pt(rems(0.))
                    .child(format!(" · {going} of {} going", guests.len())),
            );
            for a in guests {
                let (glyph, tone) = match a.response.as_str() {
                    "accepted" => (IconName::CircleCheck, t.success),
                    "declined" => (IconName::CircleX, t.danger),
                    "tentative" => (IconName::CircleQuestionMark, t.warning),
                    _ => (IconName::Clock, t.secondary),
                };
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(0.5))
                        .text_size(rems(0.74))
                        .child(icon(glyph).size(rems(0.85)).text_color(tone))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(a.email.clone()),
                        )
                        .children(a.me.then(|| div().text_color(t.secondary).child("you"))),
                );
            }
            content = content.child(list);
        }
        if e.remote_id.is_some()
            && let Some(me) = e.attendees.iter().find(|a| a.me)
        {
            let ev = e.clone();
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.6))
                    .child(div().text_color(t.secondary).child("Going?"))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .h(rems(1.8))
                            .border_1()
                            .border_color(t.divider())
                            .children(
                                [
                                    ("Yes", "accepted"),
                                    ("Maybe", "tentative"),
                                    ("No", "declined"),
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(i, (label, response))| {
                                    let active = me.response == response;
                                    let e = ev.clone();
                                    button(("rsvp", i), label, ButtonVariant::Secondary, cx)
                                        .debug_selector(move || format!("rsvp-{i}"))
                                        .relative()
                                        .flex_1()
                                        .h_full()
                                        .border_0()
                                        .disabled(self.busy)
                                        .bg(if active {
                                            t.selection
                                        } else {
                                            t.foreground.opacity(0.)
                                        })
                                        .text_color(if active { t.bright } else { t.secondary })
                                        .children(active.then(|| {
                                            div()
                                                .absolute()
                                                .left_0()
                                                .right_0()
                                                .bottom_0()
                                                .h(px(2.))
                                                .bg(t.accent)
                                        }))
                                        .on_click(cx.listener(move |app, _, _, cx| {
                                            app.busy = true;
                                            app.dispatch(Command::Respond(
                                                e.clone(),
                                                response.into(),
                                            ));
                                            cx.notify();
                                        }))
                                }),
                            ),
                    ),
            );
        }
        if !e.notes.is_empty() {
            content = content.child(
                div()
                    .pt(rems(0.6))
                    .border_t_1()
                    .border_color(t.divider())
                    .text_size(rems(0.78))
                    .whitespace_normal()
                    .child(e.notes.clone()),
            );
        }
        let edit = e.clone();
        let duplicate = e.clone();
        let delete = e.clone();
        let series = e.series_id.is_some() && e.remote_id.is_none();
        let footer = if self.delete_confirm {
            div()
                .p(rems(0.75))
                .flex()
                .items_center()
                .gap(rems(0.5))
                .child(div().flex_1().whitespace_normal().child(if series {
                    "Delete every event in this series?"
                } else {
                    "Delete this event?"
                }))
                .child(
                    button("cancel-delete", "Cancel", ButtonVariant::Secondary, cx).on_click(
                        cx.listener(|app, _, _, cx| {
                            app.delete_confirm = false;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button("confirm-delete", "Delete", ButtonVariant::Primary, cx)
                        .debug_selector(|| "confirm-delete".into())
                        .disabled(self.busy)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.busy = true;
                            app.dispatch(Command::Delete(delete.clone()));
                            cx.notify();
                        })),
                )
        } else {
            div()
                .p(rems(0.75))
                .flex()
                .items_center()
                .gap(rems(0.4))
                .child(
                    action_button(
                        "edit-event",
                        IconName::PencilLine,
                        if series { "Edit series" } else { "Edit" },
                        ButtonVariant::Primary,
                        cx,
                    )
                    .debug_selector(|| "edit-event".into())
                    .disabled(!writable || self.busy)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_editor(edit.clone(), true, window, cx)
                    })),
                )
                .child(
                    tool_button("duplicate-event", IconName::Copy, "Duplicate", "", cx).on_click(
                        cx.listener(move |app, _, window, cx| {
                            app.duplicate(&duplicate, window, cx)
                        }),
                    ),
                )
                .child(div().flex_1())
                .child(
                    tool_button("delete-event", IconName::Trash, "Delete", "", cx)
                        .disabled(!writable || self.busy)
                        .hover(|s| s.text_color(t.danger))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.delete_confirm = true;
                            cx.notify();
                        })),
                )
        };
        div()
            .id("event-details")
            .debug_selector(|| "event-details".into())
            .role(gpui_kit::Role::Dialog)
            .aria_label("Event details")
            .absolute()
            .right_0()
            .top(rems(HEADER))
            .w(if self.compact {
                window.viewport_size().width
            } else {
                rems(24.).to_pixels(window.rem_size())
            })
            .max_w(window.viewport_size().width)
            .max_h(
                window.viewport_size().height
                    - rems(HEADER + SHEET_GAP).to_pixels(window.rem_size()),
            )
            .flex()
            .flex_col()
            .border_l_1()
            .border_b_1()
            .border_color(t.control_border())
            .bg(t.background)
            .shadow_lg()
            .occlude()
            .child(
                div()
                    .h(rems(2.6))
                    .flex_shrink_0()
                    .pl(rems(1.))
                    .pr(rems(0.5))
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .child(div().w(px(3.)).h(rems(1.1)).bg(color))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(rems(0.72))
                            .text_color(t.secondary)
                            .child(calendar_name),
                    )
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
            .child(
                footer
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(t.divider()),
            )
            .into_any_element()
    }
}

fn row(glyph: IconName, content: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .gap(rems(0.7))
        .items_start()
        .child(
            icon(glyph)
                .size(rems(0.95))
                .mt(rems(0.1))
                .text_color(cx.omarchy().secondary),
        )
        .child(div().flex_1().min_w_0().text_size(rems(0.8)).child(content))
        .into_any_element()
}

pub(super) fn detail_line(glyph: IconName, text: String, cx: &App) -> AnyElement {
    row(
        glyph,
        div()
            .whitespace_normal()
            .text_size(rems(0.78))
            .child(text)
            .into_any_element(),
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times_describe_upcoming_current_and_past_events() {
        let mut e = Event::new(
            "local",
            NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
            "UTC",
        )
        .unwrap();
        let at = |m: i64| e.start + Duration::minutes(m);
        assert_eq!(relative_time(&e, at(-25)).unwrap(), "in 25 min");
        assert_eq!(relative_time(&e, at(-180)).unwrap(), "in 3 h");
        assert_eq!(relative_time(&e, at(20)).unwrap(), "Now · 40 min left");
        assert_eq!(relative_time(&e, at(61)).unwrap(), "Ended");
        e.all_day = Some((e.date(), e.date() + Duration::days(1)));
        assert!(relative_time(&e, at(0)).is_none());
    }
}
