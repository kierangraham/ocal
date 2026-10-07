//! Day and week time grid.
use super::*;

impl Ocal {
    pub(super) fn pointer_slot(&self, p: Point<Pixels>) -> Option<Slot> {
        self.grid_bounds.iter().find_map(|(date, b)| {
            if p.x >= b.left() && p.x <= b.right() && p.y >= b.top() && p.y <= b.bottom() {
                let fraction = (p.y - b.top()) / b.size.height;
                Some(Slot::new(*date, (fraction * 1440.) as u16))
            } else {
                None
            }
        })
    }
    pub(super) fn grid(
        &mut self,
        cursor_event: Option<&Event>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let compact = self.layout(window).compact;
        // Dates from a previous period would otherwise still claim pointer hits.
        self.grid_bounds.clear();
        let days = self.visible_days(compact);
        let events = self.events(days[0], *days.last().unwrap() + Duration::days(1), cx);
        let today = self.today();
        let now = Utc::now();
        let target = cx.entity().downgrade();
        let weekend = |d: NaiveDate| d.weekday().num_days_from_sunday().is_multiple_of(6);
        let mut header = div()
            .flex()
            .pl(rems(3.4))
            .border_b_1()
            .border_color(t.divider());
        for (i, date) in days.iter().enumerate() {
            let date = *date;
            let is_today = date == today;
            header = header.child(
                button(("day-heading", i), "", ButtonVariant::Secondary, cx)
                    .accessibility_label(date.format("%A %-d %B").to_string())
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(rems(2.6))
                    .border_0()
                    .gap(rems(0.4))
                    .bg(if weekend(date) {
                        t.foreground.opacity(0.02)
                    } else {
                        t.foreground.opacity(0.)
                    })
                    .hover(|s| s.bg(t.hover_fill()))
                    .child(
                        div()
                            .text_size(rems(0.72))
                            .text_color(if is_today { t.accent } else { t.secondary })
                            .child(date.format("%a").to_string()),
                    )
                    .child(day_number(
                        date.day(),
                        is_today,
                        t.bright,
                        rems(0.95),
                        rems(1.6),
                        &t,
                    ))
                    .children((date == self.date && days.len() > 1).then(|| {
                        div()
                            .absolute()
                            .left(rems(0.5))
                            .right(rems(0.5))
                            .bottom_0()
                            .h(px(2.))
                            .bg(t.accent)
                    }))
                    .on_click(cx.listener(move |app, _, _, cx| app.choose_date(date, cx))),
            );
        }
        let mut all_day = div()
            .flex()
            .min_h(rems(1.7))
            .border_b_1()
            .border_color(t.divider())
            .child(
                div()
                    .w(rems(3.4))
                    .flex_shrink_0()
                    .pt(rems(0.45))
                    .pr(rems(0.5))
                    .text_right()
                    .text_size(rems(0.55))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.secondary)
                    .child("ALL DAY"),
            );
        let mut counter = 30000;
        for (i, date) in days.iter().enumerate() {
            let date = *date;
            let items: Vec<_> = events
                .iter()
                .filter(|e| e.all_day.is_some() && !e.task && e.on_date(date, self.tz()))
                .collect();
            let mut day = div()
                .flex_1()
                .min_w_0()
                .px(rems(0.15))
                .py(rems(0.2))
                .flex()
                .flex_col()
                .gap(rems(0.12))
                .border_l_1()
                .border_color(t.divider())
                .when(weekend(date), |d| d.bg(t.foreground.opacity(0.02)));
            // Keep the row short; the rest are a click away in the day view.
            for e in items.iter().take(2) {
                day = day.child(self.event_chip(e, counter, agenda::Chip::Bar, cx));
                counter += 1;
            }
            if items.len() > 2 {
                day = day.child(
                    button(("all-day-more", i), "", ButtonVariant::Secondary, cx)
                        .h(rems(1.1))
                        .px(rems(0.3))
                        .border_0()
                        .justify_start()
                        .bg(t.foreground.opacity(0.))
                        .text_size(rems(0.62))
                        .text_color(t.secondary)
                        .child(format!("+{} more", items.len() - 2))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.choose_date(date, cx);
                            app.change_view(View::Day, window, cx);
                        })),
                );
            }
            all_day = all_day.child(day);
        }
        let now_local = now.with_timezone(&self.tz());
        let now_hours = now_local.hour() as f32 + now_local.minute() as f32 / 60.;
        let showing_today = days.contains(&today);
        let mut hours = div()
            .w(rems(3.4))
            .flex_shrink_0()
            .h(rems(HOUR * 24.))
            .relative()
            .flex()
            .flex_col();
        for h in 0..24 {
            // The current-time pill replaces an hour label it would overlap.
            let hidden = h == 0 || (showing_today && (h as f32 - now_hours).abs() < 0.3);
            // Centre each label on its hour line, as the line is what times refer to.
            hours = hours.child(
                div()
                    .h(rems(HOUR))
                    .relative()
                    .top(rems(-0.4))
                    .text_size(rems(0.62))
                    .text_color(t.secondary)
                    .pr(rems(0.5))
                    .text_right()
                    .when(!hidden, |d| d.child(format!("{h:02}:00"))),
            );
        }
        if showing_today {
            hours = hours.child(
                div()
                    .debug_selector(|| "now-pill".into())
                    .absolute()
                    .right(rems(0.25))
                    .top(rems(now_hours * HOUR - 0.45))
                    .px(rems(0.2))
                    .h(rems(0.9))
                    .flex()
                    .items_center()
                    .bg(t.accent)
                    .text_color(t.on_accent)
                    .text_size(rems(0.58))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(now_local.format("%H:%M").to_string()),
            );
        }
        // One shared layer of hour and half-hour lines beneath every column.
        let lines = div()
            .absolute()
            .top_0()
            .left(rems(3.4))
            .right_0()
            .h(rems(HOUR * 24.))
            .children((1..48).map(|half| {
                div()
                    .absolute()
                    .top(rems(half as f32 * HOUR / 2.))
                    .left_0()
                    .right_0()
                    .h(px(1.))
                    .bg(t.divider().opacity(if half % 2 == 0 { 0.65 } else { 0.22 }))
            }));
        let mut grid = div().w_full().relative().flex().child(lines).child(hours);
        let reveal = self.reveal_cursor;
        self.reveal_cursor = false;
        for (di, date) in days.iter().enumerate() {
            let date = *date;
            let bounds_target = target.clone();
            let mut col = div()
                .id(("grid-day", di))
                .debug_selector(move || format!("grid-day-{di}"))
                .w_full()
                .min_w_0()
                .h(rems(HOUR * 24.))
                .relative()
                .border_l_1()
                .border_color(t.divider())
                .when(date == today, |col| col.bg(t.accent.opacity(0.035)))
                .when(weekend(date) && date != today, |col| {
                    col.bg(t.foreground.opacity(0.02))
                })
                .on_prepaint(move |bounds, _, cx| {
                    let _ = bounds_target.update(cx, |app, _| {
                        app.grid_bounds.insert(date, bounds);
                    });
                });
            let tz = self.tz();
            let timed: Vec<_> = events
                .iter()
                .filter(|e| e.all_day.is_none() && !e.task && e.on_date(date, tz))
                .cloned()
                .collect();
            let intervals: Vec<_> = timed
                .iter()
                .map(|e| {
                    let s = e.start.with_timezone(&tz);
                    let end = e.end.with_timezone(&tz);
                    let start = if s.date_naive() < date {
                        0.
                    } else {
                        s.num_seconds_from_midnight() as f32 / 3600.
                    };
                    let end = if end.date_naive() > date {
                        24.
                    } else {
                        end.num_seconds_from_midnight() as f32 / 3600.
                    };
                    (start, end)
                })
                .collect();
            // The cursor sits beneath events; a crossed event is highlighted instead.
            if date == self.selection.cursor.day {
                let cursor = div()
                    .debug_selector(|| "time-cursor".into())
                    .absolute()
                    .left(px(2.))
                    .right(px(2.))
                    .top(rems(self.selection.cursor.minute as f32 / 60. * HOUR))
                    .h(px(2.))
                    .bg(t.accent)
                    .child(
                        div()
                            .absolute()
                            .left(-px(2.))
                            .top(-px(2.))
                            .size(px(6.))
                            .rounded_full()
                            .bg(t.accent),
                    );
                let scroll = self.grid_scroll.clone();
                col = col.child(cursor.on_prepaint(move |bounds, window, _| {
                    if !reveal {
                        return;
                    }
                    let viewport = scroll.bounds();
                    let delta = if bounds.top() < viewport.top() {
                        viewport.top() - bounds.top()
                    } else if bounds.bottom() > viewport.bottom() {
                        viewport.bottom() - bounds.bottom()
                    } else {
                        px(0.)
                    };
                    if delta != px(0.) {
                        let next =
                            (scroll.offset().y + delta).clamp(-scroll.max_offset().y, px(0.));
                        scroll.set_offset(point(px(0.), next));
                        window.refresh();
                    }
                }));
            }
            for placement in ocal_core::layout::lanes(&intervals) {
                let e = timed[placement.index].clone();
                let color = self.calendar_color(&e.calendar, cx);
                let ev = e.clone();
                let target = cx.entity().downgrade();
                let index = di * 1000 + placement.index;
                let duration = placement.end - placement.start;
                let compact_event = duration < 0.75;
                let past = e.end <= now;
                let declined = e.attendees.iter().any(|a| a.me && a.response == "declined");
                let highlighted = self
                    .selected
                    .as_ref()
                    .or(cursor_event)
                    .is_some_and(|s| s.id == e.id && s.start == e.start);
                let trigger = button(("grid-event", index), "", ButtonVariant::Secondary, cx)
                    .debug_selector(move || format!("grid-event-{index}"))
                    .accessibility_label(format!("{}, {}", e.title, e.time_label(tz)))
                    // Popover adds an auto-height wrapper: a percentage height
                    // resolves to content size there, rather than event duration.
                    .w_full()
                    .h(rems(duration * HOUR))
                    .min_h_0()
                    .flex_shrink_0()
                    .border_0()
                    .when(highlighted, |s| s.border_1())
                    .border_l_2()
                    .border_color(if highlighted { t.accent } else { color })
                    // Opaque fills keep grid lines and the time cursor out of titles.
                    .bg(tint(&t, color, if highlighted { 0.3 } else { 0.16 }))
                    .hover({
                        let hover = tint(&t, color, if highlighted { 0.34 } else { 0.26 });
                        move |s| s.bg(hover)
                    })
                    .px(rems(0.35))
                    .py(rems(if compact_event { 0.04 } else { 0.2 }))
                    .justify_start()
                    .items_start()
                    .overflow_hidden()
                    .when((past || declined) && !highlighted, |s| s.opacity(0.55))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(rems(0.16))
                            .children(highlighted.then(|| {
                                div()
                                    .debug_selector(move || format!("highlighted-event-{index}"))
                                    .absolute()
                                    .size(px(1.))
                            }))
                            .child(
                                div()
                                    .text_size(rems(if compact_event { 0.64 } else { 0.7 }))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(event_text(&t, color))
                                    .when(duration >= 1.5, |s| s.whitespace_normal())
                                    .line_clamp(if duration >= 1.5 { 2 } else { 1 })
                                    .text_ellipsis()
                                    .when(declined, |s| s.line_through())
                                    .child(e.title.clone()),
                            )
                            .children((!compact_event).then(|| {
                                div()
                                    .text_ellipsis()
                                    .text_size(rems(0.61))
                                    .text_color(t.secondary)
                                    .child(e.time_label(tz))
                            }))
                            .children((!e.location.is_empty() && duration >= 1.5).then(|| {
                                div()
                                    .text_ellipsis()
                                    .text_size(rems(0.6))
                                    .text_color(t.secondary)
                                    .child(e.location.clone())
                            })),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.pick_event(ev.clone(), window, cx)
                    }));
                let action = e.clone();
                col = col.child(
                    div()
                        .absolute()
                        .top(rems(placement.start * HOUR))
                        .h(rems((placement.end - placement.start) * HOUR))
                        .left(relative(placement.column as f32 / placement.columns as f32))
                        .w(relative(1. / placement.columns as f32))
                        .px(px(1.5))
                        .overflow_hidden()
                        .child(
                            menu(
                                ("grid-event-menu", index),
                                trigger,
                                self.event_items(&e),
                                move |index, window, cx| {
                                    let _ = target.update(cx, |app, cx| {
                                        app.event_action(action.clone(), index, window, cx)
                                    });
                                },
                            )
                            .mouse_button(MouseButton::Right),
                        ),
                );
            }
            let day_start = date.and_hms_opt(0, 0, 0).unwrap();
            let day_end = day_start + Duration::days(1);
            if let Some((s, e)) = self.selection.span()
                && e > day_start
                && s < day_end
            {
                let start = (s.max(day_start) - day_start).num_minutes() as f32 / 60.;
                let end = (e.min(day_end) - day_start).num_minutes() as f32 / 60.;
                let highlighted = div()
                    .debug_selector(move || format!("time-selection-{di}"))
                    .absolute()
                    .left(px(2.))
                    .right(px(2.))
                    .top(rems(start * HOUR))
                    .h(rems((end - start) * HOUR))
                    .border_1()
                    .border_color(t.accent)
                    .bg(t.accent.opacity(0.12))
                    .child(
                        div()
                            .px(rems(0.3))
                            .text_size(rems(0.64))
                            .text_color(t.accent)
                            .child(self.selection.label()),
                    );
                col = col.child(highlighted);
            }
            // Preview the event being edited where it will land.
            if let Some(f) = self.editor.as_ref()
                && let Some((s, e)) = f.span(cx)
            {
                let (s, e) = (s.with_timezone(&tz), e.with_timezone(&tz));
                let day_start = date.and_hms_opt(0, 0, 0).unwrap();
                let (s, e) = (s.naive_local(), e.naive_local());
                if e > day_start && s < day_start + Duration::days(1) {
                    let from = (s.max(day_start) - day_start).num_minutes() as f32 / 60.;
                    let to = ((e - day_start).num_minutes() as f32 / 60.).min(24.);
                    let title = f.title.read(cx).value().trim().to_string();
                    let color = f
                        .calendar
                        .read(cx)
                        .selected()
                        .map(|c| self.calendar_color(&c.value, cx))
                        .unwrap_or(t.accent);
                    col = col.child(
                        div()
                            .debug_selector(move || format!("draft-{di}"))
                            .absolute()
                            .left(px(1.5))
                            .right(px(1.5))
                            .top(rems(from * HOUR))
                            .h(rems((to - from) * HOUR))
                            .border_1()
                            .border_l_2()
                            .border_color(color)
                            .bg(tint(&t, color, 0.3))
                            .px(rems(0.35))
                            .overflow_hidden()
                            .text_size(rems(if to - from < 0.5 { 0.58 } else { 0.68 }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(event_text(&t, color))
                            .child(if title.is_empty() {
                                "New event".to_string()
                            } else {
                                title
                            }),
                    );
                }
            }
            if date == today {
                col = col.child(
                    div()
                        .debug_selector(|| "now-line".into())
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(rems(now_hours * HOUR))
                        .h(px(2.))
                        .bg(t.accent),
                );
            } else if showing_today {
                // A faint guide carries the current time across the rest of the week.
                col = col.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(rems(now_hours * HOUR))
                        .h(px(1.))
                        .bg(t.accent.opacity(0.35)),
                );
            }
            let slot_target = cx.entity().downgrade();
            let open_target = slot_target.clone();
            col = col
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, event: &gpui_kit::MouseDownEvent, window, cx| {
                        if app.editor.is_some() {
                            return;
                        }
                        if let Some(slot) = app.pointer_slot(event.position) {
                            app.selected = None;
                            app.date = slot.day;
                            if event.modifiers.shift {
                                app.selection.begin(slot);
                                app.dragging = true;
                            } else {
                                app.selection.cursor = slot;
                                app.selection.anchor = None;
                                if event.click_count == 2 {
                                    app.new_event(false, window, cx);
                                }
                            }
                            window.focus(&app.focus, cx);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |app, event: &gpui_kit::MouseDownEvent, _, cx| {
                        if let Some(slot) = app.pointer_slot(event.position) {
                            app.selection.cursor = slot;
                            app.selection.anchor = None;
                            app.date = slot.day;
                            if let Some(bounds) = app.grid_bounds.get(&slot.day) {
                                app.slot_anchor = ((event.position.x - bounds.left())
                                    / bounds.size.width)
                                    .clamp(0., 1.);
                            }
                            app.slot_menu = true;
                            cx.stop_propagation();
                        }
                        cx.notify();
                    }),
                );
            let items = vec![
                MenuItem::new("New event")
                    .icon(IconName::Plus)
                    .shortcut("C"),
                MenuItem::new("New task").icon(IconName::SquareCheck),
                MenuItem::new("Go to today")
                    .separator_before()
                    .icon(IconName::CalendarCheck)
                    .shortcut("T"),
            ];
            col = col.child(
                div()
                    .absolute()
                    .left(relative(self.slot_anchor))
                    .top(rems(self.selection.cursor.minute as f32 / 60. * HOUR))
                    .size(px(1.))
                    .child(
                        menu(
                            format!(
                                "slot-menu-{di}-{}-{}",
                                self.selection.cursor.minute,
                                (self.slot_anchor * 1000.) as u32
                            ),
                            ContextTarget(div().size(px(1.))),
                            items,
                            move |index, window, cx| {
                                let _ = slot_target.update(cx, |app, cx| match index {
                                    0 => app.new_event(false, window, cx),
                                    1 => app.new_event(true, window, cx),
                                    _ => app.go_today(cx),
                                });
                            },
                        )
                        .mouse_button(MouseButton::Right)
                        .open(self.slot_menu && date == self.selection.cursor.day)
                        .on_open_change(move |open, _, cx| {
                            let _ = open_target.update(cx, |app, cx| {
                                app.slot_menu = *open;
                                cx.notify();
                            });
                        }),
                    ),
            );
            grid = grid.child(div().flex_1().min_w_0().child(col));
        }
        let first = self.initial_grid_scroll || self.scroll_to_now;
        self.initial_grid_scroll = false;
        self.scroll_to_now = false;
        let initial_scroll = self.grid_scroll.clone();
        let scroll = div()
            .on_prepaint(move |bounds, window, _| {
                if !first {
                    return;
                }
                let hour = rems(HOUR).to_pixels(window.rem_size());
                // Stop just above 07:00 so its centred label isn't clipped, unless
                // the current time would be off screen below.
                let morning = hour * 7. - rems(0.6).to_pixels(window.rem_size());
                let now = hour * now_hours;
                let top = if showing_today && now > morning + bounds.size.height - hour {
                    now - bounds.size.height / 3.
                } else {
                    morning
                };
                // GPUI clamps the far end to the content during layout.
                let top = top.max(px(0.));
                initial_scroll.set_offset(point(px(0.), -top));
                window.refresh();
            })
            .id("time-scroll")
            .debug_selector(|| "time-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.grid_scroll)
            .child(grid)
            .on_mouse_move(cx.listener(|app, event: &gpui_kit::MouseMoveEvent, _, cx| {
                // Pointer moves arrive far more often than the 15-minute slot changes.
                if app.dragging
                    && let Some(slot) = app.pointer_slot(event.position)
                    && slot != app.selection.cursor
                {
                    app.selection.cursor = slot;
                    app.date = slot.day;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| {
                    if app.dragging {
                        app.dragging = false;
                        app.new_event(false, window, cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| {
                    if app.dragging {
                        app.dragging = false;
                        app.new_event(false, window, cx);
                    }
                }),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(all_day)
            .child(scroll)
            .into_any_element()
    }
}
