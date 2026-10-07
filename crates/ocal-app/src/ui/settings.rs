//! Settings and keyboard help pages.
use super::*;
use crate::appearance::{section_label, swatch};
use gpui_omarchy::{combobox, keycaps};

/// Keys pressed together, and what they do.
type Shortcut<'a> = (&'a [&'a str], &'a str);

/// A bordered settings section under an uppercase label.
fn section(label: &'static str, cx: &App) -> gpui_kit::Div {
    div().child(section_label(label, cx).px(rems(0.)))
}
fn card(cx: &App) -> gpui_kit::Div {
    div()
        .border_1()
        .border_color(cx.omarchy().divider())
        .p(rems(1.))
        .flex()
        .flex_col()
        .gap(rems(0.75))
}
fn hint(text: &'static str, cx: &App) -> gpui_kit::Div {
    div()
        .text_color(cx.omarchy().secondary)
        .text_size(rems(0.73))
        .whitespace_normal()
        .child(text)
}

impl Ocal {
    fn page(
        &self,
        id: &'static str,
        title: &'static str,
        back: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let t = cx.omarchy().clone();
        div().id(id).flex_1().min_h_0().overflow_y_scroll().child(
            div()
                .max_w(rems(42.))
                .px(rems(2.))
                .py(rems(1.5))
                .flex()
                .flex_col()
                .gap(rems(0.5))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .text_size(rems(1.4))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.bright)
                                .child(title),
                        )
                        .child(self.back_button(back, cx)),
                ),
        )
    }
    fn back_button(&self, id: &'static str, cx: &mut Context<Self>) -> gpui_omarchy::Button {
        action_button(
            id,
            IconName::ArrowLeft,
            "Back to calendar",
            ButtonVariant::Secondary,
            cx,
        )
        .debug_selector(move || id.into())
        .on_click(cx.listener(|app, _, window, cx| {
            app.settings = false;
            app.help = false;
            window.focus(&app.focus, cx);
            cx.notify();
        }))
    }
    fn accounts_section(&self, cx: &mut Context<Self>) -> gpui_kit::Div {
        let t = cx.omarchy().clone();
        let google_ready = ocal_core::oauth::ClientConfig::bundled().is_ok();
        let mut body = card(cx);
        for (i, account) in self.snapshot.accounts.iter().enumerate() {
            let count = self
                .snapshot
                .calendars
                .iter()
                .filter(|calendar| calendar.account.as_deref() == Some(account.id.as_str()))
                .count();
            body = body.child(
                div()
                    .debug_selector(move || format!("google-account-{i}"))
                    .flex()
                    .items_center()
                    .gap(rems(0.6))
                    .pb(rems(0.6))
                    .border_b_1()
                    .border_color(t.divider())
                    .child(icon(IconName::Globe).size(rems(0.9)).text_color(t.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_color(t.bright)
                                    .text_ellipsis()
                                    .child(account.id.clone()),
                            )
                            .child(div().text_color(t.secondary).text_size(rems(0.7)).child(
                                format!(
                                    "Google · {count} {}",
                                    if count == 1 { "calendar" } else { "calendars" }
                                ),
                            )),
                    ),
            );
        }
        body = body
            .child(
                div().flex().child(
                    action_button(
                        "connect-google",
                        IconName::Plus,
                        if self.busy {
                            "Connecting…"
                        } else if self.snapshot.accounts.is_empty() {
                            "Copy Google sign-in link"
                        } else {
                            "Add Google account"
                        },
                        ButtonVariant::Primary,
                        cx,
                    )
                    .debug_selector(|| "connect-google".into())
                    .disabled(self.demo || self.busy || !google_ready)
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.busy = true;
                        app.error = false;
                        app.sign_in_url = None;
                        app.cancel_sign_in.store(false, Ordering::Relaxed);
                        app.status = "Preparing Google sign-in link…".into();
                        app.dispatch(Command::Connect);
                        cx.notify();
                    })),
                ),
            )
            .child(hint(
                if self.demo {
                    "This demo is isolated. Run ocal without --demo to connect an account."
                } else if !google_ready {
                    "Google sign-in isn’t available in this development build. Local calendars work immediately."
                } else {
                    "Copy the sign-in link, paste it into your preferred browser profile, and choose an account. Keep Ocal open while signing in. Credentials stay in the desktop keyring."
                },
                cx,
            ))
            .children(self.sign_in_url.as_ref().map(|url| {
                let copy_url = url.clone();
                let open_url = url.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(0.5))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(rems(0.5))
                            .child(
                                action_button(
                                    "copy-sign-in-link",
                                    IconName::Copy,
                                    "Copy again",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .debug_selector(|| "copy-sign-in-link".into())
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_url.clone(),
                                    ));
                                    app.status = "Sign-in link copied · Paste it into your preferred browser profile".into();
                                    cx.notify();
                                })),
                            )
                            .child(
                                button(
                                    "open-sign-in-link",
                                    "Open default browser",
                                    ButtonVariant::Secondary,
                                    cx,
                                )
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    if std::process::Command::new("xdg-open")
                                        .arg(&open_url)
                                        .spawn()
                                        .is_err()
                                    {
                                        app.status = "Could not open the browser. Use Copy again and paste the link into your browser".into();
                                        cx.notify();
                                    }
                                })),
                            )
                            .child(
                                button("cancel-sign-in", "Cancel", ButtonVariant::Secondary, cx)
                                    .debug_selector(|| "cancel-sign-in".into())
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.cancel_sign_in.store(true, Ordering::Relaxed);
                                        app.sign_in_url = None;
                                        app.status = "Cancelling sign-in…".into();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(hint(
                        "This link stays active for 10 minutes. Copy again uses the same sign-in attempt.",
                        cx,
                    ))
            }))
            .children((self.busy || self.error).then(|| {
                div()
                    .id("account-status")
                    .debug_selector(|| "account-status".into())
                    .w_full()
                    .p(rems(0.7))
                    .border_1()
                    .border_l_2()
                    .border_color(if self.error { t.danger } else { t.divider() })
                    .text_color(if self.error { t.danger } else { t.secondary })
                    .whitespace_normal()
                    .child(self.status.clone())
            }))
            .child(
                div().flex().child(
                    action_button(
                        "advanced-login",
                        if self.advanced_login {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        "Advanced connection options",
                        ButtonVariant::Secondary,
                        cx,
                    )
                    .debug_selector(|| "advanced-login".into())
                    .border_0()
                    .bg(t.foreground.opacity(0.))
                    .text_color(t.secondary)
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.advanced_login = !app.advanced_login;
                        cx.notify();
                    })),
                ),
            );
        if self.advanced_login {
            body = body
                .child(hint(
                    "For developers and custom builds: use your own Google Desktop app client file.",
                    cx,
                ))
                .child(
                    div().flex().child(
                        button(
                            "connect-google-custom",
                            "Use a custom client file…",
                            ButtonVariant::Secondary,
                            cx,
                        )
                        .debug_selector(|| "connect-google-custom".into())
                        .disabled(self.demo || self.busy)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.busy = true;
                            app.error = false;
                            app.sign_in_url = None;
                            app.cancel_sign_in.store(false, Ordering::Relaxed);
                            app.status = "Choose a Google client file…".into();
                            app.dispatch(Command::ConnectCustom);
                            cx.notify();
                        })),
                    ),
                );
        }
        section("ACCOUNTS", cx).child(body)
    }
    fn sets_section(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::Div {
        let t = cx.omarchy().clone();
        let mut body = card(cx);
        for (i, set) in self.snapshot.sets.iter().enumerate() {
            let id = set.id.clone();
            let names: Vec<_> = set
                .calendars
                .iter()
                .filter_map(|c| self.snapshot.calendars.iter().find(|x| &x.id == c))
                .collect();
            body = body.child(
                div()
                    .debug_selector(move || format!("set-{i}"))
                    .flex()
                    .items_center()
                    .gap(rems(0.6))
                    .child(
                        icon(IconName::Layers)
                            .size(rems(0.9))
                            .text_color(t.secondary),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_color(t.bright).child(set.name.clone()))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(rems(0.5))
                                    .text_size(rems(0.7))
                                    .text_color(t.secondary)
                                    .children(names.into_iter().map(|c| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(rems(0.3))
                                            .child(swatch(self.calendar_color(&c.id, cx), true))
                                            .child(c.name.clone())
                                    })),
                            ),
                    )
                    .child(
                        tool_button(("delete-set", i), IconName::Trash, "Remove set", "", cx)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if app.active_set.as_deref() == Some(id.as_str()) {
                                    app.active_set = None;
                                    app.remember("set", "");
                                }
                                app.dispatch(Command::DeleteSet(id.clone()));
                                cx.notify();
                            })),
                    ),
            );
        }
        body = body
            .child(hint(
                "Save the calendars currently checked in the sidebar as a set, then switch between sets from the sidebar.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap(rems(0.5))
                    .child(
                        input("set-name", &self.set_name, window, cx)
                            .flex_1()
                            .border_1()
                            .border_color(t.divider())
                            .bg(t.background),
                    )
                    .child(
                        button("save-set", "Save set", ButtonVariant::Secondary, cx)
                            .debug_selector(|| "save-set".into())
                            .on_click(cx.listener(|app, _, window, cx| {
                                let name = app.set_name.read(cx).value().trim().to_owned();
                                if name.is_empty() {
                                    app.status = "Name your calendar set".into();
                                    app.error = true;
                                } else {
                                    app.dispatch(Command::SaveSet(CalendarSet {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        name,
                                        calendars: app
                                            .snapshot
                                            .calendars
                                            .iter()
                                            .filter(|c| c.visible)
                                            .map(|c| c.id.clone())
                                            .collect(),
                                    }));
                                    app.set_name.update(cx, |s, cx| s.set_value("", window, cx));
                                }
                                cx.notify();
                            })),
                    ),
            );
        section("CALENDAR SETS", cx).child(body)
    }
    fn display_section(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::Div {
        let t = cx.omarchy().clone();
        let row = |label: &'static str, content: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(rems(1.))
                .child(
                    div()
                        .w(rems(7.))
                        .flex_shrink_0()
                        .text_color(t.secondary)
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(content))
        };
        let zoom = div()
            .flex()
            .items_center()
            .gap(rems(0.4))
            .child(
                tool_button("zoom-out", IconName::Minus, "Smaller", "Ctrl −", cx).on_click(
                    cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(ZoomOut), cx)),
                ),
            )
            .child(
                div()
                    .w(rems(3.))
                    .text_center()
                    .text_color(t.bright)
                    .child(format!("{:.0}%", self.zoom * 100.)),
            )
            .child(
                tool_button("zoom-in", IconName::Plus, "Larger", "Ctrl +", cx).on_click(
                    cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(ZoomIn), cx)),
                ),
            );
        let body = card(cx)
            .child(row(
                "Time zone",
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(0.3))
                    .child(combobox("display-zone", &self.zone_choice, window, cx))
                    .child(hint(
                        "Events are shown in this zone. Each event keeps its own zone when edited.",
                        cx,
                    ))
                    .into_any_element(),
            ))
            .child(row("Interface size", zoom.into_any_element()))
            .child(row("Weeks start", div().child("Sunday").into_any_element()))
            .child(row(
                "Theme",
                div()
                    .text_color(t.foreground)
                    .child(format!("{} · follows Omarchy", t.name))
                    .into_any_element(),
            ));
        section("DISPLAY", cx).child(body)
    }
    pub(super) fn settings_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let page = self.page("settings-scroll", "Settings", "back-to-calendar", cx);
        let about = section("ABOUT", cx).child(
            card(cx)
                .text_size(rems(0.75))
                .text_color(t.secondary)
                .child(format!("Ocal {}", env!("CARGO_PKG_VERSION")))
                .child(if self.demo {
                    "Demo mode: nothing is saved to disk.".to_string()
                } else {
                    "Calendars are cached in ~/.local/share/ocal; Google credentials stay in the desktop keyring.".to_string()
                }),
        );
        page.child(
            div()
                .max_w(rems(42.))
                .px(rems(2.))
                .pb(rems(2.))
                .flex()
                .flex_col()
                .gap(rems(0.5))
                .child(self.accounts_section(cx))
                .child(self.sets_section(window, cx))
                .child(self.display_section(window, cx))
                .child(about),
        )
        .into_any_element()
    }
    pub(super) fn help_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.omarchy().clone();
        let groups: [(&str, &[Shortcut]); 4] = [
            (
                "NAVIGATE",
                &[
                    (
                        &["↑", "↓"],
                        "Move the time cursor 15 minutes; crossing an event highlights it",
                    ),
                    (&["←", "→"], "Previous / next day"),
                    (&["[", "]"], "Previous / next period"),
                    (&["t"], "Go to today"),
                    (&["/"], "Search two months back and four ahead"),
                    (&["esc"], "Close a panel, clear the range or the search"),
                ],
            ),
            (
                "CREATE",
                &[
                    (&["c"], "New event at the cursor"),
                    (
                        &["shift", "↑", "↓"],
                        "Extend a range; ↵ opens the editor for it",
                    ),
                    (&["↵"], "Open the highlighted event, or add at the cursor"),
                    (&["ctrl", "↵"], "Save the event being edited"),
                    (&["↑", "↓"], "In a time field: step 15 minutes"),
                ],
            ),
            (
                "VIEWS",
                &[
                    (&["1"], "Day"),
                    (&["2"], "Week"),
                    (&["3"], "Month"),
                    (&["4"], "Agenda"),
                    (&["5"], "Tasks"),
                    (&["b"], "Show or hide calendars"),
                ],
            ),
            (
                "APP",
                &[
                    (&["shift", "f10"], "Actions for the selection"),
                    (&["ctrl", "r"], "Sync Google calendars"),
                    (&["ctrl", "+"], "Larger interface"),
                    (&["ctrl", "−"], "Smaller interface"),
                    (&["ctrl", "0"], "Reset interface size"),
                    (&["?"], "This page"),
                ],
            ),
        ];
        let mut body = div().flex().flex_col().gap(rems(0.5));
        for (label, rows) in groups {
            body = body.child(
                section(label, cx).child(div().border_1().border_color(t.divider()).children(
                    rows.iter().map(|(keys, text)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(1.))
                            .px(rems(0.8))
                            .py(rems(0.45))
                            .border_b_1()
                            .border_color(t.divider().opacity(0.6))
                            .child(
                                div()
                                    .w(rems(8.))
                                    .flex_shrink_0()
                                    .child(keycaps(keys.iter().copied(), cx)),
                            )
                            .child(div().flex_1().min_w_0().whitespace_normal().child(*text))
                    }),
                )),
            );
        }
        self.page("help-scroll", "Keyboard", "close-help", cx)
            .child(div().max_w(rems(42.)).px(rems(2.)).pb(rems(2.)).child(body))
            .into_any_element()
    }
}
