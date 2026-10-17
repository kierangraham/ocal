//! Event chips shared by the views, plus the agenda and task lists.
use super::*;
use crate::appearance::{empty_state, section_label, swatch};
use sidebar::relative_day;

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
    fn is_selected(&self, e: &Event) -> bool {
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
        let past = e.end <= Utc::now();
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
            .justify_start()
            .when(past && !selected, |b| b.opacity(0.6));
        let chip = match style {
            Chip::Bar => base
                .h(rems(1.15))
                .px(rems(0.35))
                .border_l_2()
                .border_color(color)
                .bg(if selected {
                    t.selection
                } else {
                    tint(&t, color, 0.16)
                })
                .hover({
                    let hover = tint(&t, color, 0.26);
                    move |s| s.bg(hover)
                })
                .text_size(rems(0.65))
                .text_color(event_text(&t, color))
                .child(div().min_w_0().text_ellipsis().child(e.title.clone())),
            Chip::Dot => base
                .h(rems(1.15))
                .px(rems(0.3))
                .gap(rems(0.35))
                .bg(if selected {
                    t.selection
                } else {
                    t.foreground.opacity(0.)
                })
                .hover(|s| s.bg(t.hover_fill()))
                .text_size(rems(0.65))
                .child(div().size(rems(0.4)).flex_shrink_0().bg(color))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(t.secondary)
                        .child(e.start.with_timezone(&tz).format("%H:%M").to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(t.foreground)
                        .child(e.title.clone()),
                ),
            Chip::Row => base
                .h(rems(2.6))
                .px(rems(0.75))
                .gap(rems(0.75))
                .bg(if selected {
                    t.selection
                } else {
                    t.foreground.opacity(0.)
                })
                .hover(|s| s.bg(t.hover_fill()))
                .children(selected.then(|| {
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.accent)
                }))
                .child(
                    div()
                        .w(rems(6.5))
                        .flex_shrink_0()
                        .text_size(rems(0.72))
                        .text_color(t.secondary)
                        .child(e.time_label(tz)),
                )
                .child(div().w(px(3.)).h(rems(1.6)).flex_shrink_0().bg(color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(rems(0.1))
                        .child(
                            div()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_color(t.bright)
                                .child(e.title.clone()),
                        )
                        .children((!e.location.is_empty()).then(|| {
                            div()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(rems(0.68))
                                .text_color(t.secondary)
                                .child(e.location.clone())
                        })),
                )
                .children((!e.link.is_empty()).then(|| {
                    icon(IconName::Video)
                        .size(rems(0.85))
                        .text_color(t.secondary)
                })),
        };
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
        let events: Vec<_> = self
            .events(from, to, cx)
            .into_iter()
            .filter(|e| !e.task && !e.completed)
            .collect();
        let mut list = div().flex().flex_col();
        if searching {
            list = list.child(
                div()
                    .px(rems(1.))
                    .py(rems(0.6))
                    .border_b_1()
                    .border_color(t.divider())
                    .text_size(rems(0.72))
                    .text_color(t.secondary)
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
            list = list.child(
                div()
                    .flex()
                    .border_b_1()
                    .border_color(t.divider())
                    .child(
                        div()
                            .w(rems(5.))
                            .flex_shrink_0()
                            .pt(rems(0.55))
                            .pl(rems(1.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(rems(0.62))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if is_today { t.accent } else { t.secondary })
                                    .child(day.format("%a").to_string().to_uppercase()),
                            )
                            .child(
                                div()
                                    .text_size(rems(1.25))
                                    .text_color(if is_today { t.accent } else { t.bright })
                                    .child(day.day().to_string()),
                            )
                            .child(
                                div()
                                    .text_size(rems(0.6))
                                    .text_color(t.secondary)
                                    .child(day.format("%b").to_string()),
                            ),
                    )
                    .child(div().flex_1().min_w_0().py(rems(0.25)).children(
                        group.into_iter().map(|e| {
                            index += 1;
                            self.event_chip(e, 100000 + index, Chip::Row, cx)
                        }),
                    )),
            );
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
        let mut list = div().flex().flex_col().pb(rems(1.));
        for (i, e) in tasks.iter().enumerate() {
            if i == 0 && open > 0 {
                list = list.child(section_label("TO DO", cx).px(rems(1.)));
            }
            if i == open {
                list = list.child(section_label("COMPLETED", cx).px(rems(1.)));
            }
            let color = self.calendar_color(&e.calendar, cx);
            let due = e.date();
            let overdue = !e.completed && due < today;
            let changed = e.clone();
            let pick = e.clone();
            list = list.child(
                div()
                    .relative()
                    .h(rems(2.4))
                    .px(rems(0.6))
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .bg(if self.is_selected(e) {
                        t.selection
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .child(
                        tool_button(
                            ("task-check", i),
                            if e.completed {
                                IconName::SquareCheck
                            } else {
                                IconName::Square
                            },
                            if e.completed {
                                "Mark not done"
                            } else {
                                "Mark done"
                            },
                            "",
                            cx,
                        )
                        .text_color(if e.completed { t.accent } else { t.secondary })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            let mut e = changed.clone();
                            e.completed = !e.completed;
                            app.dispatch(Command::Save {
                                event: e,
                                notify: false,
                            });
                            cx.notify();
                        })),
                    )
                    .child(
                        button(("task", i), "", ButtonVariant::Secondary, cx)
                            .debug_selector(move || format!("task-{i}"))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .border_0()
                            .justify_start()
                            .gap(rems(0.6))
                            .bg(t.foreground.opacity(0.))
                            .hover(|s| s.bg(t.hover_fill()))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_color(if e.completed { t.secondary } else { t.bright })
                                    .when(e.completed, |d| d.line_through())
                                    .child(e.title.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(rems(0.7))
                                    .text_color(if overdue { t.danger } else { t.secondary })
                                    .child(relative_day(due, today)),
                            )
                            .child(swatch(color, true))
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
