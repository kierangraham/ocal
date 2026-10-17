//! The sheet's toolbar.
use super::*;
use crate::appearance::{
    RADIUS_FIELD, Tones, inset_ring, pill_tab, separator, soft_keycap, tip_text, with_tip,
};

const VIEWS: [View; 5] = [
    View::Day,
    View::Week,
    View::Month,
    View::Agenda,
    View::Tasks,
];

impl Ocal {
    /// The visible period, e.g. "October 2026" or "28 Sep – 4 Oct 2026", split so
    /// the year can be drawn in the accent colour.
    pub(super) fn period_title(&self, compact: bool) -> (String, String) {
        let year = |d: NaiveDate| d.year().to_string();
        match self.view {
            View::Day => (
                self.date
                    .format(if compact { "%a %-d %b" } else { "%A %-d %B" })
                    .to_string(),
                year(self.date),
            ),
            View::Week => {
                let days = self.visible_days(compact);
                let (first, last) = (days[0], days[days.len() - 1]);
                if compact {
                    // "6 – 8 Oct", or "30 Sep – 2 Oct" across months.
                    let start = if first.month() == last.month() {
                        first.format("%-d").to_string()
                    } else {
                        first.format("%-d %b").to_string()
                    };
                    (format!("{start} – {}", last.format("%-d %b")), year(last))
                } else if first.month() == last.month() {
                    (first.format("%B").to_string(), year(first))
                } else if first.year() == last.year() {
                    (
                        format!("{} – {}", first.format("%b"), last.format("%b")),
                        year(first),
                    )
                } else {
                    (
                        format!("{} – {}", first.format("%b %Y"), last.format("%b")),
                        year(last),
                    )
                }
            }
            View::Tasks => ("Tasks".into(), String::new()),
            _ => (self.date.format("%B").to_string(), year(self.date)),
        }
    }

    /// A short note beside the title: the ISO week, "Today", or the open task count.
    pub(super) fn period_note(&self) -> Option<String> {
        match self.view {
            // Weeks start on Sunday; the Monday inside the week names it.
            View::Week => Some(format!(
                "Week {}",
                (week_start(self.date) + Duration::days(1))
                    .iso_week()
                    .week()
            )),
            View::Day => match (self.date - self.today()).num_days() {
                0 => Some("Today".into()),
                1 => Some("Tomorrow".into()),
                _ => None,
            },
            View::Tasks => Some(format!(
                "{} to do",
                self.snapshot
                    .events
                    .iter()
                    .filter(|e| e.task && !e.completed && self.visible(&e.calendar))
                    .count()
            )),
            _ => None,
        }
    }

    /// Whether the period in view already contains today, so Today would do nothing.
    pub(super) fn showing_today(&self) -> bool {
        let today = self.today();
        match self.view {
            View::Day | View::Agenda => self.date == today,
            View::Week => self.visible_days(self.compact).contains(&today),
            View::Month => (self.date.year(), self.date.month()) == (today.year(), today.month()),
            View::Tasks => true,
        }
    }

    /// The sheet's toolbar: sidebar toggle, period, navigation, views, search and actions.
    pub(super) fn toolbar(
        &self,
        layout: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let searching = !self.search.read(cx).value().is_empty() || self.search_open;
        let inline_search = layout.wide || searching;
        let docked = layout.sidebar && !self.sidebar_hidden;
        // The sheet's width in rems: below about 58 the toolbar has no room for the
        // week note beside the title.
        let sheet = window.viewport_size().width / window.rem_size()
            - if docked { SIDEBAR } else { 0. }
            - 2. * SHEET_GAP;
        let (title, mut year) = self.period_title(layout.compact);
        let title_size = rems(if layout.compact { 1. } else { 1.25 });
        // Narrow windows only name the year when it isn't this one.
        if layout.compact && year == self.today().year().to_string() {
            year.clear();
        }
        let title = div()
            .ml(rems(0.25))
            .flex()
            .items_baseline()
            .gap(rems(0.4))
            .min_w_0()
            // The search field gives up its width first; the title is the last resort.
            .flex_shrink(0.02)
            .overflow_hidden()
            .whitespace_nowrap()
            .debug_selector(|| "period-title".into())
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(title_size)
                    .text_color(t.bright)
                    .text_ellipsis()
                    .child(title),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(title_size)
                    .text_color(t.accent)
                    .child(year),
            )
            .children(
                self.period_note()
                    .filter(|_| !layout.compact && sheet >= 58.)
                    .map(|note| {
                        div()
                            .flex_shrink_0()
                            .pl(rems(0.1))
                            .text_size(rems(0.75))
                            .text_color(t.muted())
                            .debug_selector(|| "period-note".into())
                            .child(note)
                    }),
            );
        let search_focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        let mut search = Some(
            input("search", &self.search, window, cx)
                .debug_selector(|| "search".into())
                .prefix(icon(IconName::Search).size(rems(1.)).text_color(t.muted()))
                .suffix(soft_keycap("/", false, cx))
                .w(rems(13.75))
                .min_w(rems(8.))
                .flex_shrink(1.)
                .h(rems(2.25))
                .py(rems(0.))
                .pl(rems(0.6875))
                .pr(rems(0.625))
                .gap(rems(0.5625))
                .border_0()
                .rounded(RADIUS_FIELD)
                .text_size(rems(0.8125))
                .bg(t.hover())
                .when(search_focused, |s| s.shadow(inset_ring(t.accent, 1.5))),
        );
        let title_search = searching && !layout.wide;
        div()
            .h(rems(TOOLBAR))
            .flex_shrink_0()
            .pl(rems(if layout.compact { 0.5 } else { 0.75 }))
            .pr(rems(if layout.compact { 0.5 } else { 0.875 }))
            .gap(rems(0.125))
            .flex()
            .items_center()
            .child(
                tool_button(
                    "toggle-sidebar",
                    IconName::PanelLeft,
                    if docked || self.sidebar_open {
                        "Hide sidebar"
                    } else {
                        "Show sidebar"
                    },
                    "b",
                    cx,
                )
                .on_click(cx.listener(|app, _, window, cx| app.toggle_sidebar(window, cx))),
            )
            .map(|row| {
                // Narrow windows trade the title for the search field while searching.
                if title_search {
                    row.children(search.take().map(|s| s.flex_1().ml(rems(0.25))))
                } else {
                    row.child(title)
                }
            })
            // Compact windows need the separators' room for the title.
            .when(!layout.compact, |row| {
                row.child(separator(cx).ml(rems(0.875)))
            })
            .child(
                tool_button(
                    "previous",
                    IconName::ChevronLeft,
                    "Previous period",
                    "[",
                    cx,
                )
                .on_click(cx.listener(|app, _, _, cx| app.navigate(-1, cx))),
            )
            .child(if layout.compact {
                tool_button("today", IconName::CalendarCheck, "Today", "t", cx)
                    .on_click(cx.listener(|app, _, _, cx| app.go_today(cx)))
                    .into_any_element()
            } else {
                let current = self.showing_today();
                with_tip(
                    pill_tab("today", "Today", false, cx)
                        .debug_selector(|| "today".into())
                        .text_color(if current { t.muted() } else { t.bright })
                        .when(!current, |b| b.font_weight(FontWeight::MEDIUM))
                        .hover(|s| s.bg(t.hover()).text_color(t.bright))
                        .on_click(cx.listener(|app, _, _, cx| app.go_today(cx))),
                    tip_text("Today", "t"),
                )
                .into_any_element()
            })
            .child(
                tool_button("next", IconName::ChevronRight, "Next period", "]", cx)
                    .on_click(cx.listener(|app, _, _, cx| app.navigate(1, cx))),
            )
            .child(div().flex_1().min_w(rems(0.5)))
            .child(self.view_switch(layout.compact, cx))
            .when(!layout.compact, |row| row.child(separator(cx)))
            .map(|row| {
                if inline_search {
                    row.children(search.take())
                } else {
                    row.child(
                        tool_button("open-search", IconName::Search, "Search", "/", cx).on_click(
                            cx.listener(|app, _, window, cx| app.open_search(window, cx)),
                        ),
                    )
                }
            })
            // The sidebar's New event button replaces this while it is docked.
            .when(!docked, |row| {
                row.child(
                    tool_button("new-event", IconName::Plus, "New event", "c", cx).on_click(
                        cx.listener(|app, _, window, cx| app.new_event(false, window, cx)),
                    ),
                )
            })
            .child(
                tool_button("sync", IconName::RefreshCw, "Sync calendars", "ctrl+r", cx)
                    .when(!layout.compact, |b| b.ml(rems(0.375)))
                    .disabled(self.busy)
                    .on_click(cx.listener(|app, _, _, cx| app.sync_now(cx))),
            )
            .child(self.app_menu(cx))
            .into_any_element()
    }

    /// Day/Week/Month/Agenda/Tasks as pill tabs; a menu in compact windows.
    fn view_switch(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        if compact {
            let target = cx.entity().downgrade();
            return menu(
                "view-menu",
                pill_tab("view-trigger", self.view.compact_label(), true, cx)
                    .bg(t.hover())
                    .child(icon(IconName::ChevronDown).size(rems(0.75))),
                VIEWS
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        MenuItem::new(v.compact_label())
                            .checked(*v == self.view)
                            .shortcut((i + 1).to_string())
                    })
                    .collect(),
                move |index, window, cx| {
                    let _ = target.update(cx, |app, cx| app.change_view(VIEWS[index], window, cx));
                },
            )
            .into_any_element();
        }
        div()
            .flex()
            .flex_shrink_0()
            .gap(px(2.))
            .children(VIEWS.into_iter().enumerate().map(|(i, v)| {
                with_tip(
                    pill_tab(("view", i), v.label(), v == self.view, cx)
                        .debug_selector(move || format!("view-{i}"))
                        .on_click(
                            cx.listener(move |app, _, window, cx| app.change_view(v, window, cx)),
                        ),
                    tip_text(v.label(), &(i + 1).to_string()),
                )
            }))
            .into_any_element()
    }

    fn app_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = cx.entity().downgrade();
        menu(
            "app-menu",
            tool_button(
                "app-actions",
                IconName::Ellipsis,
                "Calendar options",
                "",
                cx,
            ),
            vec![
                MenuItem::new("Settings").icon(IconName::Settings),
                MenuItem::new("Import iCalendar…").icon(IconName::Upload),
                MenuItem::new("Export visible calendars…").icon(IconName::Download),
                MenuItem::new("Keyboard shortcuts")
                    .separator_before()
                    .icon(IconName::CircleQuestionMark)
                    .shortcut("?"),
            ],
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    if app.editor.is_some() {
                        return;
                    }
                    match index {
                        0 => app.show_settings(window, cx),
                        1 => app.dispatch(Command::Import),
                        2 => app.dispatch(Command::Export(
                            app.snapshot
                                .events
                                .iter()
                                .filter(|e| app.visible(&e.calendar))
                                .cloned()
                                .collect(),
                        )),
                        _ => app.show_help(window, cx),
                    }
                    cx.notify();
                });
            },
        )
        .into_any_element()
    }
}
