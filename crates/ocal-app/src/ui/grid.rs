//! Day and week time grid.
use super::*;
use crate::appearance::{RADIUS_CONTROL, RADIUS_SMALL, Tones, inset_ring, mix, shadow};
use gpui_kit::SharedString;

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
        let multi = days.len() > 1;
        let mut header = div().flex().pl(rems(GUTTER));
        for (i, date) in days.iter().enumerate() {
            let date = *date;
            let is_today = date == today;
            let quiet = weekend(date) && !is_today;
            header = header.child(
                button(("day-heading", i), "", ButtonVariant::Secondary, cx)
                    .accessibility_label(date.format("%A %-d %B").to_string())
                    .flex_1()
                    .min_w_0()
                    .h(rems(2.375))
                    .border_0()
                    .rounded(RADIUS_CONTROL)
                    .gap(rems(0.45))
                    .bg(t.foreground.opacity(0.))
                    .hover(|s| s.bg(t.hover()))
                    .child(
                        div()
                            .text_size(rems(0.78))
                            .text_color(if is_today {
                                t.accent
                            } else if quiet {
                                t.faint()
                            } else {
                                t.muted()
                            })
                            .when(is_today, |d| d.font_weight(FontWeight::MEDIUM))
                            .child(date.format("%a").to_string()),
                    )
                    .child(
                        day_number(
                            date.day(),
                            is_today,
                            if quiet { t.faint() } else { t.bright },
                            rems(0.875),
                            rems(1.625),
                            &t,
                        )
                        .font_weight(FontWeight::SEMIBOLD),
                    )
                    // Headings open the day; arrows still move the date in place.
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.choose_date(date, cx);
                        if multi {
                            app.change_view(View::Day, window, cx);
                        }
                    })),
            );
        }
        let tz = self.tz();
        let day_items: Vec<Vec<&Event>> = days
            .iter()
            .map(|&date| {
                events
                    .iter()
                    .filter(|e| (e.all_day.is_some() || e.task) && e.on_date(date, tz))
                    .collect()
            })
            .collect();
        // The all-day row only appears when something is in it.
        let all_day = day_items.iter().any(|d| !d.is_empty()).then(|| {
            let mut row = div()
                .flex()
                .pb(rems(0.375))
                .border_b_1()
                .border_color(t.hair())
                .child(
                    div()
                        .w(rems(GUTTER))
                        .flex_shrink_0()
                        .pt(rems(0.25))
                        .pr(rems(0.625))
                        .text_right()
                        .text_size(rems(0.69))
                        .text_color(t.faint())
                        .child("all-day"),
                );
            let mut counter = 30000;
            for (i, (date, items)) in days.iter().zip(&day_items).enumerate() {
                let date = *date;
                let mut day = div()
                    .flex_1()
                    .min_w_0()
                    .px(rems(0.1875))
                    .flex()
                    .flex_col()
                    .gap(rems(0.125));
                // Keep the row short; the rest are a click away in the day view.
                for e in items.iter().take(ALL_DAY_ROWS) {
                    day = day.child(self.event_chip(e, counter, agenda::Chip::Bar, cx));
                    counter += 1;
                }
                if items.len() > ALL_DAY_ROWS {
                    day = day.child(
                        button(("all-day-more", i), "", ButtonVariant::Secondary, cx)
                            .h(rems(1.25))
                            .px(rems(0.375))
                            .border_0()
                            .rounded(RADIUS_SMALL)
                            .justify_start()
                            .bg(t.foreground.opacity(0.))
                            .hover(|s| s.bg(t.hover()).text_color(t.bright))
                            .text_size(rems(0.69))
                            .text_color(t.muted())
                            .child(format!("{} more", items.len() - ALL_DAY_ROWS))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.choose_date(date, cx);
                                app.change_view(View::Day, window, cx);
                            })),
                    );
                }
                row = row.child(day);
            }
            row
        });
        let now_local = now.with_timezone(&tz);
        let now_hours = now_local.hour() as f32 + now_local.minute() as f32 / 60.;
        let showing_today = days.contains(&today);
        let mut hours = div()
            .w(rems(GUTTER))
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
                    .top(rems(-0.42))
                    .text_size(rems(0.69))
                    .text_color(t.faint())
                    .pr(rems(0.625))
                    .text_right()
                    .when(!hidden, |d| d.child(format!("{h:02}:00"))),
            );
        }
        if showing_today {
            hours = hours.child(
                div()
                    .debug_selector(|| "now-pill".into())
                    .absolute()
                    .left(rems(0.625))
                    .top(rems(now_hours * HOUR - 0.5))
                    .px(rems(0.3125))
                    .h(rems(1.))
                    .flex()
                    .items_center()
                    .rounded(rems(0.3125))
                    .bg(t.danger)
                    .text_color(t.on_accent)
                    .text_size(rems(0.66))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(now_local.format("%H:%M").to_string()),
            );
        }
        // One shared layer of hour lines beneath every column.
        let lines = div()
            .absolute()
            .top_0()
            .left(rems(GUTTER))
            .right_0()
            .h(rems(HOUR * 24.))
            .children((1..24).map(|h| {
                div()
                    .absolute()
                    .top(rems(h as f32 * HOUR))
                    .left_0()
                    .right_0()
                    .h(px(1.))
                    .bg(t.hair())
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
                .border_color(t.foreground.opacity(0.03))
                .when(date == today, |col| col.bg(t.accent.opacity(0.04)))
                .when(weekend(date) && date != today, |col| col.bg(t.shade()))
                // Working hours stay clear; early mornings and evenings are shaded.
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(rems(WORK_START * HOUR))
                        .bg(t.shade()),
                )
                .child(
                    div()
                        .absolute()
                        .top(rems(WORK_END * HOUR))
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .bg(t.shade()),
                )
                .on_prepaint(move |bounds, _, cx| {
                    let _ = bounds_target.update(cx, |app, _| {
                        app.grid_bounds.insert(date, bounds);
                    });
                });
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
                    .left(px(4.))
                    .right(px(2.))
                    .top(rems(self.selection.cursor.minute as f32 / 60. * HOUR))
                    .h(px(2.))
                    .rounded_full()
                    .bg(t.accent.opacity(0.85))
                    .child(
                        div()
                            .absolute()
                            .left(-px(4.))
                            .top(-px(3.))
                            .size(px(8.))
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
            // Later lanes are drawn over the cascade of earlier ones, and the
            // selected event over everything.
            let mut placements = ocal_core::layout::lanes(&intervals);
            placements.sort_by_key(|p| (self.is_selected(&timed[p.index]), p.column));
            for placement in placements {
                let e = timed[placement.index].clone();
                let index = di * 1000 + placement.index;
                let block = self.grid_event(&e, index, &placement, cursor_event, now, cx);
                col = col.child(
                    div()
                        .absolute()
                        .top(rems(placement.start * HOUR))
                        .h(rems((placement.end - placement.start) * HOUR))
                        .left(relative(placement.column as f32 / placement.columns as f32))
                        .w(relative(placement.span / placement.columns as f32))
                        .px(px(2.))
                        .child(block),
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
                    .left(px(3.))
                    .right(px(3.))
                    .top(rems(start * HOUR))
                    .h(rems((end - start) * HOUR))
                    .rounded(RADIUS_SMALL)
                    .shadow(inset_ring(t.accent, 1.5))
                    .bg(t.accent.opacity(0.14))
                    .px(rems(0.5))
                    .py(rems(0.2))
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(rems(0.69))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.accent)
                            .text_ellipsis()
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
                    let title = if title.is_empty() {
                        "New event".to_string()
                    } else {
                        title
                    };
                    let range = format!("{} – {}", s.format("%H:%M"), e.format("%H:%M"));
                    col = col.child(
                        div()
                            .debug_selector(move || format!("draft-{di}"))
                            .absolute()
                            .left(px(3.))
                            .right(px(3.))
                            .top(rems(from * HOUR))
                            .h(rems((to - from) * HOUR))
                            .rounded(RADIUS_SMALL)
                            .shadow(inset_ring(t.accent, 1.5))
                            .bg(t.accent.opacity(0.14))
                            .px(rems(0.5))
                            .py(rems(if to - from < 0.5 { 0. } else { 0.2 }))
                            .overflow_hidden()
                            .text_size(rems(0.72))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.accent)
                            .child(
                                div()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(format!("{title} · {range}")),
                            ),
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
                        .top(rems(now_hours * HOUR - 0.0625))
                        .h(px(2.))
                        .rounded_full()
                        .bg(t.danger)
                        .child(
                            div()
                                .absolute()
                                .left(-px(4.))
                                .top(-px(3.))
                                .size(px(8.))
                                .rounded_full()
                                .bg(t.danger),
                        ),
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
                        .bg(t.danger.opacity(0.4)),
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
            .children(all_day)
            .child(scroll)
            .into_any_element()
    }
    /// One timed event block. Its height is exactly its duration; neighbours are
    /// kept apart by a ring in the background colour instead of a gap.
    fn grid_event(
        &self,
        e: &Event,
        index: usize,
        placement: &ocal_core::layout::Placement,
        cursor_event: Option<&Event>,
        now: chrono::DateTime<Utc>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let tz = self.tz();
        let color = self.calendar_color(&e.calendar, cx);
        let duration = placement.end - placement.start;
        let short = duration <= 0.5;
        let past = e.end <= now;
        let declined = e.attendees.iter().any(|a| a.me && a.response == "declined");
        let selected = self.is_selected(e);
        let crossed = !selected && cursor_event.is_some_and(|s| s.id == e.id && s.start == e.start);
        let (fill, hover, text) = if selected {
            (color, color, t.on_accent)
        } else if past || declined {
            (
                tint(&t, color, 0.09),
                tint(&t, color, 0.16),
                mix(&t, color, 0.3, t.muted()),
            )
        } else {
            (
                tint(&t, color, if crossed { 0.26 } else { 0.16 }),
                tint(&t, color, 0.26),
                event_text(&t, color),
            )
        };
        let mut shadows = vec![shadow(t.background, 0., 0., 1.5)];
        if selected {
            shadows.push(shadow(color.opacity(0.5), 6., 18., -6.));
        } else if crossed {
            shadows.extend(inset_ring(t.accent, 1.));
        }
        let start = e.start.with_timezone(&tz).format("%H:%M").to_string();
        let title = div()
            .min_w_0()
            .text_size(rems(0.75))
            .font_weight(FontWeight::SEMIBOLD)
            .text_ellipsis()
            .when(duration >= 1.5, |s| s.whitespace_normal().line_clamp(2))
            .when(declined, |s| s.line_through())
            .child(e.title.clone());
        let secondary = |s: SharedString| {
            div()
                .flex_shrink_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(rems(0.69))
                .text_color(text.opacity(0.75))
                .child(s)
        };
        let body = if short {
            div()
                .w_full()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .gap(rems(0.375))
                .child(title)
                .child(secondary(start.into()))
        } else {
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(rems(0.06))
                .child(title)
                .child(secondary(e.time_label(tz).into()))
                .children(
                    (!e.location.is_empty() && duration >= 1.5)
                        .then(|| secondary(e.location.clone().into())),
                )
        };
        let ev = e.clone();
        let trigger = button(("grid-event", index), "", ButtonVariant::Secondary, cx)
            .debug_selector(move || format!("grid-event-{index}"))
            .accessibility_label(format!("{}, {}", e.title, e.time_label(tz)))
            // Popover adds an auto-height wrapper: a percentage height
            // resolves to content size there, rather than event duration.
            .relative()
            .w_full()
            .h(rems(duration * HOUR))
            .min_h_0()
            .flex_shrink_0()
            .border_0()
            .rounded(RADIUS_SMALL)
            .bg(fill)
            .hover(move |s| s.bg(hover))
            .shadow(shadows)
            .pl(rems(0.6875))
            .pr(rems(0.375))
            .py(rems(if short { 0. } else { 0.25 }))
            .justify_start()
            .items_start()
            .overflow_hidden()
            .text_color(text)
            .child(
                div()
                    .absolute()
                    .left(rems(0.25))
                    .top(px(3.))
                    .bottom(px(3.))
                    .w(px(3.))
                    .rounded_full()
                    .bg(if selected {
                        gpui_kit::black().opacity(0.25)
                    } else if past || declined {
                        color.opacity(0.55)
                    } else {
                        color
                    }),
            )
            .children((selected || crossed).then(|| {
                div()
                    .debug_selector(move || format!("highlighted-event-{index}"))
                    .absolute()
                    .size(px(1.))
            }))
            .child(body)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(
                cx.listener(move |app, _, window, cx| app.pick_event(ev.clone(), window, cx)),
            );
        let trigger = self.anchor_event(trigger, e, cx);
        let action = e.clone();
        let target = cx.entity().downgrade();
        menu(
            ("grid-event-menu", index),
            trigger,
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
}

/// Width of the hour-label gutter.
const GUTTER: f32 = 3.625;
/// All-day items shown per day before "N more".
const ALL_DAY_ROWS: usize = 3;
/// Hours outside this range are shaded as off-hours.
const WORK_START: f32 = 8.;
const WORK_END: f32 = 18.;
