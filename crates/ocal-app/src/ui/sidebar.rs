//! Mini month, calendar list and upcoming events.
use super::*;
use crate::appearance::{
    RADIUS_CONTROL, RADIUS_FIELD, RADIUS_SMALL, Tones, accent_glow, nav_row, section_label,
    send_button, shadow, swatch, with_tip,
};
use gpui_kit::{linear_color_stop, linear_gradient};

impl Ocal {
    fn mini_month(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let cell = rems(1.75);
        let days = month_days(self.date);
        let today = self.today();
        let tz = self.tz();
        // One expansion for the whole grid marks the days that have events.
        let mut busy = HashSet::new();
        let events = self.events(days[0], days[41] + Duration::days(1), cx);
        for e in events.into_iter().filter(|e| !e.completed) {
            let (mut d, end) = e.dates_in(tz);
            while d < end && d <= days[41] {
                busy.insert(d);
                d += Duration::days(1);
            }
        }
        let week = (self.view == View::Week).then(|| week_start(self.date));
        let mut grid = div().flex().flex_col();
        for (row, dates) in days.chunks(7).enumerate() {
            let in_view = week == Some(dates[0]);
            grid = grid.child(
                div()
                    .flex()
                    .when(in_view, |r| r.bg(t.hover_fill()))
                    .children(dates.iter().enumerate().map(|(i, d)| {
                        let d = *d;
                        let selected = d == self.date;
                        let is_today = d == today;
                        let color = if is_today {
                            t.on_accent
                        } else if d.month() != self.date.month() {
                            t.secondary.opacity(0.55)
                        } else {
                            t.foreground
                        };
                        button(("mini", row * 7 + i), "", ButtonVariant::Secondary, cx)
                            .accessibility_label(d.format("%A %-d %B %Y").to_string())
                            .relative()
                            .size(cell)
                            .p(rems(0.))
                            .border_0()
                            .text_size(rems(0.7))
                            .text_color(color)
                            .bg(if is_today {
                                t.accent
                            } else if selected {
                                t.selection
                            } else {
                                t.foreground.opacity(0.)
                            })
                            .hover({
                                let fill = t.hover_fill();
                                move |s| if is_today { s } else { s.bg(fill) }
                            })
                            .when(is_today || selected, |b| {
                                b.font_weight(FontWeight::SEMIBOLD)
                            })
                            .child(d.day().to_string())
                            .children((busy.contains(&d) && !is_today).then(|| {
                                div()
                                    .absolute()
                                    .bottom(rems(0.2))
                                    .left(relative(0.5))
                                    .ml(px(-1.5))
                                    .size(px(3.))
                                    .rounded_full()
                                    .bg(if selected { t.accent } else { t.secondary })
                            }))
                            .on_click(cx.listener(move |app, _, _, cx| app.choose_date(d, cx)))
                    })),
            );
        }
        div()
            .px(rems(0.75))
            .pt(rems(0.8))
            .child(
                div()
                    .flex()
                    .items_center()
                    .pl(rems(0.25))
                    .pb(rems(0.4))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.bright)
                            .child(self.date.format("%B %Y").to_string()),
                    )
                    .child(
                        tool_button("mini-prev", IconName::ChevronLeft, "Previous month", "", cx)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.choose_date(move_month(app.date, -1), cx)
                            })),
                    )
                    .child(
                        tool_button("mini-next", IconName::ChevronRight, "Next month", "", cx)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.choose_date(move_month(app.date, 1), cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .children(["S", "M", "T", "W", "T", "F", "S"].into_iter().map(|s| {
                        div()
                            .w(cell)
                            .pb(rems(0.2))
                            .text_center()
                            .text_size(rems(0.6))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.secondary)
                            .child(s)
                    })),
            )
            .child(grid)
            .into_any_element()
    }
    /// Calendar sets as rows; selecting one filters every view.
    fn calendar_sets(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.snapshot.sets.is_empty() {
            return None;
        }
        let rows = std::iter::once((None, "All calendars".to_string()))
            .chain(
                self.snapshot
                    .sets
                    .iter()
                    .map(|s| (Some(s.id.clone()), s.name.clone())),
            )
            .enumerate()
            .map(|(i, (id, name))| {
                nav_row(("calendar-set", i), self.active_set == id, cx)
                    .debug_selector(move || format!("calendar-set-{i}"))
                    .child(icon(IconName::Layers).size(rems(0.8)))
                    .child(div().flex_1().min_w_0().text_ellipsis().child(name))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.active_set = id.clone();
                        app.remember("set", id.clone().unwrap_or_default());
                        app.selected = None;
                        window.focus(&app.focus, cx);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        Some(
            div()
                .child(section_label("SETS", cx))
                .children(rows)
                .into_any_element(),
        )
    }
    fn calendar_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut list = div().child(section_label("CALENDARS", cx));
        for (group_index, (account, indices)) in
            calendar_groups(&self.snapshot).into_iter().enumerate()
        {
            if indices.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .debug_selector(move || format!("calendar-group-{group_index}"))
                    .px(rems(0.75))
                    .pt(rems(0.35))
                    .pb(rems(0.15))
                    .text_size(rems(0.66))
                    .text_color(t.secondary)
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(account.unwrap_or_else(|| "On this computer".into())),
            );
            for i in indices {
                let c = self.snapshot.calendars[i].clone();
                let visible = self.visible(&c.id);
                let color = self.calendar_color(&c.id, cx);
                list = list.child(
                    nav_row(("calendar-row", i), false, cx)
                        .debug_selector(move || format!("calendar-row-{i}"))
                        .accessibility_label(format!(
                            "{} calendar, {}",
                            c.display_name(),
                            if visible { "visible" } else { "hidden" }
                        ))
                        .h(rems(1.7))
                        .child(swatch(color, visible))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_color(if visible { t.foreground } else { t.secondary })
                                .child(c.name.clone()),
                        )
                        .children((!c.writable).then(|| {
                            div()
                                .text_size(rems(0.6))
                                .text_color(t.secondary)
                                .child("read-only")
                        }))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            window.focus(&app.focus, cx);
                            let mut c = c.clone();
                            c.visible = !c.visible;
                            if let Some(current) =
                                app.snapshot.calendars.iter_mut().find(|x| x.id == c.id)
                            {
                                current.visible = c.visible;
                            }
                            if app.active_set.take().is_some() {
                                app.remember("set", "");
                            }
                            app.dispatch(Command::Visible(c));
                            cx.notify();
                        })),
                );
            }
        }
        list.child(
            nav_row("sidebar-accounts", false, cx)
                .debug_selector(|| "sidebar-accounts".into())
                .h(rems(1.7))
                .text_color(t.secondary)
                .child(icon(IconName::Plus).size(rems(0.8)))
                .child("Add Google account")
                .on_click(cx.listener(|app, _, window, cx| app.show_settings(window, cx))),
        )
        .into_any_element()
    }
    /// The next few events from now, labelled relative to today.
    fn up_next(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let today = self.today();
        let now = Utc::now();
        let tz = self.tz();
        let upcoming: Vec<_> = self
            .events(today, today + Duration::days(7), cx)
            .into_iter()
            .filter(|e| !e.task && !e.completed && e.end > now)
            .take(8)
            .collect();
        let mut list = div().pb(rems(0.6)).child(section_label("UP NEXT", cx));
        if upcoming.is_empty() {
            list = list.child(
                div()
                    .px(rems(0.75))
                    .text_size(rems(0.73))
                    .text_color(t.secondary)
                    .child("Nothing else this week."),
            );
        }
        for (i, e) in upcoming.into_iter().enumerate() {
            let color = self.calendar_color(&e.calendar, cx);
            let day = e.dates_in(tz).0.max(today);
            let when = format!("{} · {}", relative_day(day, today), e.time_label(tz));
            let now_on = e.all_day.is_none() && e.start <= now;
            let selected = self
                .selected
                .as_ref()
                .is_some_and(|s| s.id == e.id && s.start == e.start);
            list =
                list.child(
                    nav_row(("upcoming", i), selected, cx)
                        .h(rems(2.5))
                        .gap(rems(0.55))
                        .child(div().w(px(3.)).h(rems(1.7)).flex_shrink_0().bg(color))
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
                                .child(
                                    div()
                                        .text_size(rems(0.66))
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(if now_on { t.accent } else { t.secondary })
                                        .child(if now_on {
                                            format!("Now · {}", e.time_label(tz))
                                        } else {
                                            when
                                        }),
                                ),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.pick_event(e.clone(), window, cx)
                        })),
                );
        }
        list.into_any_element()
    }
    /// Brand, demo badge and the primary New event button.
    fn sidebar_top(&self, overlay: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        // The palette has no violet; a hue turn from the accent stands in for it.
        let violet = gpui_kit::Hsla {
            h: (t.accent.h + 0.11).fract(),
            ..t.accent
        };
        let brand = div()
            .flex()
            .items_center()
            .gap(rems(0.625))
            .px(rems(0.375))
            .pt(rems(0.25))
            .pb(rems(1.))
            .child(
                div()
                    .size(rems(1.625))
                    .flex_shrink_0()
                    .rounded(RADIUS_CONTROL)
                    .bg(linear_gradient(
                        140.,
                        linear_color_stop(t.accent, 0.),
                        linear_color_stop(violet, 1.),
                    ))
                    .text_color(t.on_accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(IconName::CalendarRange).size(rems(0.95))),
            )
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(rems(0.94))
                    .text_color(t.bright)
                    .child("Ocal"),
            )
            .when(self.demo, |row| {
                row.child(with_tip(
                    div()
                        .id("demo-badge")
                        .debug_selector(|| "demo-badge".into())
                        .px(rems(0.4375))
                        .py(rems(0.1875))
                        .rounded_full()
                        .bg(t.warning.opacity(0.14))
                        .text_size(rems(0.625))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.warning)
                        .child("DEMO"),
                    "Sample calendar · changes stay in this session",
                ))
            })
            // Floating over a narrow window, the sidebar carries its own close button.
            .when(overlay, |row| {
                row.child(
                    tool_button(
                        "close-sidebar",
                        IconName::PanelLeft,
                        "Hide sidebar",
                        "b",
                        cx,
                    )
                    .mr(rems(-0.375))
                    .on_click(cx.listener(|app, _, window, cx| app.toggle_sidebar(window, cx))),
                )
            });
        div()
            .flex_shrink_0()
            .px(rems(0.75))
            .pt(rems(0.875))
            .child(brand)
            .child(
                send_button(
                    "new-event",
                    "New event",
                    Some(IconName::Plus),
                    Some("c"),
                    cx,
                )
                .debug_selector(|| "new-event".into())
                .w_full()
                .h(rems(2.375))
                .gap(rems(0.5625))
                .rounded(RADIUS_FIELD)
                .text_size(rems(0.875))
                .shadow(accent_glow(&t))
                .on_click(cx.listener(|app, _, window, cx| app.new_event(false, window, cx))),
            )
            .into_any_element()
    }
    pub(super) fn sidebar(&self, overlay: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .debug_selector(|| "sidebar".into())
            .w(rems(SIDEBAR))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(self.sidebar_top(overlay, cx))
            .child(self.mini_month(cx))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(self.calendar_sets(cx))
                    .child(self.calendar_list(cx))
                    .child(self.up_next(cx)),
            )
            .child(self.footer(cx))
            .into_any_element()
    }

    /// The footer's sync line: what to say, and the dot's colour.
    pub(super) fn sync_status(&self, cx: &App) -> (String, gpui_kit::Hsla) {
        let t = cx.omarchy();
        let signing_in = self.busy && self.account_note.as_ref().is_some_and(|(_, e)| !e);
        if self.demo {
            ("Demo calendar".into(), t.muted())
        } else if signing_in {
            ("Signing in…".into(), t.accent)
        } else if self.snapshot.accounts.is_empty() {
            ("On this computer".into(), t.muted())
        } else if self.sync.syncing {
            ("Syncing…".into(), t.accent)
        } else if self.sync.offline {
            ("Offline".into(), t.muted())
        } else if self.sync.error.is_some() {
            ("Sync failed".into(), t.danger)
        } else if let Some(synced) = self.sync.synced_label(Utc::now()) {
            (synced, t.success)
        } else {
            ("Not synced yet".into(), t.muted())
        }
    }

    /// Sync status, the light/dark toggle and Settings, like Omail's footer.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let (text, color) = self.sync_status(cx);
        // A failure explains itself on hover.
        let detail = self
            .sync
            .error
            .clone()
            .filter(|_| !self.demo && !self.sync.syncing && !self.snapshot.accounts.is_empty());
        let status = div()
            .id("sync-status")
            .debug_selector(|| "sync-status".into())
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(rems(0.5))
            .child(
                div()
                    .size(rems(0.375))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(color)
                    .shadow(vec![shadow(color.opacity(0.18), 0., 0., 3.)]),
            )
            .child(div().min_w_0().text_ellipsis().child(text));
        let status = match detail {
            Some(detail) => with_tip(status, detail),
            None => status,
        };
        let dark = t.dark();
        let small = |b: gpui_omarchy::Button| b.size(rems(1.75)).rounded(RADIUS_SMALL);
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rems(0.25))
            .pl(rems(1.125))
            .pr(rems(0.875))
            .pt(rems(0.5))
            .pb(rems(0.75))
            .whitespace_nowrap()
            .text_size(rems(0.75))
            .text_color(t.muted())
            .child(status)
            .child(small(
                tool_button(
                    "appearance-toggle",
                    if dark { IconName::Sun } else { IconName::Moon },
                    if dark { "Light mode" } else { "Dark mode" },
                    "",
                    cx,
                )
                .on_click(cx.listener(|app, _, _, cx| app.toggle_appearance(cx))),
            ))
            .child(small(
                tool_button("settings", IconName::Settings, "Settings", "", cx)
                    .on_click(cx.listener(|app, _, window, cx| app.show_settings(window, cx))),
            ))
            .into_any_element()
    }
}

/// "Today", "Tomorrow" or a short weekday and date.
pub(super) fn relative_day(day: NaiveDate, today: NaiveDate) -> String {
    match (day - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        2..=6 => day.format("%A").to_string(),
        _ => day.format("%a %-d %b").to_string(),
    }
}

/// Include calendar-owned account IDs as well as saved accounts, so cached calendars
/// remain visible even if an older build did not persist their account record.
pub(super) fn calendar_groups(snapshot: &Snapshot) -> Vec<(Option<String>, Vec<usize>)> {
    let mut accounts: std::collections::BTreeSet<String> = snapshot
        .accounts
        .iter()
        .map(|account| account.id.clone())
        .collect();
    accounts.extend(
        snapshot
            .calendars
            .iter()
            .filter_map(|calendar| calendar.account.clone()),
    );
    std::iter::once(None)
        .chain(accounts.into_iter().map(Some))
        .map(|account| {
            let indices = snapshot
                .calendars
                .iter()
                .enumerate()
                .filter(|(_, calendar)| calendar.account == account)
                .map(|(i, _)| i)
                .collect();
            (account, indices)
        })
        .collect()
}
