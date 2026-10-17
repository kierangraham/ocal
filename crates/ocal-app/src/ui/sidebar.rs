//! Mini month, calendar list and upcoming events.
use super::*;
use crate::appearance::{
    RADIUS_CARD, RADIUS_CONTROL, RADIUS_FIELD, RADIUS_SMALL, Tones, accent_glow, inset_ring,
    pill_tab, send_button, shadow, with_tip,
};
use gpui_kit::{linear_color_stop, linear_gradient};

impl Ocal {
    fn mini_month(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let cell = rems(1.6875);
        let days = month_days(self.date);
        let today = self.today();
        let week = (self.view == View::Week).then(|| week_start(self.date));
        let mut grid = div().flex().flex_col().gap(px(1.));
        for (row, dates) in days.chunks(7).enumerate() {
            let in_view = week == Some(dates[0]);
            grid = grid.child(
                div()
                    .flex()
                    // The week in view is a band with rounded ends.
                    .when(in_view, |r| r.rounded_full().bg(t.hover()))
                    .children(dates.iter().enumerate().map(|(i, d)| {
                        let d = *d;
                        let selected = d == self.date;
                        let is_today = d == today;
                        let color = if is_today {
                            t.on_accent
                        } else if d.month() != self.date.month() {
                            t.faint()
                        } else if selected {
                            t.bright
                        } else {
                            t.foreground
                        };
                        div()
                            .flex_1()
                            .h(cell)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                button(("mini", row * 7 + i), "", ButtonVariant::Secondary, cx)
                                    .accessibility_label(d.format("%A %-d %B %Y").to_string())
                                    .size(rems(1.5625))
                                    .p(rems(0.))
                                    .border_0()
                                    .rounded_full()
                                    .text_size(rems(0.75))
                                    .text_color(color)
                                    .bg(if is_today {
                                        t.accent
                                    } else {
                                        t.foreground.opacity(0.)
                                    })
                                    .when(selected && !is_today, |b| {
                                        b.shadow(inset_ring(t.accent, 1.5))
                                    })
                                    .hover({
                                        let fill = t.hover();
                                        let bright = t.bright;
                                        move |s| {
                                            if is_today {
                                                s
                                            } else {
                                                s.bg(fill).text_color(bright)
                                            }
                                        }
                                    })
                                    .when(is_today, |b| b.font_weight(FontWeight::SEMIBOLD))
                                    .child(d.day().to_string())
                                    .on_click(
                                        cx.listener(move |app, _, _, cx| app.choose_date(d, cx)),
                                    ),
                            )
                    })),
            );
        }
        let small = |b: gpui_omarchy::Button| b.size(rems(1.625)).rounded(RADIUS_SMALL);
        div()
            .px(rems(0.875))
            .pt(rems(1.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .pl(rems(0.5))
                    .pb(rems(0.25))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(rems(0.8125))
                            .text_color(t.bright)
                            .child(self.date.format("%B %Y").to_string()),
                    )
                    .child(small(
                        tool_button("mini-prev", IconName::ChevronLeft, "Previous month", "", cx)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.choose_date(move_month(app.date, -1), cx)
                            })),
                    ))
                    .child(small(
                        tool_button("mini-next", IconName::ChevronRight, "Next month", "", cx)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.choose_date(move_month(app.date, 1), cx)
                            })),
                    )),
            )
            .child(
                div()
                    .flex()
                    .children(
                        ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
                            .into_iter()
                            .map(|s| {
                                div()
                                    .flex_1()
                                    .h(rems(1.375))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(rems(0.66))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.faint())
                                    .child(s)
                            }),
                    ),
            )
            .child(grid)
            .into_any_element()
    }
    /// The timed event happening now, if any, as a card with its progress.
    fn current_event(&self, cx: &App) -> Option<Event> {
        let today = self.today();
        let now = Utc::now();
        self.events(today, today + Duration::days(1), cx)
            .into_iter()
            .filter(|e| e.all_day.is_none() && !e.task && e.start <= now && e.end > now)
            .min_by_key(|e| e.start)
    }
    fn now_card(&self, e: &Event, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let tz = self.tz();
        let now = Utc::now();
        let color = self.calendar_color(&e.calendar, cx);
        let total = (e.end - e.start).num_seconds().max(1) as f32;
        let done = ((now - e.start).num_seconds() as f32 / total).clamp(0., 1.);
        let left = (e.end - now).num_minutes().max(1);
        let when = format!(
            "{} – {}",
            e.start.with_timezone(&tz).format("%H:%M"),
            e.end.with_timezone(&tz).format("%H:%M")
        );
        let detail = if e.location.is_empty() {
            when
        } else {
            format!("{} · {when}", e.location)
        };
        let fill = t.surface.opacity(if t.dark() { 0.45 } else { 0.8 });
        let pick = e.clone();
        let card = div()
            .id("now-card")
            .debug_selector(|| "now-card".into())
            .mt(rems(0.875))
            .px(rems(0.875))
            .py(rems(0.75))
            .rounded(RADIUS_CARD)
            .bg(fill)
            .hover(|s| s.bg(t.selection))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.4375))
                    .text_size(rems(0.69))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.faint())
                    .child(
                        div()
                            .size(rems(0.375))
                            .rounded_full()
                            .bg(color)
                            .shadow(vec![shadow(color.opacity(0.2), 0., 0., 3.)]),
                    )
                    .child("Now")
                    .child(div().flex_1())
                    .child(format!("{} left", agenda::duration_label(left))),
            )
            .child(
                div()
                    .mt(rems(0.375))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.bright)
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(e.title.clone()),
            )
            .child(
                div()
                    .text_size(rems(0.75))
                    .text_color(t.muted())
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(detail),
            )
            .child(
                div()
                    .mt(rems(0.625))
                    .h(px(3.))
                    .rounded_full()
                    .bg(t.hair())
                    .child(div().h_full().w(relative(done)).rounded_full().bg(color)),
            )
            .on_click(
                cx.listener(move |app, _, window, cx| app.pick_event(pick.clone(), window, cx)),
            );
        self.anchor_event(card, e, cx).into_any_element()
    }
    /// A sentence-case sidebar heading, as in Omail.
    fn side_label(text: &'static str, cx: &App) -> gpui_kit::Div {
        div()
            .px(rems(0.625))
            .pt(rems(1.125))
            .pb(rems(0.25))
            .text_size(rems(0.69))
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.omarchy().faint())
            .child(text)
    }
    /// A 34 px list row: dot, name and a right-hand note.
    fn side_row(
        id: impl Into<gpui_kit::ElementId>,
        active: bool,
        cx: &App,
    ) -> gpui_omarchy::Button {
        let t = cx.omarchy();
        button(id, "", ButtonVariant::Secondary, cx)
            .w_full()
            .h(rems(2.125))
            .px(rems(0.625))
            .gap(rems(0.6875))
            .justify_start()
            .border_0()
            .rounded(RADIUS_CONTROL)
            .text_size(rems(0.84))
            .text_color(t.foreground)
            .bg(if active {
                t.selection
            } else {
                t.foreground.opacity(0.)
            })
            .hover(|s| s.bg(t.hover()))
    }
    fn side_dot(color: gpui_kit::Hsla, filled: bool) -> gpui_kit::Div {
        div()
            .mx(rems(0.25))
            .size(rems(0.5))
            .flex_shrink_0()
            .rounded_full()
            .map(|d| {
                if filled {
                    d.bg(color)
                } else {
                    d.shadow(inset_ring(color, 1.5))
                }
            })
    }
    /// The next few items after the one happening now, grouped by day.
    fn up_next(&self, current: Option<&Event>, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let today = self.today();
        let now = Utc::now();
        let tz = self.tz();
        let mut upcoming: Vec<_> = self
            .events(today, today + Duration::days(UP_NEXT_DAYS), cx)
            .into_iter()
            .filter(|e| {
                if e.task {
                    !e.completed && e.date() >= today
                } else {
                    e.end > now && current.is_none_or(|c| c.id != e.id || c.start != e.start)
                }
            })
            .collect();
        // Within a day, all-day items and tasks come before timed events.
        upcoming.sort_by_key(|e| {
            let day = e.dates_in(tz).0.max(today);
            (day, e.all_day.is_none() && !e.task, e.start)
        });
        upcoming.truncate(UP_NEXT_ITEMS);
        let mut list = div().child(Self::side_label("Up next", cx));
        if upcoming.is_empty() && current.is_none() {
            list = list.child(
                div()
                    .px(rems(0.625))
                    .text_size(rems(0.75))
                    .text_color(t.muted())
                    .child("Nothing else this week."),
            );
        }
        let mut last_day = None;
        for (i, e) in upcoming.into_iter().enumerate() {
            let day = e.dates_in(tz).0.max(today);
            if last_day != Some(day) {
                last_day = Some(day);
                list = list.child(
                    div()
                        .px(rems(0.625))
                        .pt(rems(0.5))
                        .pb(rems(0.125))
                        .text_size(rems(0.69))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.faint())
                        .child(match (day - today).num_days() {
                            0 => "Today".to_string(),
                            1 => "Tomorrow".to_string(),
                            _ => day.format("%a %-d %b").to_string(),
                        }),
                );
            }
            let color = self.calendar_color(&e.calendar, cx);
            let note = if e.task {
                "Task".to_string()
            } else if e.all_day.is_some() {
                "All day".to_string()
            } else {
                e.start.with_timezone(&tz).format("%H:%M").to_string()
            };
            let row = Self::side_row(("upcoming", i), self.is_selected(&e), cx)
                .debug_selector(move || format!("upcoming-{i}"))
                .child(Self::side_dot(color, true))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(e.title.clone()),
                )
                .child(
                    div()
                        .text_size(rems(0.75))
                        .text_color(t.muted())
                        .child(note),
                );
            let pick = e.clone();
            list = list.child(self.anchor_event(row, &e, cx).on_click(
                cx.listener(move |app, _, window, cx| app.pick_event(pick.clone(), window, cx)),
            ));
        }
        list.into_any_element()
    }
    /// Calendar sets as tabs, then the calendars in the active set by account.
    fn calendar_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut list = div().child(Self::side_label("Calendars", cx));
        if !self.snapshot.sets.is_empty() {
            let tabs = std::iter::once((None, "All".to_string()))
                .chain(
                    self.snapshot
                        .sets
                        .iter()
                        .map(|s| (Some(s.id.clone()), s.name.clone())),
                )
                .enumerate()
                .map(|(i, (id, name))| {
                    pill_tab(("calendar-set", i), name, self.active_set == id, cx)
                        .debug_selector(move || format!("calendar-set-{i}"))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.active_set = id.clone();
                            app.remember("set", id.clone().unwrap_or_default());
                            app.selected = None;
                            window.focus(&app.focus, cx);
                            cx.notify();
                        }))
                });
            list = list.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(rems(0.125))
                    .px(rems(0.25))
                    .pb(rems(0.25))
                    .children(tabs),
            );
        }
        // With a set active, only its calendars are listed.
        let set = self
            .active_set
            .as_ref()
            .and_then(|id| self.snapshot.sets.iter().find(|s| &s.id == id));
        let week = week_start(self.date);
        let shown = self.events(week, week + Duration::days(7), cx);
        let groups = calendar_groups(&self.snapshot);
        let labelled = groups.iter().filter(|(_, i)| !i.is_empty()).count() > 1;
        for (group_index, (account, indices)) in groups.into_iter().enumerate() {
            let indices: Vec<_> = indices
                .into_iter()
                .filter(|&i| {
                    set.is_none_or(|s| s.calendars.contains(&self.snapshot.calendars[i].id))
                })
                .collect();
            if indices.is_empty() {
                continue;
            }
            // Accounts are only worth naming once there is more than one.
            if labelled {
                list = list.child(
                    div()
                        .debug_selector(move || format!("calendar-group-{group_index}"))
                        .px(rems(0.625))
                        .pt(rems(0.5))
                        .pb(rems(0.125))
                        .text_size(rems(0.69))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.faint())
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(account.unwrap_or_else(|| "On this computer".into())),
                );
            }
            for i in indices {
                let c = self.snapshot.calendars[i].clone();
                let visible = self.visible(&c.id);
                let color = self.calendar_color(&c.id, cx);
                let count = shown.iter().filter(|e| e.calendar == c.id).count();
                list = list.child(
                    Self::side_row(("calendar-row", i), false, cx)
                        .debug_selector(move || format!("calendar-row-{i}"))
                        .accessibility_label(format!(
                            "{} calendar, {}",
                            c.display_name(),
                            if visible { "visible" } else { "hidden" }
                        ))
                        .text_color(if visible { t.foreground } else { t.faint() })
                        .child(Self::side_dot(color, visible))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(c.name.clone()),
                        )
                        .children((!c.writable).then(|| {
                            div()
                                .text_size(rems(0.66))
                                .text_color(t.faint())
                                .child("read-only")
                        }))
                        .children((visible && count > 0).then(|| {
                            div()
                                .text_size(rems(0.75))
                                .text_color(t.muted())
                                .child(count.to_string())
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
            Self::side_row("sidebar-accounts", false, cx)
                .debug_selector(|| "sidebar-accounts".into())
                .text_color(t.muted())
                .child(
                    div()
                        .w(rems(1.))
                        .flex()
                        .justify_center()
                        .child(icon(IconName::Plus).size(rems(0.875))),
                )
                .child("Add Google account")
                .on_click(cx.listener(|app, _, window, cx| app.show_settings(window, cx))),
        )
        .into_any_element()
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
            .child({
                let current = self.current_event(cx);
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.mini_month(cx))
                    .child(
                        div()
                            .px(rems(0.75))
                            .pb(rems(0.75))
                            .children(current.as_ref().map(|e| self.now_card(e, cx)))
                            .child(self.up_next(current.as_ref(), cx))
                            .child(self.calendar_list(cx)),
                    )
            })
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

/// Up next looks this many days ahead and lists at most this many items.
const UP_NEXT_DAYS: i64 = 4;
const UP_NEXT_ITEMS: usize = 3;

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
