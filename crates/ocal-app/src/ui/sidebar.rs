//! Mini month, calendar list and upcoming events.
use super::*;

impl Ocal {
    pub(super) fn mini_month(
        &self,
        date: NaiveDate,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = cx.omarchy().clone();
        let cell = if compact { 1.6 } else { 1.85 };
        let mut grid = div().flex().flex_col().gap(rems(0.12));
        let today = Utc::now().with_timezone(&self.tz()).date_naive();
        for (week, days) in month_days(date).chunks(7).enumerate() {
            grid = grid.child(div().flex().children(days.iter().enumerate().map(|(i, d)| {
                let d = *d;
                let selected = d == self.date;
                let today = d == today;
                let mut b = button(
                    format!("mini-{}-{week}-{i}", date.format("%Y%m")),
                    d.day().to_string(),
                    ButtonVariant::Secondary,
                    cx,
                )
                .accessibility_label(d.format("%A %d %B %Y").to_string())
                .w(rems(cell))
                .h(rems(cell))
                .p(rems(0.))
                .border_0()
                .text_size(rems(0.7))
                .text_color(if today {
                    t.accent
                } else if d.month() == date.month() {
                    t.foreground
                } else {
                    t.secondary.opacity(0.5)
                })
                .bg(if selected {
                    t.selection
                } else {
                    t.foreground.opacity(0.)
                });
                if today {
                    b = b.font_weight(FontWeight::SEMIBOLD);
                }
                b.on_click(cx.listener(move |app, _, _, cx| app.choose_date(d, cx)))
            })));
        }
        let _ = window;
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb(rems(0.6))
                    .child(
                        div()
                            .text_size(rems(0.86))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(date.format("%B %Y").to_string()),
                    )
                    .when(!compact, |row| {
                        row.child(
                            tool_button(
                                "mini-prev",
                                IconName::ChevronLeft,
                                "Previous month",
                                "",
                                cx,
                            )
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.choose_date(move_month(app.date, -1), cx)
                            })),
                        )
                        .child(
                            tool_button("mini-next", IconName::ChevronRight, "Next month", "", cx)
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.choose_date(move_month(app.date, 1), cx)
                                })),
                        )
                    }),
            )
            .child(div().flex().pb(rems(0.3)).children(
                ["M", "T", "W", "T", "F", "S", "S"].into_iter().map(|s| {
                    div()
                        .w(rems(cell))
                        .text_center()
                        .text_size(rems(0.63))
                        .text_color(t.secondary)
                        .child(s)
                }),
            ))
            .child(grid)
            .into_any_element()
    }
    pub(super) fn calendar_sets(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = cx.entity().downgrade();
        let sets = self.snapshot.sets.clone();
        let active = self.active_set.clone();
        let mut items = vec![MenuItem::new("All calendars").checked(active.is_none())];
        items.extend(
            sets.iter()
                .map(|s| MenuItem::new(s.name.clone()).checked(active.as_ref() == Some(&s.id))),
        );
        menu(
            "calendar-sets",
            button("calendar-set-trigger", "", ButtonVariant::Secondary, cx)
                .border_0()
                .child(
                    self.active_set
                        .as_ref()
                        .and_then(|id| self.snapshot.sets.iter().find(|s| &s.id == id))
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "All calendars".into()),
                )
                .child(icon(IconName::ChevronDown).size(rems(0.8))),
            items,
            move |index, window, cx| {
                let _ = target.update(cx, |app, cx| {
                    app.active_set = if index == 0 {
                        None
                    } else {
                        sets.get(index - 1).map(|s| s.id.clone())
                    };
                    app.selected = None;
                    window.focus(&app.focus, cx);
                    cx.notify();
                });
            },
        )
        .into_any_element()
    }
    pub(super) fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let mut side = div()
            .w(rems(16.5))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(t.inset)
            .border_r_1()
            .border_color(t.divider())
            .child(
                div()
                    .p(rems(1.1))
                    .child(self.mini_month(self.date, false, window, cx)),
            )
            .child(
                div()
                    .px(rems(1.))
                    .pb(rems(0.7))
                    .child(self.calendar_sets(cx)),
            );
        let mut calendar_list = div()
            .id("sidebar-calendars")
            .min_h_0()
            .max_h(rems(18.))
            .overflow_y_scroll();
        for (group_index, (account, indices)) in
            calendar_groups(&self.snapshot).into_iter().enumerate()
        {
            calendar_list = calendar_list.child(
                div()
                    .debug_selector(move || format!("calendar-group-{group_index}"))
                    .px(rems(1.15))
                    .pt(rems(0.65))
                    .pb(rems(0.3))
                    .text_size(rems(0.63))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.secondary)
                    .text_ellipsis()
                    .child(account.unwrap_or_else(|| "Local calendars".into())),
            );
            for i in indices {
                let c = self.snapshot.calendars[i].clone();
                let visible = self.visible(&c.id);
                let color = self.calendar_color(&c.id, cx);
                calendar_list = calendar_list.child(
                    button(("calendar-row", i), "", ButtonVariant::Secondary, cx)
                        .debug_selector(move || format!("calendar-row-{i}"))
                        .accessibility_label(format!(
                            "{} calendar, {}",
                            c.display_name(),
                            if visible { "visible" } else { "hidden" }
                        ))
                        .w_full()
                        .border_0()
                        .px(rems(1.15))
                        .py(rems(0.4))
                        .gap(rems(0.65))
                        .justify_start()
                        .child(
                            icon(if visible {
                                IconName::SquareCheck
                            } else {
                                IconName::Square
                            })
                            .size(rems(0.9))
                            .text_color(color),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .text_size(rems(0.76))
                                .child(c.name.clone()),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            window.focus(&app.focus, cx);
                            let mut c = c.clone();
                            c.visible = !c.visible;
                            if let Some(current) =
                                app.snapshot.calendars.iter_mut().find(|x| x.id == c.id)
                            {
                                current.visible = c.visible;
                            }
                            app.active_set = None;
                            app.dispatch(Command::Visible(c));
                            cx.notify();
                        })),
                );
            }
        }
        side = side.child(calendar_list).child(
            action_button(
                "sidebar-accounts",
                IconName::Plus,
                "Add Google account",
                ButtonVariant::Secondary,
                cx,
            )
            .mx(rems(0.8))
            .mt(rems(0.4))
            .border_0()
            .debug_selector(|| "sidebar-accounts".into())
            .on_click(cx.listener(|app, _, _, cx| {
                app.settings = true;
                app.help = false;
                app.selected = None;
                app.sidebar_open = false;
                cx.notify();
            })),
        );
        let upcoming = self.events(self.date, self.date + Duration::days(5), cx);
        let mut agenda = div()
            .id("sidebar-agenda")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(rems(1.))
            .pt(rems(1.))
            .child(
                div()
                    .pb(rems(0.65))
                    .text_color(t.secondary)
                    .text_size(rems(0.62))
                    .child("UP NEXT"),
            );
        if upcoming.is_empty() {
            agenda = agenda.child(
                div()
                    .text_color(t.secondary)
                    .text_size(rems(0.74))
                    .child("A little breathing room."),
            );
        }
        let mut last = None;
        for (i, e) in upcoming.iter().take(15).enumerate() {
            let day = e.dates_in(self.tz()).0;
            if last != Some(day) {
                agenda = agenda.child(
                    div()
                        .pt(rems(0.6))
                        .pb(rems(0.35))
                        .text_size(rems(0.7))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(day.format("%a, %-d %b").to_string()),
                );
                last = Some(day);
            }
            let e = e.clone();
            let color = self.calendar_color(&e.calendar, cx);
            agenda =
                agenda.child(
                    button(("upcoming", i), "", ButtonVariant::Secondary, cx)
                        .border_0()
                        .w_full()
                        .justify_start()
                        .px(rems(0.3))
                        .py(rems(0.4))
                        .gap(rems(0.55))
                        .child(div().w(px(2.)).h(rems(1.8)).bg(color))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(rems(0.15))
                                .child(
                                    div()
                                        .text_size(rems(0.73))
                                        .text_ellipsis()
                                        .child(e.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(rems(0.63))
                                        .text_color(t.secondary)
                                        .child(e.time_label(self.tz())),
                                ),
                        )
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.pick_event(e.clone(), window, cx)
                        })),
                );
        }
        side = side.child(agenda).child(
            div()
                .border_t_1()
                .border_color(t.divider())
                .p(rems(0.65))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    action_button(
                        "settings",
                        IconName::Settings,
                        "Settings",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .border_0()
                    .on_click(cx.listener(|app, _, window, cx| {
                        if app.editor.is_none() {
                            app.settings = true;
                            app.help = false;
                            app.selected = None;
                            window.focus(&app.focus, cx);
                            cx.notify();
                        }
                    })),
                )
                .child(
                    tool_button(
                        "help",
                        IconName::CircleQuestionMark,
                        "Keyboard shortcuts",
                        "?",
                        cx,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.help = true;
                        cx.notify();
                    })),
                ),
        );
        side.into_any_element()
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
