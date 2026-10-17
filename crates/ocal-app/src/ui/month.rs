//! Month view.
use super::*;
use agenda::Chip;

impl Ocal {
    pub(super) fn month(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let days = month_days(self.date);
        let events: Vec<_> = self
            .events(days[0], days[41] + Duration::days(1), cx)
            .into_iter()
            .filter(|e| !e.completed)
            .collect();
        let compact = self.layout(window).compact;
        // Fit as many items as the measured cell height allows.
        let rem = window.rem_size();
        let fit =
            (((self.month_cell_height - rems(1.9).to_pixels(rem)) / rems(1.27).to_pixels(rem))
                .floor() as usize)
                .max(1);
        let mut view = div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .flex()
                .border_b_1()
                .border_color(t.divider())
                .children(
                    ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
                        .into_iter()
                        .map(|s| {
                            div()
                                .flex_1()
                                .px(rems(0.5))
                                .py(rems(0.45))
                                .text_color(t.secondary)
                                .text_size(rems(0.62))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(s)
                        }),
                ),
        );
        let today = self.today();
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
                    .relative()
                    .bg(if date.month() != self.date.month() {
                        t.inset
                    } else if date.weekday().num_days_from_sunday().is_multiple_of(6) {
                        t.background.blend(t.foreground.opacity(0.02))
                    } else {
                        t.background
                    })
                    .p(rems(if compact { 0.15 } else { 0.25 }))
                    .flex()
                    .flex_col()
                    .gap(rems(0.12))
                    .overflow_hidden()
                    .children((date == self.date).then(|| {
                        div()
                            .absolute()
                            .inset_0()
                            .border_1()
                            .border_color(t.accent.opacity(0.7))
                    }))
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
                        .h(rems(1.4))
                        .flex_shrink_0()
                        .bg(t.foreground.opacity(0.))
                        .justify_start()
                        .child(day_number(
                            date.day(),
                            date == today,
                            if date.month() == self.date.month() {
                                t.foreground
                            } else {
                                t.secondary
                            },
                            rems(0.72),
                            rems(1.4),
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
                // All-day items first, then timed events in start order.
                let mut day_events: Vec<_> = events
                    .iter()
                    .filter(|e| !e.task && e.on_date(date, self.tz()))
                    .collect();
                day_events.sort_by_key(|e| (e.all_day.is_none(), e.start));
                let shown = if day_events.len() > fit {
                    fit.saturating_sub(1)
                } else {
                    fit
                };
                for (j, e) in day_events.iter().take(shown).enumerate() {
                    let style = if e.all_day.is_some() {
                        Chip::Bar
                    } else {
                        Chip::Dot
                    };
                    cell = cell.child(self.event_chip(e, week * 700 + i * 100 + j, style, cx));
                }
                if day_events.len() > shown {
                    cell = cell.child(
                        button(
                            ("month-more", week * 7 + i),
                            "",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .debug_selector(move || format!("month-more-{}", week * 7 + i))
                        .h(rems(1.1))
                        .px(rems(0.3))
                        .flex_shrink_0()
                        .border_0()
                        .justify_start()
                        .bg(t.foreground.opacity(0.))
                        .hover(|s| s.bg(t.hover_fill()))
                        .text_size(rems(0.62))
                        .text_color(t.secondary)
                        .child(format!("+{} more", day_events.len() - shown))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.choose_date(date, cx);
                                app.change_view(View::Day, window, cx);
                            },
                        )),
                    );
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
