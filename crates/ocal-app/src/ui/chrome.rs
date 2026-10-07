//! Header row and status bar.
use super::*;
use gpui_omarchy::{keycaps, with_tooltip};

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
            View::Week if !compact => {
                let first = week_start(self.date);
                let last = first + Duration::days(6);
                if first.month() == last.month() {
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

    pub(super) fn header(
        &self,
        layout: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let searching = !self.search.read(cx).value().is_empty() || self.search_open;
        let inline_search = layout.wide || searching;
        let (title, year) = self.period_title(layout.compact);
        let brand = div()
            .w(rems(SIDEBAR - 0.6))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rems(0.55))
            .child(
                icon(IconName::CalendarRange)
                    .size(rems(1.))
                    .text_color(t.accent),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(rems(0.9))
                    .text_color(t.bright)
                    .child("Ocal"),
            )
            .child(div().flex_1())
            .child(
                tool_button(
                    "hide-sidebar",
                    IconName::PanelLeft,
                    "Hide calendars",
                    "B",
                    cx,
                )
                .on_click(cx.listener(|app, _, window, cx| app.toggle_sidebar(window, cx))),
            );
        let title = div()
            .flex()
            .items_baseline()
            .gap(rems(0.4))
            .min_w_0()
            .flex_shrink(1.)
            .overflow_hidden()
            .whitespace_nowrap()
            .debug_selector(|| "period-title".into())
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(rems(1.))
                    .text_color(t.bright)
                    .text_ellipsis()
                    .child(title),
            )
            .child(div().text_size(rems(1.)).text_color(t.accent).child(year));
        let mut search = Some(
            input("search", &self.search, window, cx)
                .debug_selector(|| "search".into())
                .w(rems(if layout.wide { 16. } else { 14. }))
                .min_w(rems(8.))
                .flex_shrink(1.)
                .py(rems(0.22))
                .border_1()
                .border_color(t.divider())
                .bg(t.background),
        );
        let title_search = searching && !layout.wide;
        div()
            .h(rems(HEADER))
            .flex_shrink_0()
            .px(rems(0.6))
            .gap(rems(0.35))
            .border_b_1()
            .border_color(t.divider())
            .bg(t.inset)
            .flex()
            .items_center()
            .map(|row| {
                if layout.sidebar && !self.sidebar_hidden {
                    row.child(brand)
                } else {
                    row.child(
                        tool_button("toggle-sidebar", IconName::PanelLeft, "Calendars", "B", cx)
                            .on_click(
                                cx.listener(|app, _, window, cx| app.toggle_sidebar(window, cx)),
                            ),
                    )
                }
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
            .child(
                button("today", "Today", ButtonVariant::Secondary, cx)
                    .h(rems(1.7))
                    .px(rems(0.6))
                    .border_1()
                    .border_color(t.divider())
                    .bg(t.foreground.opacity(0.))
                    .on_click(cx.listener(|app, _, _, cx| app.go_today(cx))),
            )
            .child(
                tool_button("next", IconName::ChevronRight, "Next period", "]", cx)
                    .on_click(cx.listener(|app, _, _, cx| app.navigate(1, cx))),
            )
            .child(div().w(rems(0.3)).flex_shrink_0())
            .map(|row| {
                // Narrow windows trade the title for the search field while searching.
                if title_search {
                    row.children(search.take().map(|s| s.flex_1()))
                } else {
                    row.child(title).child(div().flex_1().min_w(rems(0.5)))
                }
            })
            .child(self.view_switch(layout.compact, cx))
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
            .child(
                tool_button("new-event", IconName::Plus, "New event", "C", cx)
                    .on_click(cx.listener(|app, _, window, cx| app.new_event(false, window, cx))),
            )
            .child(
                tool_button("sync", IconName::RefreshCw, "Sync calendars", "Ctrl R", cx)
                    .disabled(self.busy)
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.dispatch(Command::Sync(app.date));
                        app.status = "Syncing…".into();
                        app.error = false;
                        cx.notify();
                    })),
            )
            .child(self.app_menu(cx))
            .into_any_element()
    }

    /// Segmented Day/Week/Month/Agenda/Tasks control; a menu in compact windows.
    fn view_switch(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        if compact {
            let target = cx.entity().downgrade();
            return menu(
                "view-menu",
                button(
                    "view-trigger",
                    self.view.label(),
                    ButtonVariant::Secondary,
                    cx,
                )
                .h(rems(1.7))
                .border_0()
                .bg(t.foreground.opacity(0.))
                .child(icon(IconName::ChevronDown).size(rems(0.75))),
                VIEWS
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        MenuItem::new(v.label())
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
            .h(rems(1.7))
            .border_1()
            .border_color(t.divider())
            .children(VIEWS.into_iter().enumerate().map(|(i, v)| {
                let active = v == self.view;
                with_tooltip(
                    button(("view", i), v.label(), ButtonVariant::Secondary, cx)
                        .debug_selector(move || format!("view-{i}"))
                        .h_full()
                        .px(rems(0.6))
                        .border_0()
                        .text_size(rems(0.74))
                        .relative()
                        .bg(if active {
                            t.selection
                        } else {
                            t.foreground.opacity(0.)
                        })
                        .text_color(if active { t.bright } else { t.secondary })
                        .hover(|s| s.bg(t.hover_fill()).text_color(t.bright))
                        .children(active.then(|| {
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(2.))
                                .bg(t.accent)
                        }))
                        .on_click(
                            cx.listener(move |app, _, window, cx| app.change_view(v, window, cx)),
                        ),
                    format!("{} · {}", v.label(), i + 1),
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
                        _ => {
                            app.help = true;
                            app.settings = false;
                            app.selected = None;
                        }
                    }
                    cx.notify();
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn status_bar(
        &self,
        layout: Layout,
        cursor_event: Option<&Event>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let hint = |keys: Vec<&'static str>, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(rems(0.35))
                .child(keycaps(keys, cx))
                .child(label)
        };
        let grid = matches!(self.view, View::Day | View::Week) && !self.settings && !self.help;
        div()
            .h(rems(STATUS))
            .flex_shrink_0()
            .px(rems(0.9))
            .border_t_1()
            .border_color(t.divider())
            .bg(t.inset)
            .flex()
            .items_center()
            .gap(rems(0.9))
            .text_size(rems(0.68))
            .child(
                div()
                    .debug_selector(|| "status".into())
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(if self.error { t.danger } else { t.secondary })
                    .child(self.status.clone()),
            )
            .children((grid && !layout.compact).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.9))
                    .text_color(t.secondary)
                    .child(div().text_color(t.foreground).child(self.selection.label()))
                    .child(hint(vec!["↑", "↓"], "move"))
                    .child(hint(vec!["shift"], "range"))
                    .child(hint(
                        vec!["↵"],
                        if cursor_event.is_some() {
                            "open"
                        } else {
                            "add"
                        },
                    ))
                    .child(hint(vec!["?"], "keys"))
            }))
            .into_any_element()
    }
}
