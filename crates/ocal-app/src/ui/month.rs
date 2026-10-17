//! Month view.
use super::*;
use crate::appearance::{Tones, inset_ring};
use agenda::Chip;

impl Ocal {
    pub(super) fn month(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let days = month_days(self.date);
        let events = self.events(days[0], days[41] + Duration::days(1), cx);
        let compact = self.layout(window).compact;
        // Fit as many items as the measured cell height allows.
        let rem = window.rem_size();
        let fit = (((self.month_cell_height - rems(2.2).to_pixels(rem))
            / rems(1.375).to_pixels(rem))
        .floor() as usize)
            .max(1);
        let mut view = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px(rems(0.75))
            .pb(rems(0.75))
            .child(
                div().flex().children(
                    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                        .into_iter()
                        .map(|s| {
                            div()
                                .flex_1()
                                .px(rems(0.625))
                                .pb(rems(0.5))
                                .text_color(t.muted())
                                .text_size(rems(0.75))
                                .child(s)
                        }),
                ),
            );
        let today = self.today();
        for (week, dates) in days.chunks(7).enumerate() {
            let mut row = div().flex().flex_1().min_h_0();
            for (i, date) in dates.iter().enumerate() {
                let date = *date;
                let outside = date.month() != self.date.month();
                let is_today = date == today;
                let selected = date == self.date && !is_today;
                let mut cell = div()
                    .id(("month-day", week * 7 + i))
                    .debug_selector(move || format!("month-day-{}", week * 7 + i))
                    .w_full()
                    .h(self.month_cell_height)
                    .min_w_0()
                    .min_h_0()
                    .border_t_1()
                    .border_color(t.hair())
                    .relative()
                    // Columns are only faintly divided; weeks carry the lines.
                    .when(i > 0, |c| {
                        c.child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(1.))
                                .bg(t.foreground.opacity(0.03)),
                        )
                    })
                    .when(
                        date.weekday().num_days_from_sunday().is_multiple_of(6),
                        |c| c.bg(t.shade()),
                    )
                    .p(rems(if compact { 0.15 } else { 0.3125 }))
                    .flex()
                    .flex_col()
                    .gap(rems(0.0625))
                    .overflow_hidden()
                    .child(
                        // The date opens that day; the first of a month names it.
                        div().flex_shrink_0().pb(rems(0.1875)).child(
                            button(
                                ("month-date", week * 7 + i),
                                if date.day() == 1 {
                                    date.format("%-d %b").to_string()
                                } else {
                                    date.day().to_string()
                                },
                                ButtonVariant::Secondary,
                                cx,
                            )
                            .accessibility_label(date.format("%A %-d %B").to_string())
                            .border_0()
                            .min_w(rems(1.5))
                            .h(rems(1.5))
                            .px(rems(0.3125))
                            .rounded_full()
                            .text_size(rems(0.75))
                            .font_weight(if outside {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .text_color(if is_today {
                                t.on_accent
                            } else if outside {
                                t.faint()
                            } else {
                                t.bright
                            })
                            .bg(if is_today {
                                t.accent
                            } else {
                                t.foreground.opacity(0.)
                            })
                            .when(selected, |b| b.shadow(inset_ring(t.accent, 1.5)))
                            .hover({
                                let fill = if is_today { t.accent } else { t.hover() };
                                move |s| s.bg(fill)
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(
                                move |app, _, window, cx| {
                                    app.choose_date(date, cx);
                                    app.change_view(View::Day, window, cx);
                                },
                            )),
                        ),
                    );
                // All-day items first, then timed events in start order.
                let mut day_events: Vec<_> = events
                    .iter()
                    .filter(|e| e.on_date(date, self.tz()))
                    .collect();
                day_events.sort_by_key(|e| (e.all_day.is_none() && !e.task, e.start));
                let shown = if day_events.len() > fit {
                    fit.saturating_sub(1)
                } else {
                    fit
                };
                for (j, e) in day_events.iter().take(shown).enumerate() {
                    let style = if e.all_day.is_some() || e.task {
                        Chip::Bar
                    } else {
                        Chip::Dot
                    };
                    let chip = self.event_chip(e, week * 700 + i * 100 + j, style, cx);
                    // Neighbouring months' events stay readable but recede.
                    cell = cell.child(div().when(outside, |d| d.opacity(0.45)).child(chip));
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
                        .h(rems(1.125))
                        .px(rems(0.375))
                        .flex_shrink_0()
                        .border_0()
                        .rounded(rems(0.3125))
                        .justify_start()
                        .bg(t.foreground.opacity(0.))
                        .hover(|s| s.bg(t.hover()).text_color(t.bright))
                        .text_size(rems(0.69))
                        .text_color(t.muted())
                        .child(format!("{} more", day_events.len() - shown))
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
