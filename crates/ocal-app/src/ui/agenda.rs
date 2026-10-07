//! Agenda and task lists.
use super::*;
use crate::appearance::empty_state;

impl Ocal {
    pub(super) fn event_chip(
        &self,
        e: &Event,
        index: usize,
        small: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let color = self.calendar_color(&e.calendar, cx);
        let e = e.clone();
        let pick = e.clone();
        let action = e.clone();
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|s| s.id == e.id && s.start == e.start);
        let target = cx.entity().downgrade();
        let button = button(
            (if small { "month-event" } else { "agenda-event" }, index),
            "",
            ButtonVariant::Secondary,
            cx,
        )
        .debug_selector(move || format!("event-{index}"))
        .accessibility_label(format!("{}, {}", e.title, e.time_label(self.tz())))
        .w_full()
        .min_w_0()
        .border_0()
        .border_l_2()
        .border_color(color)
        .bg(if selected {
            t.selection
        } else {
            color.opacity(0.09)
        })
        .hover(move |s| s.bg(color.opacity(0.17)))
        .px(rems(0.4))
        .py(rems(if small { 0.18 } else { 0.6 }))
        .justify_start()
        .gap(rems(0.5))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_size(rems(if small { 0.65 } else { 0.84 }))
                .child(e.title.clone()),
        )
        .children((!small).then(|| {
            div()
                .flex_shrink_0()
                .text_size(rems(0.7))
                .text_color(t.secondary)
                .child(e.time_label(self.tz()))
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |app, _, window, cx| app.pick_event(pick.clone(), window, cx)));
        menu(
            ("event-menu", index),
            button,
            self.event_items(&e),
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.event_action(action.clone(), index, window, cx)
                });
            },
        )
        .mouse_button(MouseButton::Right)
        .into_any_element()
    }
    pub(super) fn agenda(&self, tasks: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut events = self.events(self.date, self.date + Duration::days(60), cx);
        if tasks {
            events = self
                .snapshot
                .events
                .iter()
                .filter(|e| e.task && self.visible(&e.calendar))
                .cloned()
                .collect();
            events.sort_by_key(|e| (e.completed, e.start));
        }
        let mut view = div()
            .id("agenda-scroll")
            .debug_selector(|| "agenda-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.agenda_scroll)
            .p(rems(1.4))
            .child(
                div()
                    .max_w(rems(52.))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(rems(0.45))
                    .children(events.iter().enumerate().flat_map(|(i, e)| {
                        let date = e.dates_in(self.tz()).0;
                        let heading =
                            (i == 0 || events[i - 1].dates_in(self.tz()).0 != date).then(|| {
                                div()
                                    .pt(rems(0.85))
                                    .pb(rems(0.35))
                                    .text_size(rems(0.75))
                                    .text_color(t.secondary)
                                    .child(date.format("%A, %-d %B").to_string())
                                    .into_any_element()
                            });
                        let e = e.clone();
                        let event = if e.task {
                            let changed = e.clone();
                            div()
                                .flex()
                                .gap(rems(0.5))
                                .items_center()
                                .child(
                                    tool_button(
                                        ("task-check", i),
                                        if e.completed {
                                            IconName::SquareCheck
                                        } else {
                                            IconName::Square
                                        },
                                        "Complete task",
                                        "",
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |app, _, _, cx| {
                                            let mut e = changed.clone();
                                            e.completed = !e.completed;
                                            app.dispatch(Command::Save {
                                                event: e,
                                                notify: false,
                                            });
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(div().flex_1().min_w_0().child(self.event_chip(
                                    &e,
                                    100000 + i,
                                    false,
                                    cx,
                                )))
                                .into_any_element()
                        } else {
                            self.event_chip(&e, 100000 + i, false, cx)
                        };
                        heading
                            .into_iter()
                            .chain(std::iter::once(event))
                            .collect::<Vec<_>>()
                    })),
            );
        if events.is_empty() {
            view = view.child(if tasks {
                empty_state(
                    IconName::SquareCheck,
                    "No tasks",
                    "Add one from the context menu or with C in a local calendar.",
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
        view.into_any_element()
    }
}
