//! Event chips shared by the views, plus the agenda and task lists.
use super::*;
use crate::appearance::{RADIUS_SMALL, Tones, empty_state, inset_ring, label_chip};

/// How an event is drawn outside the time grid.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Chip {
    /// A filled bar for all-day items.
    Bar,
    /// A dot, start time and title for timed events in a month cell.
    Dot,
    /// A full-width agenda row with time, title and location.
    Row,
}

impl Ocal {
    pub(super) fn is_selected(&self, e: &Event) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|s| s.id == e.id && s.start == e.start)
    }
    /// An event in any non-grid view. Click opens details; right-click opens actions.
    pub(super) fn event_chip(
        &self,
        e: &Event,
        index: usize,
        style: Chip,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let tz = self.tz();
        let color = self.calendar_color(&e.calendar, cx);
        let selected = self.is_selected(e);
        let past = e.end <= Utc::now() && !e.task;
        let pick = e.clone();
        let action = e.clone();
        let target = cx.entity().downgrade();
        let id = match style {
            Chip::Row => "agenda-event",
            _ => "month-event",
        };
        let base = button((id, index), "", ButtonVariant::Secondary, cx)
            .debug_selector(move || format!("event-{index}"))
            .accessibility_label(format!("{}, {}", e.title, e.time_label(tz)))
            .relative()
            .w_full()
            .min_w_0()
            .border_0()
            .justify_start();
        let title = |c: gpui_kit::Hsla| {
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(c)
                .when(e.task && e.completed, |d| d.line_through())
                .child(e.title.clone())
        };
        let chip = match style {
            // All-day events are filled pills; tasks are outlined, with a checkbox.
            Chip::Bar => {
                let (fill, hover, text) = if selected {
                    (color, color, t.on_accent)
                } else if e.task {
                    (
                        t.foreground.opacity(0.),
                        t.hover(),
                        if e.completed { t.muted() } else { t.foreground },
                    )
                } else {
                    (
                        tint(&t, color, 0.16),
                        tint(&t, color, 0.24),
                        event_text(&t, color),
                    )
                };
                base.h(rems(1.375))
                    .px(rems(0.5))
                    .gap(rems(0.4375))
                    .rounded(RADIUS_SMALL)
                    .bg(fill)
                    .hover(move |s| s.bg(hover))
                    .when(e.task && !selected, |b| b.shadow(inset_ring(t.hair(), 1.)))
                    .text_size(rems(0.75))
                    .font_weight(FontWeight::MEDIUM)
                    .children(
                        e.task
                            .then(|| self.task_toggle(("chip-check", index), e, rems(0.8125), cx)),
                    )
                    .child(title(text))
            }
            Chip::Dot => {
                let text = if selected {
                    t.on_accent
                } else if past {
                    t.muted()
                } else {
                    t.foreground
                };
                base.h(rems(1.25))
                    .px(rems(0.3125))
                    .gap(rems(0.375))
                    .rounded(rems(0.3125))
                    .bg(if selected {
                        color
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .hover({
                        let hover = if selected { color } else { t.hover() };
                        move |s| s.bg(hover)
                    })
                    .text_size(rems(0.75))
                    .child(
                        div()
                            .size(rems(0.375))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(if selected { t.on_accent } else { color }),
                    )
                    .child(title(text))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(rems(0.69))
                            .text_color(if selected {
                                t.on_accent.opacity(0.7)
                            } else {
                                t.faint()
                            })
                            .child(e.start.with_timezone(&tz).format("%H:%M").to_string()),
                    )
            }
            Chip::Row => {
                let minutes = (e.end - e.start).num_minutes();
                let (time, detail) = if e.task {
                    ("Task".to_string(), None)
                } else if e.all_day.is_some() {
                    ("All day".to_string(), None)
                } else {
                    (
                        e.start.with_timezone(&tz).format("%H:%M").to_string(),
                        Some(duration_label(minutes)),
                    )
                };
                let sub = !e.location.is_empty() || !e.link.is_empty();
                base.min_h(rems(3.))
                    .py(rems(0.4375))
                    .px(rems(0.75))
                    .gap(rems(0.875))
                    .rounded(rems(0.625))
                    .bg(if selected {
                        t.selection
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .hover({
                        let hover = if selected { t.selection } else { t.hover() };
                        move |s| s.bg(hover)
                    })
                    .child(
                        div()
                            .w(rems(5.5))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .text_size(rems(0.78))
                            .text_color(if past { t.muted() } else { t.foreground })
                            .child(time)
                            .children(detail.map(|d| {
                                div().text_size(rems(0.72)).text_color(t.muted()).child(d)
                            })),
                    )
                    .child(
                        div()
                            .w(px(3.))
                            .h(rems(2.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(color),
                    )
                    .children(
                        e.task
                            .then(|| self.task_toggle(("row-check", index), e, rems(1.), cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(rems(0.0625))
                            .child(
                                title(if past || e.completed {
                                    t.muted()
                                } else {
                                    t.bright
                                })
                                .font_weight(FontWeight::MEDIUM),
                            )
                            .children(sub.then(|| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(rems(0.375))
                                    .min_w_0()
                                    .text_size(rems(0.78))
                                    .text_color(t.muted())
                                    .when(!e.location.is_empty(), |d| {
                                        d.child(icon(IconName::MapPin).size(rems(0.8125))).child(
                                            div()
                                                .min_w_0()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .child(e.location.clone()),
                                        )
                                    })
                                    .when(!e.location.is_empty() && !e.link.is_empty(), |d| {
                                        d.child("·")
                                    })
                                    .when(!e.link.is_empty(), |d| {
                                        d.child(icon(IconName::Video).size(rems(0.8125)))
                                            .child(meeting_provider(&e.link))
                                    })
                            })),
                    )
                    .child(label_chip(self.calendar_name(&e.calendar), color))
            }
        };
        let chip = self.anchor_event(chip, e, cx);
        menu(
            ("event-menu", index),
            chip.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(
                    cx.listener(move |app, _, window, cx| app.pick_event(pick.clone(), window, cx)),
                ),
            self.event_items(e),
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.event_action(action.clone(), index, window, cx)
                });
            },
        )
        .mouse_button(MouseButton::Right)
        .into_any_element()
    }
    pub(super) fn agenda(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let tz = self.tz();
        let today = self.today();
        let searching = !self.search.read(cx).value().trim().is_empty();
        // Search covers the whole cached range; the agenda reads forwards.
        let (from, to) = if searching {
            (
                self.date - Duration::days(60),
                self.date + Duration::days(120),
            )
        } else {
            (self.date, self.date + Duration::days(60))
        };
        let mut events = self.events(from, to, cx);
        // Within a day, all-day items and tasks lead.
        events.sort_by_key(|e| {
            (
                e.dates_in(tz).0.max(from),
                e.all_day.is_none() && !e.task,
                e.start,
            )
        });
        let mut list = div()
            .flex()
            .flex_col()
            .px(rems(0.75))
            .pb(rems(2.))
            .max_w(rems(54.));
        if searching {
            list = list.child(
                div()
                    .px(rems(0.75))
                    .pt(rems(1.))
                    .text_size(rems(0.78))
                    .text_color(t.muted())
                    .child(format!(
                        "{} {} matching “{}”",
                        events.len(),
                        if events.len() == 1 { "event" } else { "events" },
                        self.search.read(cx).value().trim()
                    )),
            );
        }
        let mut index = 0;
        let mut i = 0;
        while i < events.len() {
            let day = events[i].dates_in(tz).0.max(from);
            let group: Vec<_> = events[i..]
                .iter()
                .take_while(|e| e.dates_in(tz).0.max(from) == day)
                .collect();
            i += group.len();
            let is_today = day == today;
            // "Today 17 October", "Tomorrow 18 October", "Monday 19 October".
            let name = match (day - today).num_days() {
                0 => "Today".to_string(),
                1 => "Tomorrow".to_string(),
                -1 => "Yesterday".to_string(),
                _ => day.format("%A").to_string(),
            };
            list = list
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(rems(0.5))
                        .px(rems(0.75))
                        .pt(rems(1.))
                        .pb(rems(0.375))
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_today { t.accent } else { t.bright })
                                .child(name),
                        )
                        .child(div().text_size(rems(0.75)).text_color(t.muted()).child(
                            if day.year() == today.year() {
                                day.format("%-d %B").to_string()
                            } else {
                                day.format("%-d %B %Y").to_string()
                            },
                        )),
                )
                .children(group.into_iter().map(|e| {
                    index += 1;
                    self.event_chip(e, 100000 + index, Chip::Row, cx)
                }));
        }
        if events.is_empty() {
            list = list.child(if searching {
                empty_state(
                    IconName::Search,
                    "No matching events",
                    "Search covers two months back and four months ahead.",
                    cx,
                )
            } else {
                empty_state(
                    IconName::CalendarCheck,
                    "Nothing scheduled",
                    "A little breathing room. Press C to add an event.",
                    cx,
                )
            });
        }
        div()
            .id("agenda-scroll")
            .debug_selector(|| "agenda-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.agenda_scroll)
            .child(list)
            .into_any_element()
    }
    pub(super) fn tasks(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let today = self.today();
        let mut tasks: Vec<_> = self
            .snapshot
            .events
            .iter()
            .filter(|e| e.task && self.visible(&e.calendar))
            .cloned()
            .collect();
        tasks.sort_by_key(|e| (e.completed, e.start));
        let open = tasks.iter().filter(|e| !e.completed).count();
        let heading = |name: &'static str, count: usize| {
            div()
                .flex()
                .items_baseline()
                .gap(rems(0.5))
                .px(rems(0.75))
                .pt(rems(1.))
                .pb(rems(0.375))
                .child(
                    div()
                        .text_size(rems(0.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.bright)
                        .child(name),
                )
                .child(
                    div()
                        .text_size(rems(0.75))
                        .text_color(t.muted())
                        .child(count.to_string()),
                )
        };
        let mut list = div().flex().flex_col().px(rems(0.75)).pb(rems(1.));
        for (i, e) in tasks.iter().enumerate() {
            if i == 0 && open > 0 {
                list = list.child(heading("To do", open));
            }
            if i == open {
                list = list.child(heading("Completed", tasks.len() - open));
            }
            let due = e.date();
            let overdue = !e.completed && due < today;
            let pick = e.clone();
            list = list.child(
                div()
                    .relative()
                    .h(rems(2.625))
                    .pl(rems(0.375))
                    .flex()
                    .items_center()
                    .gap(rems(0.25))
                    .rounded(rems(0.625))
                    .bg(if self.is_selected(e) {
                        t.selection
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .child(
                        div()
                            .size(rems(1.9))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(self.task_toggle(("task-check", i), e, rems(1.), cx)),
                    )
                    .child(
                        button(("task", i), "", ButtonVariant::Secondary, cx)
                            .debug_selector(move || format!("task-{i}"))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .border_0()
                            .rounded(rems(0.625))
                            .justify_start()
                            .gap(rems(0.75))
                            .px(rems(0.5))
                            .bg(t.foreground.opacity(0.))
                            .hover(|s| s.bg(t.hover()))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_color(if e.completed { t.muted() } else { t.bright })
                                    .when(e.completed, |d| d.line_through())
                                    .child(e.title.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(rems(0.75))
                                    .text_color(if overdue {
                                        t.danger
                                    } else if due == today && !e.completed {
                                        t.accent
                                    } else {
                                        t.muted()
                                    })
                                    .when(due == today || overdue, |d| {
                                        d.font_weight(FontWeight::MEDIUM)
                                    })
                                    .child(match (due - today).num_days() {
                                        0 => "Today".to_string(),
                                        1 => "Tomorrow".to_string(),
                                        -1 => "Yesterday".to_string(),
                                        _ => due.format("%a %-d %b").to_string(),
                                    }),
                            )
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.pick_event(pick.clone(), window, cx)
                            })),
                    ),
            );
        }
        if tasks.is_empty() {
            list = list.child(empty_state(
                IconName::SquareCheck,
                "No tasks",
                "Right-click a day and choose New task.",
                cx,
            ));
        }
        div()
            .id("tasks-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(div().max_w(rems(48.)).child(list))
            .into_any_element()
    }
}

/// "30 min", "2 h" or "1 h 30 min".
pub(super) fn duration_label(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// The meeting service a link belongs to, for labels and buttons.
pub(super) fn meeting_provider(link: &str) -> &'static str {
    let host = link
        .split("://")
        .nth(1)
        .unwrap_or(link)
        .split(['/', '?'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    if host.ends_with("meet.google.com") {
        "Google Meet"
    } else if host.ends_with("zoom.us") {
        "Zoom"
    } else if host.contains("teams.microsoft") || host.contains("teams.live") {
        "Teams"
    } else {
        "Meeting"
    }
}
