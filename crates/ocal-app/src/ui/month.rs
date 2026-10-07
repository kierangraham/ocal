//! Month view.
use super::*;

impl Ocal {
    pub(super) fn month(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let days = month_days(self.date);
        let events = self.events(days[0], days[41] + Duration::days(1), cx);
        let compact = window.viewport_size().width < px(COMPACT * self.zoom);
        let mut view = div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .flex()
                .border_b_1()
                .border_color(t.divider())
                .children(
                    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                        .into_iter()
                        .map(|s| {
                            div()
                                .flex_1()
                                .py(rems(0.6))
                                .text_center()
                                .text_color(t.secondary)
                                .text_size(rems(0.7))
                                .child(s)
                        }),
                ),
        );
        let today = Utc::now().with_timezone(&self.tz()).date_naive();
        for (week, dates) in days.chunks(7).enumerate() {
            let mut row = div().flex().flex_1().min_h_0();
            for (i, date) in dates.iter().enumerate() {
                let date = *date;
                let mut cell = div()
                    .id(("month-day", week * 7 + i))
                    .debug_selector(move || format!("month-day-{}", week * 7 + i))
                    .w_full()
                    .h(self.month_cell_height)
                    .min_w_0()
                    .min_h_0()
                    .border_b_1()
                    .border_l_1()
                    .border_color(t.divider())
                    .bg(if date == self.date {
                        t.accent.opacity(0.06)
                    } else if date.month() != self.date.month() {
                        t.inset
                    } else {
                        t.background
                    })
                    .p(rems(if compact { 0.15 } else { 0.4 }))
                    .flex()
                    .flex_col()
                    .gap(rems(0.2))
                    .overflow_hidden()
                    .child(
                        button(
                            ("month-date", week * 7 + i),
                            "",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .accessibility_label(date.format("%A %-d %B").to_string())
                        .border_0()
                        .p(rems(0.))
                        .h(rems(1.5))
                        .bg(t.foreground.opacity(0.))
                        .justify_end()
                        .child(day_number(
                            date.day(),
                            date == today,
                            if date.month() == self.date.month() {
                                t.foreground
                            } else {
                                t.secondary
                            },
                            rems(0.72),
                            rems(1.5),
                            &t,
                        ))
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.choose_date(date, cx);
                                if compact {
                                    app.change_view(View::Day, window, cx);
                                }
                            },
                        )),
                    );
                let day_events: Vec<_> = events
                    .iter()
                    .filter(|e| !e.task && e.on_date(date, self.tz()))
                    .collect();
                for (j, e) in day_events
                    .iter()
                    .take(if compact { 2 } else { 3 })
                    .enumerate()
                {
                    cell = cell.child(self.event_chip(e, week * 700 + i * 100 + j, true, cx));
                }
                if day_events.len() > if compact { 2 } else { 3 } {
                    cell = cell.child(div().text_color(t.secondary).text_size(rems(0.61)).child(
                        format!("+{} more", day_events.len() - if compact { 2 } else { 3 }),
                    ));
                }
                let target = cx.entity().downgrade();
                cell = cell.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, event: &gpui_kit::MouseDownEvent, window, cx| {
                        app.choose_date(date, cx);
                        if event.click_count == 2 {
                            app.new_event(false, window, cx);
                        }
                    }),
                );
                let size_target = cx.entity().downgrade();
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .on_prepaint(move |bounds, window, cx| {
                            let _ = size_target.update(cx, |app, cx| {
                                if app.month_cell_height != bounds.size.height {
                                    app.month_cell_height = bounds.size.height;
                                    cx.notify();
                                    window.refresh();
                                }
                            });
                        })
                        .child(
                            menu(
                                ("month-context", week * 7 + i),
                                ContextTarget(cell),
                                vec![
                                    MenuItem::new("New event").icon(IconName::Plus),
                                    MenuItem::new("New task").icon(IconName::SquareCheck),
                                ],
                                move |index, window, cx| {
                                    let _ = target.update(cx, |app, cx| {
                                        app.choose_date(date, cx);
                                        app.new_event(index == 1, window, cx);
                                    });
                                },
                            )
                            .mouse_button(MouseButton::Right),
                        ),
                );
            }
            view = view.child(row);
        }
        view.into_any_element()
    }
}
