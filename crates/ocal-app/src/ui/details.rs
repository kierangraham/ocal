//! Event details panel.
use super::*;
use crate::appearance::{
    RADIUS_SMALL, Tones, card_shadow, label_chip, mode_button, pill_tab, send_button, with_tip,
};

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
    /// The selected event as a card beside where it is drawn, or a full-width
    /// panel in compact windows.
    pub(super) fn details(
        &self,
        anchor: Option<Bounds<Pixels>>,
        docked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(e) = self.selected.as_ref() else {
            return div().into_any_element();
        };
        let t = cx.omarchy().clone();
        let tz = self.tz();
        let writable = self.writable(e);
        let color = self.calendar_color(&e.calendar, cx);
        let target = cx.entity().downgrade();
        let action = e.clone();
        let open_target = target.clone();
        let small = |b: gpui_omarchy::Button| b.size(rems(1.75)).rounded(RADIUS_SMALL);
        let more = menu(
            "details-menu",
            small(tool_button(
                "details-more",
                IconName::Ellipsis,
                "Event actions",
                "Shift F10",
                cx,
            )),
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
            small(tool_button(id, IconName::Copy, what, "", cx)).on_click(cx.listener(
                move |app, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    app.toast(format!("{what} copied"), cx);
                },
            ))
        };
        let (day, _) = e.dates_in(tz);
        let hint = relative_time(e, Utc::now());
        let header = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rems(0.5))
            .pl(rems(1.125))
            .pr(rems(0.625))
            .pt(rems(0.75))
            .text_size(rems(0.75))
            .text_color(t.muted())
            .child(label_chip(self.calendar_name(&e.calendar), color))
            .children(hint.map(|hint| {
                div()
                    .min_w_0()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .when(hint.starts_with("Now"), |d| d.text_color(t.accent))
                    .child(hint)
            }))
            .child(div().flex_1())
            .child(more)
            .child(
                small(tool_button(
                    "close-details",
                    IconName::X,
                    "Close details",
                    "Esc",
                    cx,
                ))
                .on_click(cx.listener(|app, _, window, cx| {
                    app.selected = None;
                    app.event_menu = false;
                    window.focus(&app.focus, cx);
                    cx.notify();
                })),
            );
        let when = if e.task {
            div().child(format!("Due {}", sidebar::relative_day(day, self.today())))
        } else if e.all_day.is_some() {
            let (first, end) = e.dates_in(tz);
            let last = end - Duration::days(1);
            div()
                .child(
                    div()
                        .text_color(t.foreground)
                        .font_weight(FontWeight::MEDIUM)
                        .child(first.format("%A %-d %B").to_string()),
                )
                .child(if last > first {
                    format!("All day · until {}", last.format("%a %-d %b"))
                } else {
                    "All day".into()
                })
        } else {
            div()
                .child(
                    div()
                        .text_color(t.foreground)
                        .font_weight(FontWeight::MEDIUM)
                        .child(day.format("%A %-d %B").to_string()),
                )
                .child(format!(
                    "{} · {}",
                    e.time_label(tz).replace('–', " – "),
                    agenda::duration_label((e.end - e.start).num_minutes())
                ))
        };
        let mut rows = div().flex().flex_col().gap(rems(0.625)).pt(rems(0.75));
        if !e.link.is_empty() {
            let link = e.link.clone();
            rows = rows.child(row(
                IconName::Video,
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(agenda::meeting_provider(&e.link))
                            .child(
                                div()
                                    .text_size(rems(0.75))
                                    .text_color(t.muted())
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(
                                        e.link
                                            .trim_start_matches("https://")
                                            .trim_start_matches("http://")
                                            .to_string(),
                                    ),
                            ),
                    )
                    .child(copy("copy-link", e.link.clone(), "Meeting link", cx))
                    .child(
                        with_tip(
                            send_button("join-meeting", "Join", None, None, cx)
                                .debug_selector(|| "join-meeting".into())
                                .h(rems(1.75)),
                            "Join meeting",
                        )
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
                    .into_any_element(),
                cx,
            ));
        } else if e.conference_status == "pending" {
            rows = rows.child(detail_line(
                IconName::Video,
                "Google is generating the Meet link; it appears here automatically.".into(),
                cx,
            ));
        } else if e.conference_status == "failure" {
            rows = rows.child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(t.danger)
                    .child("Google could not generate this Meet link."),
            );
        }
        if !e.location.is_empty() {
            rows = rows.child(row(
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
        // Only mention the event's zone when its clock time differs from the display.
        let offset = |z: &str| {
            use chrono::Offset;
            e.start.with_timezone(&zone(z)).offset().fix()
        };
        if e.all_day.is_none() && offset(&e.timezone) != offset(&self.display_zone) {
            rows = rows.child(detail_line(
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
            rows = rows.child(detail_line(
                IconName::Repeat,
                if e.remote_id.is_some() {
                    "Part of a recurring event".into()
                } else {
                    format!("Repeats {}", e.repeat.label().to_lowercase())
                },
                cx,
            ));
        }
        if !e.attendees.is_empty() {
            rows = rows.child(self.guests(e, cx));
        }
        if !e.notes.is_empty() {
            rows = rows.child(row(
                IconName::TextAlignStart,
                div()
                    .whitespace_normal()
                    .child(e.notes.clone())
                    .into_any_element(),
                cx,
            ));
        }
        let content = div()
            .id("details-scroll")
            .flex_shrink(1.)
            .min_h_0()
            .overflow_y_scroll()
            .px(rems(1.125))
            .pb(rems(0.875))
            .child(
                div()
                    .pt(rems(0.5))
                    .pb(rems(0.25))
                    .text_size(rems(1.25))
                    .line_height(rems(1.5625))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.bright)
                    .whitespace_normal()
                    .when(e.completed, |d| d.line_through())
                    .child(e.title.clone()),
            )
            .child(when.text_size(rems(0.78)).text_color(t.muted()))
            .child(rows);
        let edit = e.clone();
        let duplicate = e.clone();
        let delete = e.clone();
        let series = e.series_id.is_some() && e.remote_id.is_none();
        let footer = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rems(0.125))
            .px(rems(0.5))
            .py(rems(0.5))
            .border_t_1()
            .border_color(t.hair())
            .child(
                with_tip(
                    mode_button(
                        "edit-event",
                        if series { "Edit series" } else { "Edit" },
                        Some(IconName::PencilLine),
                        cx,
                    )
                    .debug_selector(|| "edit-event".into()),
                    "Edit · enter",
                )
                .disabled(!writable || self.busy)
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_editor(edit.clone(), true, window, cx)
                })),
            )
            .child(
                mode_button("duplicate-event", "Duplicate", Some(IconName::Copy), cx).on_click(
                    cx.listener(move |app, _, window, cx| app.duplicate(&duplicate, window, cx)),
                ),
            )
            .child(div().flex_1())
            .child(
                small(tool_button(
                    "delete-event",
                    IconName::Trash,
                    "Delete",
                    "Del",
                    cx,
                ))
                .debug_selector(|| "delete-event".into())
                .disabled(!writable || self.busy)
                .hover(|s| s.bg(t.danger.opacity(0.1)).text_color(t.danger))
                .on_click(cx.listener(move |app, _, _, cx| app.delete_event(delete.clone(), cx))),
            );
        let viewport = window.viewport_size();
        let rem = window.rem_size();
        let card = div()
            .id("event-details")
            .debug_selector(|| "event-details".into())
            .role(gpui_kit::Role::Dialog)
            .aria_label("Event details")
            .absolute()
            .flex()
            .flex_col()
            .bg(t.background)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(content)
            .child(footer);
        // Compact windows have no room beside the event: a full-width panel.
        if self.compact {
            return card
                .left_0()
                .right_0()
                .top(rems(HEADER))
                .max_h(viewport.height - rems(HEADER + SHEET_GAP).to_pixels(rem))
                .border_b_1()
                .border_color(t.hair())
                .shadow(card_shadow(&t))
                .into_any_element();
        }
        // Beside the event when it is on screen, flipping left near the right
        // edge; otherwise in the sheet's top-right corner.
        let gap = rems(SHEET_GAP).to_pixels(rem);
        let sheet_left = if docked {
            rems(SIDEBAR).to_pixels(rem)
        } else {
            gap
        };
        let sheet_right = viewport.width - gap;
        let top_limit = rems(TOOLBAR + SHEET_GAP).to_pixels(rem) + gap;
        let bottom_limit = viewport.height - gap * 2.;
        let width = rems(CARD_WIDTH)
            .to_pixels(rem)
            .min(sheet_right - sheet_left - gap * 2.);
        let space = px(10.);
        let (left, top) = match anchor
            .filter(|b| b.bottom() > top_limit && b.top() < bottom_limit && b.left() < sheet_right)
        {
            Some(b) => {
                let left = if b.right() + space + width <= sheet_right - gap {
                    b.right() + space
                } else {
                    b.left() - space - width
                };
                // Leave room for a typical card below a low anchor.
                let lowest = (bottom_limit - rems(20.).to_pixels(rem)).max(top_limit);
                (
                    left.max(sheet_left + gap),
                    (b.top() - px(8.)).clamp(top_limit, lowest),
                )
            }
            None => (sheet_right - width - gap, top_limit),
        };
        card.left(left)
            .top(top)
            .w(width)
            .max_h(bottom_limit - top)
            .rounded(rems(0.875))
            .shadow(card_shadow(&t))
            .into_any_element()
    }
    /// "4 guests · 3 accepted · 1 awaiting", which expands to the guest list.
    fn guests(&self, e: &Event, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut guests: Vec<_> = e.attendees.iter().collect();
        guests.sort_by_key(|a| !a.me);
        let count = |r: &str| guests.iter().filter(|a| a.response == r).count();
        let awaiting = guests.len() - count("accepted") - count("declined") - count("tentative");
        let summary = [
            (count("accepted"), "accepted"),
            (count("tentative"), "maybe"),
            (count("declined"), "declined"),
            (awaiting, "awaiting"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect::<Vec<_>>()
        .join(" · ");
        let open = self.guests_open;
        let mut list = div().flex().flex_col().gap(rems(0.3)).child(
            div()
                .id("guests-summary")
                .debug_selector(|| "guests-summary".into())
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(rems(0.375))
                .child(
                    div()
                        .flex_1()
                        .child(format!(
                            "{} guest{}",
                            guests.len(),
                            if guests.len() == 1 { "" } else { "s" }
                        ))
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(t.muted())
                                .child(summary),
                        ),
                )
                .child(
                    icon(if open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .size(rems(0.875))
                    .text_color(t.muted()),
                )
                .on_click(cx.listener(|app, _, _, cx| {
                    app.guests_open = !app.guests_open;
                    cx.notify();
                })),
        );
        if open {
            for a in &guests {
                let (glyph, tone) = match a.response.as_str() {
                    "accepted" => (IconName::CircleCheck, t.success),
                    "declined" => (IconName::CircleX, t.danger),
                    "tentative" => (IconName::CircleQuestionMark, t.warning),
                    _ => (IconName::Clock, t.muted()),
                };
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(0.5))
                        .text_size(rems(0.75))
                        .child(icon(glyph).size(rems(0.85)).text_color(tone))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(a.email.clone()),
                        )
                        .children(a.me.then(|| div().text_color(t.muted()).child("you"))),
                );
            }
        }
        if e.remote_id.is_some()
            && let Some(me) = e.attendees.iter().find(|a| a.me)
        {
            let ev = e.clone();
            list = list.child(
                div()
                    .pt(rems(0.25))
                    .flex()
                    .items_center()
                    .gap(rems(0.375))
                    .child(div().text_color(t.muted()).pr(rems(0.25)).child("Going?"))
                    .children(
                        [
                            ("Yes", "accepted"),
                            ("Maybe", "tentative"),
                            ("No", "declined"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (label, response))| {
                            let e = ev.clone();
                            pill_tab(("rsvp", i), label, me.response == response, cx)
                                .debug_selector(move || format!("rsvp-{i}"))
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
        row(IconName::Users, list.into_any_element(), cx)
    }
}

/// Width of the details card.
const CARD_WIDTH: f32 = 21.25;

fn row(glyph: IconName, content: AnyElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .gap(rems(0.6875))
        .items_start()
        .child(
            icon(glyph)
                .size(rems(1.))
                .mt(rems(0.125))
                .flex_shrink_0()
                .text_color(cx.omarchy().muted()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(rems(0.8125))
                .text_color(cx.omarchy().foreground)
                .child(content),
        )
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
